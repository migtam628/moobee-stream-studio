//! Streaming lifecycle controller. The registered diagnostic uses no media,
//! devices, credentials or network. The internal native path uses the same
//! controller with a real Program capture worker.
use serde::Serialize;
use crate::live_health::{Monitor,Reading,Sample};
use std::{sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) enum Phase{Starting,Streaming,Stopping,Stopped,Failed}
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) enum Mode{Rehearsal,Native}
#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Snapshot{
    session_id:String,mode:Mode,phase:Phase,elapsed_ms:u64,media_frames:u32,
    sent_packets:u64,sent_payload_bytes:u64,pending_packets:usize,pending_bytes:usize,
    late_frames:u32,bitrate_kbps:u64,error:Option<&'static str>,health:Reading,
}
struct Entry{snapshot:Snapshot,start:Instant,stop:Arc<AtomicBool>,monitor:Monitor}
#[derive(Default)]
pub(crate) struct Controller{entry:Mutex<Option<Entry>>}
#[derive(Clone)]
pub(crate) struct Run{controller:Arc<Controller>,id:String,stop:Arc<AtomicBool>}
impl Controller{
    pub(crate) fn start(self:&Arc<Self>,id:String,mode:Mode)->Result<Run,String>{
        if uuid::Uuid::parse_str(&id).map_or(true,|v|v.to_string()!=id){return Err("Invalid stream session ID".into())}
        let mut e=self.entry.lock().map_err(|_|"Stream state unavailable")?;
        if e.as_ref().is_some_and(|v|!matches!(v.snapshot.phase,Phase::Stopped|Phase::Failed)){return Err("Another streaming session is active".into())}
        let stop=Arc::new(AtomicBool::new(false));
        *e=Some(Entry{snapshot:Snapshot{session_id:id.clone(),mode,phase:Phase::Starting,elapsed_ms:0,media_frames:0,
            sent_packets:0,sent_payload_bytes:0,pending_packets:0,pending_bytes:0,late_frames:0,bitrate_kbps:0,error:None,health:Reading::default()},start:Instant::now(),stop:stop.clone(),monitor:Monitor::default()});
        Ok(Run{controller:self.clone(),id,stop})
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
    pub(crate) fn request_stop(&self,id:&str)->bool{
        let Ok(mut e)=self.entry.lock()else{return false};
        let Some(e)=e.as_mut().filter(|v|v.snapshot.session_id==id)else{return false};
        if matches!(e.snapshot.phase,Phase::Stopped|Phase::Failed){return false}
        e.stop.store(true,Ordering::Release);e.snapshot.phase=Phase::Stopping;true
    }
}
impl Run{
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
    pub(crate) fn running(&self){self.update(|s|{if s.phase==Phase::Starting&&!self.cancelled(){s.phase=Phase::Streaming;}})}
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
            s.bitrate_kbps=s.sent_payload_bytes.saturating_mul(8)/s.elapsed_ms.max(1);
            if s.phase==Phase::Starting&&!self.cancelled()&&worker.session.phase==crate::live_session::Phase::Sending{s.phase=Phase::Streaming;}
        }
        e.monitor.observe(Sample{elapsed_ms:s.elapsed_ms,frames,late_frames:late,sent_bytes:s.sent_payload_bytes,sent_packets:s.sent_packets,queue_bytes:s.pending_bytes,queue_packets:s.pending_packets});
    }
    pub(crate) fn finish(&self,success:bool){self.update(|s|{
        s.phase=if success{Phase::Stopped}else{Phase::Failed};
        if success{s.pending_packets=0;s.pending_bytes=0;}
        s.error=if success{None}else{Some("Streaming session ended before successful completion")};
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
    #[test]fn scoped_stop_and_terminal_state(){
        let c=Arc::new(Controller::default());let id=uuid::Uuid::new_v4().to_string();
        let r=c.start(id.clone(),Mode::Rehearsal).unwrap();r.running();
        assert!(c.start(uuid::Uuid::new_v4().to_string(),Mode::Native).is_err());
        assert!(!c.request_stop("other"));assert!(c.request_stop(&id));assert!(r.cancelled());
        r.running();assert_eq!(c.snapshot(&id).unwrap().phase,Phase::Stopping);
        r.finish(true);assert_eq!(c.snapshot(&id).unwrap().phase,Phase::Stopped);
        assert!(!c.request_stop(&id));
        let next=uuid::Uuid::new_v4().to_string();let n=c.start(next.clone(),Mode::Native).unwrap();
        r.finish(false);assert_eq!(c.snapshot(&next).unwrap().phase,Phase::Starting);assert!(!c.stop_rehearsal(&next));
        n.finish(false);assert_eq!(c.snapshot(&next).unwrap().phase,Phase::Failed);
        let reused=c.start(next.clone(),Mode::Rehearsal).unwrap();n.finish(false);
        assert_eq!(c.snapshot(&next).unwrap().phase,Phase::Starting);reused.finish(true);
    }
}
