//! Clocked, bounded publish pump. The owner supplies monotonic elapsed time
//! and transport readiness. No network thread or capture side effects here.
use crate::live_media_queue::MediaQueue;
use crate::stream_mux::Packet;
const MAX_BATCH:usize=8;
const MAX_LATENESS_MS:u64=2000;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum State{Ready,Stopped,Failed}
#[derive(Debug,PartialEq,Eq)]
pub(crate) struct Tick{pub sent:usize,pub next_due_ms:Option<u64>,pub state:State}
pub(crate) struct Pacer{state:State,last_elapsed:u64,sent:u64,bytes:u64}
impl Default for Pacer{fn default()->Self{Self{state:State::Ready,last_elapsed:0,sent:0,bytes:0}}}
impl Pacer{
    pub(crate) fn stats(&self)->(u64,u64){(self.sent,self.bytes)}
    pub(crate) fn stop(&mut self,queue:&mut MediaQueue){queue.fail();self.state=State::Stopped}
    fn fail(&mut self,queue:&mut MediaQueue){queue.fail();self.state=State::Failed}
    /// One tick sends only due packets, at most eight, and never waits on a
    /// future packet. A blocked transport should not call this until writable;
    /// an external owner must service inbound controls and enforce deadlines.
    pub(crate) fn tick(&mut self,queue:&mut MediaQueue,elapsed_ms:u64,mut send:impl FnMut(&Packet)->Result<(),String>)->Result<Tick,String>{
        if self.state!=State::Ready{return Ok(Tick{sent:0,next_due_ms:None,state:self.state})}
        if elapsed_ms<self.last_elapsed||queue.stats().restart_required{
            self.fail(queue);return Err("Publishing clock or media queue requires restart".into())
        }
        self.last_elapsed=elapsed_ms;
        let mut sent=0;
        while sent<MAX_BATCH{
            let Some(next)=queue.peek() else{break};
            let due=u64::from(next.timestamp_ms);
            if due>elapsed_ms{break}
            if elapsed_ms-due>MAX_LATENESS_MS{
                self.fail(queue);return Err("Publishing fell behind media timeline; restart required".into())
            }
            let packet=queue.pop().ok_or("Media queue changed unexpectedly")?;
            if send(&packet).is_err(){self.fail(queue);return Err("Publishing transport rejected media".into())}
            sent+=1;self.sent=self.sent.saturating_add(1);self.bytes=self.bytes.saturating_add(packet.payload.len() as u64);
        }
        Ok(Tick{sent,next_due_ms:queue.peek().map(|p|u64::from(p.timestamp_ms)),state:self.state})
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    use crate::stream_mux::{avc_sequence,avc_access_unit};
    fn queue()->MediaQueue{
        let mut q=MediaQueue::default();
        q.push(avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap()).unwrap();
        q.push(avc_access_unit(&[&[0x65,1]],0,0,true).unwrap()).unwrap();
        for i in 1..=20{q.push(avc_access_unit(&[&[0x41,2]],i*100,0,false).unwrap()).unwrap();}
        q
    }
    #[test]fn due_packets_are_paced_and_batches_bounded(){
        let mut q=queue();let mut p=Pacer::default();let mut seen=vec![];
        let t=p.tick(&mut q,0,|x|{seen.push(x.timestamp_ms);Ok(())}).unwrap();
        assert_eq!(t.sent,2);assert_eq!(t.next_due_ms,Some(100));assert_eq!(seen,[0,0]);
        assert_eq!(p.tick(&mut q,99,|_|panic!("early packet")).unwrap().sent,0);
        let t=p.tick(&mut q,1000,|x|{seen.push(x.timestamp_ms);Ok(())}).unwrap();
        assert_eq!(t.sent,8);assert_eq!(t.next_due_ms,Some(900));assert_eq!(p.stats().0,10);
    }
    #[test]fn rejects_late_clock_and_transport_failure(){
        let mut q=queue();let mut p=Pacer::default();
        assert!(p.tick(&mut q,2001,|_|panic!("late packet")).is_err());assert!(q.stats().restart_required);
        let mut q=queue();let mut p=Pacer::default();
        let err=p.tick(&mut q,0,|_|Err("sensitive transport detail".into())).unwrap_err();assert!(!err.contains("sensitive"));assert!(q.peek().is_none());
        let mut q=queue();let mut p=Pacer::default();p.tick(&mut q,100,|_|Ok(())).unwrap();assert!(p.tick(&mut q,99,|_|Ok(())).is_err());
    }
    #[test]fn explicit_stop_suppresses_delivery(){
        let mut q=queue();let mut p=Pacer::default();p.stop(&mut q);
        assert_eq!(p.tick(&mut q,0,|_|panic!("stopped")).unwrap().state,State::Stopped);assert_eq!(q.stats().queued_bytes,0);
    }
}
