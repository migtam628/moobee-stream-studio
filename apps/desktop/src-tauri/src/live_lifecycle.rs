//! Streaming lifecycle controller. The registered diagnostic uses no media,
//! devices, credentials or network. The internal native path uses the same
//! controller with a real Program capture worker.
use serde::Serialize;
use crate::live_health::{Monitor,Reading,Sample};
use std::{sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) enum Phase{Starting,Streaming,Reconnecting,Stopping,Stopped,Failed}
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) enum Mode{Rehearsal,Native}
/// Sender-only termination classification; never receiver acceptance.
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) enum NativeOutcome{SenderCompleted,StoppedEarly,Failed,Unverified}
#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Snapshot{
    session_id:String,mode:Mode,phase:Phase,native_outcome:Option<NativeOutcome>,elapsed_ms:u64,media_frames:u32,
    sent_packets:u64,sent_payload_bytes:u64,pending_packets:usize,pending_bytes:usize,
    late_frames:u32,bitrate_kbps:u64,error:Option<&'static str>,health:Reading,reconnect_attempt:u8,destinations:Vec<crate::live_fanout::DestinationStats>,
    resources:crate::live_soak::Resources,timing:Option<crate::live_soak::Timing>,bitrate_advice:crate::live_bitrate::Reading,
}
struct Entry{snapshot:Snapshot,start:Instant,stop:Arc<AtomicBool>,destination_stops:std::collections::HashMap<String,Arc<AtomicBool>>,monitor:Monitor,epoch_started_ms:u64,advisor:crate::live_bitrate::Advisor}
#[derive(Default)]
pub(crate) struct Controller{entry:Mutex<Option<Entry>>}
#[derive(Clone)]
pub(crate) struct Run{controller:Arc<Controller>,id:String,stop:Arc<AtomicBool>,retry_failure:Arc<Mutex<Option<crate::live_reconnect::Failure>>>}
impl Controller{
    pub(crate) fn start(self:&Arc<Self>,id:String,mode:Mode)->Result<Run,String>{
        if uuid::Uuid::parse_str(&id).map_or(true,|v|v.to_string()!=id){return Err("Invalid stream session ID".into())}
        let mut e=self.entry.lock().map_err(|_|"Stream state unavailable")?;
        if e.as_ref().is_some_and(|v|!matches!(v.snapshot.phase,Phase::Stopped|Phase::Failed)){return Err("Another streaming session is active".into())}
        let stop=Arc::new(AtomicBool::new(false));
        *e=Some(Entry{snapshot:Snapshot{session_id:id.clone(),mode,phase:Phase::Starting,native_outcome:None,elapsed_ms:0,media_frames:0,
            sent_packets:0,sent_payload_bytes:0,pending_packets:0,pending_bytes:0,late_frames:0,bitrate_kbps:0,error:None,health:Reading::default(),reconnect_attempt:0,destinations:vec![],resources:crate::live_soak::Resources::default(),timing:None,bitrate_advice:crate::live_bitrate::Reading::default()},start:Instant::now(),stop:stop.clone(),destination_stops:Default::default(),monitor:Monitor::default(),epoch_started_ms:0,advisor:crate::live_bitrate::Advisor::default()});
        Ok(Run{controller:self.clone(),id,stop,retry_failure:Arc::new(Mutex::new(None))})
    }
    pub(crate) fn current(&self)->Option<Snapshot>{
        let e=self.entry.lock().ok()?;
        let e=e.as_ref()?;
        let mut s=e.snapshot.clone();
        if !matches!(s.phase,Phase::Stopped|Phase::Failed){s.elapsed_ms=e.start.elapsed().as_millis() as u64;}
        s.health=e.monitor.reading(s.elapsed_ms,matches!(s.phase,Phase::Stopped|Phase::Failed));
        Some(s)
    }
    pub(crate) fn snapshot(&self,id:&str)->Option<Snapshot>{
        let e=self.entry.lock().ok()?;
        let e=e.as_ref().filter(|v|v.snapshot.session_id==id)?;
        let mut s=e.snapshot.clone();
        if !matches!(s.phase,Phase::Stopped|Phase::Failed){s.elapsed_ms=e.start.elapsed().as_millis() as u64;}
        s.health=e.monitor.reading(s.elapsed_ms,matches!(s.phase,Phase::Stopped|Phase::Failed));
        Some(s)
    }
    pub(crate) fn stop_rehearsal(&self,id:&str)->bool{
        let Ok(mut e)=self.entry.lock()else{return false};
        let Some(e)=e.as_mut().filter(|v|v.snapshot.session_id==id&&v.snapshot.mode==Mode::Rehearsal)else{return false};
        if matches!(e.snapshot.phase,Phase::Stopped|Phase::Failed){return false}
        e.stop.store(true,Ordering::Release);e.snapshot.phase=Phase::Stopping;true
    }
    pub(crate) fn stop_native(&self,id:&str)->bool{
        let Ok(mut e)=self.entry.lock()else{return false};
        let Some(e)=e.as_mut().filter(|v|v.snapshot.session_id==id&&v.snapshot.mode==Mode::Native)else{return false};
        if matches!(e.snapshot.phase,Phase::Stopped|Phase::Failed){return false}
        e.stop.store(true,Ordering::Release);e.snapshot.phase=Phase::Stopping;true
    }
    /// Only an existing, independently authorized multistream output may
    /// be retired. Never create a destination or change another session.
    pub(crate) fn stop_destination(&self,id:&str,destination:&str)->bool{
        let Ok(mut entry)=self.entry.lock()else{return false};
        let Some(e)=entry.as_mut().filter(|e|e.snapshot.session_id==id&&e.snapshot.mode==Mode::Native&&e.snapshot.phase==Phase::Streaming&&!e.stop.load(Ordering::Acquire))else{return false};
        let active=|d:&crate::live_fanout::DestinationStats|matches!(d.phase,"authorized"|"sending"|"reconnecting")&&e.destination_stops.get(&d.id).is_some_and(|flag|!flag.load(Ordering::Acquire));
        if e.snapshot.destinations.iter().filter(|d|active(d)).count()<2||
           !e.snapshot.destinations.iter().any(|d|d.id==destination&&active(d)){return false}
        let Some(flag)=e.destination_stops.get(destination)else{return false};
        flag.store(true,Ordering::Release);true
    }
    pub(crate) fn request_stop(&self,id:&str)->bool{
        let Ok(mut e)=self.entry.lock()else{return false};
        let Some(e)=e.as_mut().filter(|v|v.snapshot.session_id==id)else{return false};
        if matches!(e.snapshot.phase,Phase::Stopped|Phase::Failed){return false}
        e.stop.store(true,Ordering::Release);e.snapshot.phase=Phase::Stopping;true
    }
}
impl Run{
    pub(crate) fn native_active(&self)->bool{
        self.controller.entry.lock().ok().and_then(|entry|{
            entry.as_ref().filter(|e|e.snapshot.session_id==self.id&&Arc::ptr_eq(&e.stop,&self.stop))
                .map(|e|e.snapshot.mode==Mode::Native&&!matches!(e.snapshot.phase,Phase::Stopped|Phase::Failed|Phase::Stopping)&&!self.cancelled())
        }).unwrap_or(false)
    }
    pub(crate) fn register_destination_stops(&self,ids:Vec<String>)->Result<std::collections::HashMap<String,Arc<AtomicBool>>,String>{
        let mut entry=self.controller.entry.lock().map_err(|_|"Stream state unavailable")?;
        let e=entry.as_mut().filter(|e|e.snapshot.session_id==self.id&&Arc::ptr_eq(&e.stop,&self.stop)&&e.snapshot.mode==Mode::Native&&e.snapshot.phase==Phase::Starting&&!self.cancelled()).ok_or("Inactive multistream session")?;
        if !(2..=3).contains(&ids.len())||!e.destination_stops.is_empty(){return Err("Invalid multistream control registration".into())}
        let mut flags=std::collections::HashMap::new();
        for id in ids{
            if !uuid::Uuid::parse_str(&id).is_ok_and(|v|v.to_string()==id)||flags.insert(id,Arc::new(AtomicBool::new(false))).is_some(){return Err("Invalid or duplicate destination identity".into())}
        }
        e.destination_stops=flags.clone();Ok(flags)
    }
    pub(crate) fn stop_flag(&self)->Arc<AtomicBool>{self.stop.clone()}
    pub(crate) fn cancelled(&self)->bool{self.stop.load(Ordering::Acquire)}
    fn update(&self,f:impl FnOnce(&mut Snapshot)){
        if let Ok(mut entry)=self.controller.entry.lock(){
            if let Some(e)=entry.as_mut().filter(|v|v.snapshot.session_id==self.id&&Arc::ptr_eq(&v.stop,&self.stop)){
                if matches!(e.snapshot.phase,Phase::Stopped|Phase::Failed){return}
                e.snapshot.elapsed_ms=e.start.elapsed().as_millis() as u64;f(&mut e.snapshot);
            }
        }
    }
    pub(crate) fn record_retry_failure(&self,cause:crate::live_reconnect::Failure){if let Ok(mut f)=self.retry_failure.lock(){*f=Some(cause);}}
    pub(crate) fn take_retry_failure(&self)->crate::live_reconnect::Failure{self.retry_failure.lock().ok().and_then(|mut f|f.take()).unwrap_or(crate::live_reconnect::Failure::Unknown)}
    pub(crate) fn recovery_wait(&self){self.update(|s|{if !self.cancelled()&&s.mode==Mode::Native{s.phase=Phase::Reconnecting;}})}
    pub(crate) fn begin_epoch(&self,attempt:u8){
        let _=self.take_retry_failure();
        if let Ok(mut entry)=self.controller.entry.lock(){
            if let Some(e)=entry.as_mut().filter(|v|v.snapshot.session_id==self.id&&Arc::ptr_eq(&v.stop,&self.stop)){
                let s=&mut e.snapshot;
                if self.cancelled()||matches!(s.phase,Phase::Stopped|Phase::Failed){return}
                s.phase=Phase::Starting;s.reconnect_attempt=attempt;s.destinations.clear();s.timing=None;s.media_frames=0;s.sent_packets=0;s.sent_payload_bytes=0;
                s.pending_packets=0;s.pending_bytes=0;s.late_frames=0;s.bitrate_kbps=0;s.health=Reading::default();
                e.monitor=Monitor::default();e.epoch_started_ms=e.start.elapsed().as_millis() as u64;
            }
        }
    }
    pub(crate) fn running(&self){self.update(|s|{if s.phase==Phase::Starting&&!self.cancelled(){s.phase=Phase::Streaming;}})}
    pub(crate) fn enable_inplace_bitrate(&self,enabled:bool){
        if let Ok(mut entry)=self.controller.entry.lock(){
            if let Some(e)=entry.as_mut().filter(|v|v.snapshot.session_id==self.id&&Arc::ptr_eq(&v.stop,&self.stop)&&v.snapshot.mode==Mode::Native&&v.snapshot.phase==Phase::Starting){
                e.advisor.enable_inplace(enabled);e.snapshot.bitrate_advice=e.advisor.reading();
            }
        }
    }
    pub(crate) fn inplace_target(&self)->Option<u32>{
        let mut entry=self.controller.entry.lock().ok()?;
        let e=entry.as_mut().filter(|v|v.snapshot.session_id==self.id&&Arc::ptr_eq(&v.stop,&self.stop)&&v.snapshot.mode==Mode::Native&&v.snapshot.phase==Phase::Streaming&&!self.cancelled())?;
        e.advisor.inplace_target(e.start.elapsed().as_millis()as u64,e.snapshot.destinations.len())
    }
    pub(crate) fn inplace_result(&self,target:u32,result:Result<bool,()>){
        if let Ok(mut entry)=self.controller.entry.lock(){
            if let Some(e)=entry.as_mut().filter(|v|v.snapshot.session_id==self.id&&Arc::ptr_eq(&v.stop,&self.stop)&&!matches!(v.snapshot.phase,Phase::Stopped|Phase::Failed)){
                e.advisor.inplace_result(target,result);e.snapshot.bitrate_advice=e.advisor.reading();
            }
        }
    }
    pub(crate) fn bitrate_config(&self,initial:u32,current:u32,enabled:bool){
        if let Ok(mut entry)=self.controller.entry.lock(){
            if let Some(e)=entry.as_mut().filter(|v|v.snapshot.session_id==self.id&&Arc::ptr_eq(&v.stop,&self.stop)&&v.snapshot.mode==Mode::Native&&!matches!(v.snapshot.phase,Phase::Stopped|Phase::Failed)){
                e.advisor.configure(initial,current,enabled);e.snapshot.bitrate_advice=e.advisor.reading();
            }
        }
    }
    pub(crate) fn destination_telemetry(&self,destinations:Vec<crate::live_fanout::DestinationStats>){
        if let Ok(mut entry)=self.controller.entry.lock(){
            if let Some(e)=entry.as_mut().filter(|v|v.snapshot.session_id==self.id&&Arc::ptr_eq(&v.stop,&self.stop)&&v.snapshot.mode==Mode::Native&&!matches!(v.snapshot.phase,Phase::Stopped|Phase::Failed)){
                e.advisor.observe(e.start.elapsed().as_millis()as u64,&destinations);
                e.snapshot.bitrate_advice=e.advisor.reading();e.snapshot.destinations=destinations;
            }
        }
    }
    pub(crate) fn resource_sample(&self,memory:u64,cpu:Option<f32>){self.update(|s|{if s.mode==Mode::Native{s.resources.observe(s.elapsed_ms,memory,cpu);}})}
    pub(crate) fn timing_sample(&self,timing:crate::live_soak::Timing){self.update(|s|{if s.mode==Mode::Native{s.timing=Some(timing);}})}
    pub(crate) fn telemetry(&self,frames:u32,late:u32,worker:crate::live_publish_worker::WorkerStats){
        let Ok(mut entry)=self.controller.entry.lock()else{return};
        let Some(e)=entry.as_mut().filter(|v|v.snapshot.session_id==self.id&&Arc::ptr_eq(&v.stop,&self.stop))else{return};
        let s=&mut e.snapshot;
        if matches!(s.phase,Phase::Stopped|Phase::Failed){return}
        s.elapsed_ms=e.start.elapsed().as_millis() as u64;
        {
            if s.mode!=Mode::Native{return}
            s.media_frames=frames;s.late_frames=late;s.sent_packets=worker.session.sent_packets;
            s.sent_payload_bytes=worker.session.sent_payload_bytes;
            s.pending_packets=worker.pending_packets+worker.session.queued_packets;
            s.pending_bytes=worker.pending_bytes+worker.session.queued_bytes;
            s.bitrate_kbps=s.sent_payload_bytes.saturating_mul(8)/s.elapsed_ms.saturating_sub(e.epoch_started_ms).max(1);
            if s.phase==Phase::Starting&&!self.cancelled()&&worker.session.phase==crate::live_session::Phase::Sending{s.phase=Phase::Streaming;}
        }
        e.monitor.observe(Sample{elapsed_ms:s.elapsed_ms,frames,late_frames:late,sent_bytes:s.sent_payload_bytes,sent_packets:s.sent_packets,queue_bytes:s.pending_bytes,queue_packets:s.pending_packets});
    }
    pub(crate) fn finish(&self,success:bool){self.finish_with_outcome(success,None)}
    /// Only the native Program report may establish completed sender duration.
    /// A clean early stop remains successful shutdown, not a full-duration test.
    pub(crate) fn finish_native(&self,stopped_early:Option<bool>){
        let outcome=match stopped_early{
            Some(false)=>NativeOutcome::SenderCompleted,
            Some(true)=>NativeOutcome::StoppedEarly,
            None=>NativeOutcome::Failed,
        };
        self.finish_with_outcome(stopped_early.is_some(),Some(outcome));
    }
    fn finish_with_outcome(&self,success:bool,outcome:Option<NativeOutcome>){self.update(|s|{
        s.phase=if success{Phase::Stopped}else{Phase::Failed};
        if success{s.pending_packets=0;s.pending_bytes=0;}
        s.error=if success{None}else{Some("Streaming session ended before successful completion")};
        if s.mode==Mode::Native{
            s.native_outcome=Some(if success{
                if self.cancelled(){NativeOutcome::StoppedEarly}else{outcome.unwrap_or(NativeOutcome::Unverified)}
            }else{NativeOutcome::Failed});
        }
    });}
}
pub(crate) fn start_rehearsal(controller:&Arc<Controller>,id:String,seconds:u32)->Result<(),String>{
    if !(2..=15).contains(&seconds){return Err("Choose a 2–15 second offline lifecycle rehearsal".into())}
    let run=controller.start(id,Mode::Rehearsal)?;
    let guard=run.clone();
    if std::thread::Builder::new().name("moobee-live-rehearsal".into()).spawn(move||{
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
            let start=Instant::now();
            if !run.cancelled(){run.running();}
            while !run.cancelled()&&start.elapsed()<Duration::from_secs(u64::from(seconds)){
                std::thread::sleep(Duration::from_millis(25));
            }
        }));
        run.finish(result.is_ok());
    }).is_err(){guard.finish(false);return Err("Could not start lifecycle rehearsal".into())}
    Ok(())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn native_sender_completion_is_distinct_from_early_stop_and_failure(){
        let c=Arc::new(Controller::default());
        for (reported,expected,phase) in [(Some(false),NativeOutcome::SenderCompleted,Phase::Stopped),
            (Some(true),NativeOutcome::StoppedEarly,Phase::Stopped),(None,NativeOutcome::Failed,Phase::Failed)]{
            let id=uuid::Uuid::new_v4().to_string();
            let run=c.start(id.clone(),Mode::Native).unwrap();
            run.running();
            run.finish_native(reported);
            run.finish(true); // A later generic worker completion cannot overwrite terminal evidence.
            let status=c.snapshot(&id).unwrap();
            assert_eq!(status.native_outcome,Some(expected));
            assert_eq!(status.phase,phase);
        }
        let id=uuid::Uuid::new_v4().to_string();
        let run=c.start(id.clone(),Mode::Native).unwrap();
        assert!(c.stop_native(&id));run.finish_native(Some(false));
        assert_eq!(c.snapshot(&id).unwrap().native_outcome,Some(NativeOutcome::StoppedEarly));
        let id=uuid::Uuid::new_v4().to_string();
        let run=c.start(id.clone(),Mode::Native).unwrap();run.finish(true);
        assert_eq!(c.snapshot(&id).unwrap().native_outcome,Some(NativeOutcome::Unverified));
        let id=uuid::Uuid::new_v4().to_string();
        let run=c.start(id.clone(),Mode::Rehearsal).unwrap();run.finish(true);
        assert_eq!(c.snapshot(&id).unwrap().native_outcome,None);
    }
    #[test]fn destination_stop_is_scoped_and_cannot_retire_last_active_output(){
        use crate::live_fanout::DestinationStats;
        let c=Arc::new(Controller::default());
        let id=uuid::Uuid::new_v4().to_string();
        let a=uuid::Uuid::new_v4().to_string();let b=uuid::Uuid::new_v4().to_string();
        let r=c.start(id.clone(),Mode::Native).unwrap();
        let flags=r.register_destination_stops(vec![a.clone(),b.clone()]).unwrap();
        assert!(!c.stop_destination(&id,&a));r.running();
        let row=|id:String|DestinationStats{id,phase:"sending",sent_packets:1,sent_payload_bytes:1,queued_packets:0,queued_bytes:0,admitted_packets:1,admitted_bytes:1,reconnect_attempt:0,interrupted:false};
        r.destination_telemetry(vec![row(a.clone()),row(b.clone())]);
        assert!(!c.stop_destination("other",&a));
        assert!(!c.stop_destination(&id,"unknown"));
        assert!(c.stop_destination(&id,&a));
        assert!(flags[&a].load(Ordering::Acquire));
        assert!(!flags[&b].load(Ordering::Acquire));
        assert!(!c.stop_destination(&id,&a));assert!(!c.stop_destination(&id,&b));
        assert!(!r.cancelled());assert!(c.stop_native(&id));
        r.finish(false);
        assert!(!c.stop_destination(&id,&b));
    }
    #[test]fn scoped_stop_and_terminal_state(){
        let c=Arc::new(Controller::default());let id=uuid::Uuid::new_v4().to_string();
        let r=c.start(id.clone(),Mode::Rehearsal).unwrap();r.running();
        assert!(c.start(uuid::Uuid::new_v4().to_string(),Mode::Native).is_err());
        assert!(!c.stop_native(&id));assert!(!c.request_stop("other"));assert!(c.request_stop(&id));assert!(r.cancelled());
        r.running();assert_eq!(c.snapshot(&id).unwrap().phase,Phase::Stopping);assert!(!r.native_active());
        r.finish(true);assert_eq!(c.snapshot(&id).unwrap().phase,Phase::Stopped);
        assert!(!c.request_stop(&id));
        let next=uuid::Uuid::new_v4().to_string();let n=c.start(next.clone(),Mode::Native).unwrap();
        r.finish(false);assert_eq!(c.snapshot(&next).unwrap().phase,Phase::Starting);assert!(n.native_active());assert!(!c.stop_rehearsal(&next));assert!(c.stop_native(&next));assert!(!n.native_active());
        n.finish(false);assert_eq!(c.snapshot(&next).unwrap().phase,Phase::Failed);
        let reused=c.start(next.clone(),Mode::Rehearsal).unwrap();n.finish(false);
        assert_eq!(c.snapshot(&next).unwrap().phase,Phase::Starting);reused.finish(true);
    }
}
