//! One encoded batch, independently bounded publisher workers. No socket
//! opening, credential access or destination discovery here.
use crate::{live_publish_worker::{Worker,WorkerStats},live_session::{Phase,Stats},live_reconnect::{Failure,Policy,Decision},live_rejoin::Config,stream_mux::Packet};
use serde::Serialize;
use std::{collections::HashSet,sync::{Arc,atomic::{AtomicBool,Ordering}},thread::JoinHandle,time::{Duration,Instant}};
pub(crate) const MAX_DESTINATIONS:usize=3;
#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct DestinationStats{
    pub id:String,pub phase:&'static str,pub sent_packets:u64,pub sent_payload_bytes:u64,
    pub queued_packets:usize,pub queued_bytes:usize,pub admitted_packets:u64,pub admitted_bytes:u64,
    pub reconnect_attempt:u8,pub interrupted:bool,
}
pub(crate) type Connector=Arc<dyn Fn(crate::rtmp_session::ConnectionPermit)->Result<Worker,Failure>+Send+Sync>;
enum Recovery{Waiting,Connecting(JoinHandle<Result<Worker,Failure>>),AwaitKey}
struct Output{
    id:String,worker:Worker,failed:bool,completed:bool,admitted_packets:u64,admitted_bytes:u64,failure:Option<Failure>,
    connector:Option<Connector>,policy:Policy,recovery:Option<Recovery>,first_failure:Option<u64>,attempt:u8,
    interrupted:bool,base:Option<u32>,prior_packets:u64,prior_bytes:u64,
    epoch_admitted_packets:u64,epoch_admitted_bytes:u64,
    user_stop:Arc<AtomicBool>,retired:bool,
}
pub(crate) struct Fanout{
    outputs:Vec<Output>,finished:bool,config:Config,start:Instant,
    cancel:Arc<AtomicBool>,session_stop:Option<Arc<AtomicBool>>,
}
impl Fanout{
    pub(crate) fn single(worker:Worker)->Result<Self,String>{Self::new(vec![("single".into(),worker)])}
    pub(crate) fn new(workers:Vec<(String,Worker)>)->Result<Self,String>{
        Self::with_recovery(workers.into_iter().map(|(id,w)|(id,w,None)).collect(),None)
    }
    pub(crate) fn with_recovery(workers:Vec<(String,Worker,Option<Connector>)>,session_stop:Option<Arc<AtomicBool>>)->Result<Self,String>{
        if workers.is_empty()||workers.len()>MAX_DESTINATIONS{return Err("Choose one to three publishing destinations".into())}
        let mut ids=HashSet::new();
        for (id,w,_) in &workers{
            if !(id=="single"&&workers.len()==1)&&!uuid::Uuid::parse_str(id).is_ok_and(|v|v.to_string()==*id){return Err("Invalid destination identity".into())}
            if !ids.insert(id.clone())||w.stats()?.session.phase!=Phase::Authorized||w.is_finished(){return Err("Requires distinct fresh authorized publishers".into())}
        }
        Ok(Self{outputs:workers.into_iter().map(|(id,worker,connector)|Output{
            id,worker,failed:false,completed:false,admitted_packets:0,admitted_bytes:0,failure:None,
            connector,policy:Policy::default(),recovery:None,first_failure:None,attempt:0,interrupted:false,base:None,
            prior_packets:0,prior_bytes:0,epoch_admitted_packets:0,epoch_admitted_bytes:0,user_stop:Arc::new(AtomicBool::new(false)),retired:false,
        }).collect(),finished:false,config:Config::default(),start:Instant::now(),cancel:Arc::new(AtomicBool::new(false)),session_stop})
    }
    pub(crate) fn bind_destination_stops(mut self,flags:std::collections::HashMap<String,Arc<AtomicBool>>)->Result<Self,String>{
        if self.outputs.len()<2||flags.len()!=self.outputs.len(){return Err("Invalid destination stop controls".into())}
        for o in &mut self.outputs{o.user_stop=flags.get(&o.id).ok_or("Missing destination stop permit")?.clone();}
        Ok(self)
    }
    fn retire_if_requested(o:&mut Output){
        if o.retired||!o.user_stop.load(Ordering::Acquire){return}
        o.retired=true;o.interrupted=true;
        // An already reconnecting output has no continuous epoch to drain.
        // Revocation also prevents an in-flight connector from rejoining.
        let recovering=o.recovery.take().is_some();
        o.connector=None;
        if recovering||o.failed{o.worker.request_stop();}else{o.worker.request_drain();}
    }
    fn fail(o:&mut Output){
        if !o.failed{
            o.failure=o.worker.stats().ok().and_then(|s|s.session.failure);
            o.failed=true;o.worker.request_failure();
        }
    }
    fn schedule(o:&mut Output,now:u64,allowed:bool,cause:Failure){
        o.interrupted=true;o.failure=Some(cause);o.failed=true;
        o.first_failure.get_or_insert(now);
        o.recovery=if o.connector.is_some()&&matches!(o.policy.failed(now,cause,allowed,!allowed),Decision::WaitUntil(_)){
            Some(Recovery::Waiting)
        }else{None};
    }
    fn advance(o:&mut Output,now:u64,allowed:bool,cancel:&Arc<AtomicBool>,session_stop:&Option<Arc<AtomicBool>>){
        let Some(state)=o.recovery.take()else{return};
        if !allowed||o.first_failure.is_some_and(|t|now.saturating_sub(t)>30000){
            o.failed=true;o.worker.request_failure();return
        }
        match state{
            Recovery::Waiting=>match o.policy.poll(now,allowed,!allowed){
                Decision::WaitUntil(_)=>o.recovery=Some(Recovery::Waiting),
                Decision::Stop=>o.failed=true,
                Decision::StartFreshEpoch{attempt}=>{
                    o.attempt=attempt;
                    let connector=o.connector.as_ref().unwrap().clone();
                    let cancel=cancel.clone();let stop=session_stop.clone();let output_stop=o.user_stop.clone();
                    let remaining=30000u64.saturating_sub(now.saturating_sub(o.first_failure.unwrap_or(now)));
                    let deadline=Instant::now()+Duration::from_millis(remaining);
                    let h=std::thread::Builder::new().name("moobee-destination-recovery".into()).spawn(move||{
                        let allowed:crate::rtmp_session::ConnectionPermit=Arc::new(move||Instant::now()<deadline&&!cancel.load(Ordering::Acquire)&&!output_stop.load(Ordering::Acquire)&&!stop.as_ref().is_some_and(|s|s.load(Ordering::Acquire)));
                        if !allowed(){return Err(Failure::Unknown)}
                        let worker=connector(allowed.clone())?;
                        if !allowed(){worker.request_stop();return Err(Failure::Unknown)}
                        Ok(worker)
                    });
                    match h{Ok(h)=>o.recovery=Some(Recovery::Connecting(h)),Err(_)=>o.failed=true}
                }
            },
            Recovery::Connecting(h)=>{
                if !h.is_finished(){o.recovery=Some(Recovery::Connecting(h));return}
                match h.join(){
                    Ok(Ok(worker))=>{
                        if let Ok(s)=o.worker.stats(){
                            o.prior_packets=o.prior_packets.saturating_add(s.session.sent_packets);
                            o.prior_bytes=o.prior_bytes.saturating_add(s.session.sent_payload_bytes);
                        }
                        o.worker=worker;o.epoch_admitted_packets=0;o.epoch_admitted_bytes=0;
                        o.base=None;o.failed=false;o.failure=None;o.recovery=Some(Recovery::AwaitKey);
                    }
                    Ok(Err(cause))=>Self::schedule(o,now,allowed,cause),
                    Err(_)=>o.failed=true,
                }
            }
            Recovery::AwaitKey=>o.recovery=Some(Recovery::AwaitKey),
        }
    }
    fn submit_to(o:&mut Output,packets:Vec<Packet>)->Result<(),String>{
        if packets.is_empty(){return Ok(())}
        let count=packets.len() as u64;
        let bytes=packets.iter().map(|p|p.payload.len()as u64).sum::<u64>();
        o.worker.submit_batch(packets)?;
        o.admitted_packets+=count;o.admitted_bytes+=bytes;
        o.epoch_admitted_packets+=count;o.epoch_admitted_bytes+=bytes;Ok(())
    }
    pub(crate) fn submit(&mut self,packets:Vec<Packet>)->Result<(),String>{
        if self.finished||packets.is_empty()||packets.len()>32{return Err("Invalid publishing fanout batch".into())}
        let bytes=packets.iter().try_fold(0usize,|n,p|n.checked_add(p.payload.len())).ok_or("Publishing batch overflow")?;
        if bytes>8*1024*1024{return Err("Publishing batch exceeds budget".into())}
        self.config.observe(&packets);
        let now=self.start.elapsed().as_millis() as u64;
        let allowed=!self.cancel.load(Ordering::Acquire)&&!self.session_stop.as_ref().is_some_and(|s|s.load(Ordering::Acquire));
        let mut viable=0;
        for o in &mut self.outputs{
            Self::retire_if_requested(o);
            if o.retired{continue}
            Self::advance(o,now,allowed,&self.cancel,&self.session_stop);
            if matches!(o.recovery,Some(Recovery::Waiting|Recovery::Connecting(_))){viable+=1;continue}
            if o.failed{continue}
            if o.worker.is_finished()||o.worker.stats().is_ok_and(|s|s.session.phase==Phase::Failed){
                Self::fail(o);o.recovery=None;
                if o.connector.is_some(){let cause=o.failure.unwrap_or(Failure::Unknown);Self::schedule(o,now,allowed,cause);}
                if o.recovery.is_some(){viable+=1;}continue
            }
            let batch=if matches!(o.recovery,Some(Recovery::AwaitKey)){
                let Some((base,batch))=self.config.start(&packets)else{viable+=1;continue};
                o.base=Some(base);o.recovery=None;batch
            }else if let Some(base)=o.base{Config::rebase(&packets,base)}
            else{packets.iter().map(|p|Packet{kind:p.kind,timestamp_ms:p.timestamp_ms,payload:p.payload.clone()}).collect()};
            if Self::submit_to(o,batch).is_err(){
                Self::fail(o);
                if o.connector.is_some(){let cause=o.failure.unwrap_or(Failure::Unknown);Self::schedule(o,now,allowed,cause);}
                if o.recovery.is_some(){viable+=1;}continue
            }
            viable+=1;
        }
        if viable==0{return Err("All publishing destinations failed".into())}
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn test_awaiting_key(&self,index:usize)->bool{
        matches!(self.outputs[index].recovery,Some(Recovery::AwaitKey))
    }
    #[cfg(test)]
    pub(crate) fn test_signal_inbound(&self,id:&str)->Result<(),String>{
        self.outputs.iter().find(|o|o.id==id).ok_or("Unknown test destination")?.worker.signal_inbound()
    }
    pub(crate) fn retry_failure(&self)->Option<Failure>{
        // A single destination may restart its entire encoder epoch. Do not
        // restart a surviving destination's shared encoder after another fails.
        if self.outputs.len()==1{self.outputs[0].failure}else{None}
    }
    pub(crate) fn stats(&self)->Result<WorkerStats,String>{
        let mut total=WorkerStats{session:Stats{phase:Phase::Authorized,queued_packets:0,queued_bytes:0,sent_packets:0,sent_payload_bytes:0,restart_required:false,failure:None},pending_packets:0,pending_bytes:0,stopping:self.finished};
        let mut active=0;let mut sending=false;let mut failed=0;
        for o in &self.outputs{
            let s=o.worker.stats()?;
            total.session.sent_packets=total.session.sent_packets.saturating_add(o.prior_packets).saturating_add(s.session.sent_packets);
            total.session.sent_payload_bytes=total.session.sent_payload_bytes.saturating_add(o.prior_bytes).saturating_add(s.session.sent_payload_bytes);
            total.session.queued_packets+=s.session.queued_packets;total.session.queued_bytes+=s.session.queued_bytes;
            total.pending_packets+=s.pending_packets;total.pending_bytes+=s.pending_bytes;
            if o.retired{continue}
            if o.recovery.is_some(){active+=1;}
            else if o.failed||s.session.phase==Phase::Failed{failed+=1;}
            else if matches!(s.session.phase,Phase::Authorized|Phase::Sending){active+=1;sending|=s.session.phase==Phase::Sending;}
        }
        total.session.phase=if active>0{if sending{Phase::Sending}else{Phase::Authorized}}else if failed>0{Phase::Failed}else{Phase::Stopped};
        total.session.restart_required=active==0&&failed>0;total.session.failure=self.retry_failure();
        Ok(total)
    }
    pub(crate) fn destinations(&self)->Vec<DestinationStats>{
        self.outputs.iter().map(|o|{
            let stats=o.worker.stats().ok();
            let phase=if o.retired||o.user_stop.load(Ordering::Acquire){
                match stats.map(|s|s.session.phase){Some(Phase::Stopped)=>"stopped",Some(Phase::Failed) if o.retired=>"failed",_=>"stopping"}
            }else if o.recovery.is_some(){"reconnecting"}else if o.failed{"failed"}else if o.completed{"stopped"}else{match stats.map(|s|s.session.phase){Some(Phase::Authorized)=>"authorized",Some(Phase::Sending)=>"sending",Some(Phase::Stopped)=>"stopped",_=>"failed"}};
            DestinationStats{id:o.id.clone(),phase,sent_packets:o.prior_packets.saturating_add(stats.map_or(0,|s|s.session.sent_packets)),
                sent_payload_bytes:o.prior_bytes.saturating_add(stats.map_or(0,|s|s.session.sent_payload_bytes)),
                queued_packets:stats.map_or(0,|s|s.pending_packets+s.session.queued_packets),
                queued_bytes:stats.map_or(0,|s|s.pending_bytes+s.session.queued_bytes),
                admitted_packets:o.admitted_packets,admitted_bytes:o.admitted_bytes,reconnect_attempt:o.attempt,interrupted:o.interrupted}
        }).collect()
    }
    pub(crate) fn finish(&mut self)->Result<(),String>{
        if self.finished{return if self.outputs.iter().all(|o|o.completed&&!o.interrupted){Ok(())}else{Err("One or more destinations did not finish publishing".into())}}
        self.cancel.store(true,Ordering::Release);
        for o in &mut self.outputs{Self::retire_if_requested(o);if o.recovery.take().is_some(){Self::fail(o);}if !o.failed&&!o.retired{o.worker.request_drain();}}
        let deadline=Instant::now()+Duration::from_secs(5);
        loop{
            let mut pending=false;
            for o in &mut self.outputs{
                if o.failed||o.completed{continue}
                match o.worker.reap(){
                    Ok(false)=>{if Instant::now()>=deadline{Self::fail(o);}else{pending=true;}},
                    Ok(true)=>{
                        let complete=o.worker.stats().is_ok_and(|s|s.session.phase==Phase::Stopped&&s.session.sent_packets==o.epoch_admitted_packets&&s.session.sent_payload_bytes==o.epoch_admitted_bytes&&s.pending_packets==0&&s.session.queued_packets==0);
                        if complete{o.completed=true}else{Self::fail(o)}
                    }
                    Err(_)=>Self::fail(o),
                }
            }
            if !pending{break}std::thread::sleep(Duration::from_millis(10));
        }
        self.finished=true;
        if self.outputs.iter().all(|o|o.completed&&!o.interrupted){Ok(())}else{Err("One or more destinations did not finish publishing".into())}
    }
}
impl Drop for Fanout{fn drop(&mut self){self.cancel.store(true,Ordering::Release);if !self.finished{for o in &self.outputs{o.worker.request_failure();}}}}
