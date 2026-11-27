//! One encoded batch, independently bounded publisher workers. No socket
//! opening, credential access or destination discovery here.
use crate::{live_publish_worker::{Worker,WorkerStats},live_session::{Phase,Stats},live_reconnect::Failure,stream_mux::Packet};
use serde::Serialize;
use std::{collections::HashSet,time::{Duration,Instant}};
pub(crate) const MAX_DESTINATIONS:usize=3;
#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct DestinationStats{
    pub id:String,pub phase:&'static str,pub sent_packets:u64,pub sent_payload_bytes:u64,
    pub queued_packets:usize,pub queued_bytes:usize,pub admitted_packets:u64,pub admitted_bytes:u64,
}
struct Output{id:String,worker:Worker,failed:bool,completed:bool,admitted_packets:u64,admitted_bytes:u64,failure:Option<Failure>}
pub(crate) struct Fanout{outputs:Vec<Output>,finished:bool}
impl Fanout{
    pub(crate) fn single(worker:Worker)->Result<Self,String>{Self::new(vec![("single".into(),worker)])}
    pub(crate) fn new(workers:Vec<(String,Worker)>)->Result<Self,String>{
        if workers.is_empty()||workers.len()>MAX_DESTINATIONS{return Err("Choose one to three publishing destinations".into())}
        let mut ids=HashSet::new();
        for (id,w) in &workers{
            if !(id=="single"&&workers.len()==1)&&!uuid::Uuid::parse_str(id).is_ok_and(|v|v.to_string()==*id){return Err("Invalid destination identity".into())}
            if !ids.insert(id.clone())||w.stats()?.session.phase!=Phase::Authorized||w.is_finished(){return Err("Requires distinct fresh authorized publishers".into())}
        }
        Ok(Self{outputs:workers.into_iter().map(|(id,worker)|Output{id,worker,failed:false,completed:false,admitted_packets:0,admitted_bytes:0,failure:None}).collect(),finished:false})
    }
    fn fail(o:&mut Output){
        if !o.failed{
            o.failure=o.worker.stats().ok().and_then(|s|s.session.failure);
            o.failed=true;o.worker.request_failure();
        }
    }
    pub(crate) fn submit(&mut self,packets:Vec<Packet>)->Result<(),String>{
        if self.finished||packets.is_empty()||packets.len()>32{return Err("Invalid publishing fanout batch".into())}
        let bytes=packets.iter().try_fold(0usize,|n,p|n.checked_add(p.payload.len())).ok_or("Publishing batch overflow")?;
        if bytes>8*1024*1024{return Err("Publishing batch exceeds budget".into())}
        let count=packets.len() as u64;
        let mut accepted=0;
        for o in &mut self.outputs{
            if o.failed{continue}
            if o.worker.is_finished()||o.worker.submit_batch(packets.iter().map(|p|Packet{kind:p.kind,timestamp_ms:p.timestamp_ms,payload:p.payload.clone()}).collect()).is_err(){Self::fail(o);continue}
            o.admitted_packets+=count;o.admitted_bytes+=bytes as u64;accepted+=1;
        }
        if accepted==0{return Err("All publishing destinations failed".into())}
        Ok(())
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
            total.session.sent_packets=total.session.sent_packets.saturating_add(s.session.sent_packets);
            total.session.sent_payload_bytes=total.session.sent_payload_bytes.saturating_add(s.session.sent_payload_bytes);
            total.session.queued_packets+=s.session.queued_packets;total.session.queued_bytes+=s.session.queued_bytes;
            total.pending_packets+=s.pending_packets;total.pending_bytes+=s.pending_bytes;
            if o.failed||s.session.phase==Phase::Failed{failed+=1;}
            else if matches!(s.session.phase,Phase::Authorized|Phase::Sending){active+=1;sending|=s.session.phase==Phase::Sending;}
        }
        total.session.phase=if active>0{if sending{Phase::Sending}else{Phase::Authorized}}else if failed>0{Phase::Failed}else{Phase::Stopped};
        total.session.restart_required=active==0&&failed>0;total.session.failure=self.retry_failure();
        Ok(total)
    }
    pub(crate) fn destinations(&self)->Vec<DestinationStats>{
        self.outputs.iter().map(|o|{
            let stats=o.worker.stats().ok();
            let phase=if o.failed{"failed"}else if o.completed{"stopped"}else{match stats.map(|s|s.session.phase){Some(Phase::Authorized)=>"authorized",Some(Phase::Sending)=>"sending",Some(Phase::Stopped)=>"stopped",_=>"failed"}};
            DestinationStats{id:o.id.clone(),phase,sent_packets:stats.map_or(0,|s|s.session.sent_packets),
                sent_payload_bytes:stats.map_or(0,|s|s.session.sent_payload_bytes),
                queued_packets:stats.map_or(0,|s|s.pending_packets+s.session.queued_packets),
                queued_bytes:stats.map_or(0,|s|s.pending_bytes+s.session.queued_bytes),
                admitted_packets:o.admitted_packets,admitted_bytes:o.admitted_bytes}
        }).collect()
    }
    pub(crate) fn finish(&mut self)->Result<(),String>{
        if self.finished{return if self.outputs.iter().all(|o|o.completed){Ok(())}else{Err("One or more destinations did not finish publishing".into())}}
        for o in &self.outputs{if !o.failed{o.worker.request_drain();}}
        let deadline=Instant::now()+Duration::from_secs(5);
        loop{
            let mut pending=false;
            for o in &mut self.outputs{
                if o.failed||o.completed{continue}
                match o.worker.reap(){
                    Ok(false)=>{if Instant::now()>=deadline{Self::fail(o);}else{pending=true;}},
                    Ok(true)=>{
                        let complete=o.worker.stats().is_ok_and(|s|s.session.phase==Phase::Stopped&&s.session.sent_packets==o.admitted_packets&&s.session.sent_payload_bytes==o.admitted_bytes&&s.pending_packets==0&&s.session.queued_packets==0);
                        if complete{o.completed=true}else{Self::fail(o)}
                    }
                    Err(_)=>Self::fail(o),
                }
            }
            if !pending{break}std::thread::sleep(Duration::from_millis(10));
        }
        self.finished=true;
        if self.outputs.iter().all(|o|o.completed){Ok(())}else{Err("One or more destinations did not finish publishing".into())}
    }
}
impl Drop for Fanout{fn drop(&mut self){if !self.finished{for o in &self.outputs{o.worker.request_failure();}}}}
