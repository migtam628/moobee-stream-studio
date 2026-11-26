//! Dedicated owner of an already authorized publisher. No implicit networking.
//! The capture producer never performs socket I/O or waits for queue space.
use crate::{live_session::{Phase,Session,Stats},stream_mux::Packet};
use std::{collections::VecDeque,io::{Read,Write},sync::{Arc,Mutex,Condvar},thread::{self,JoinHandle},time::{Duration,Instant}};
const MAX_PENDING_PACKETS:usize=128;
const MAX_PENDING_BYTES:usize=8*1024*1024;
const MAX_PACKET_BYTES:usize=2*1024*1024;
const POLL:Duration=Duration::from_millis(5);
const MAX_IDLE:Duration=Duration::from_secs(5);
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum Command{Running,Stop,Fail}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct WorkerStats{
    pub session:Stats,pub pending_packets:usize,pub pending_bytes:usize,pub stopping:bool,
}
struct Mailbox{
    command:Command,packets:VecDeque<Packet>,bytes:usize,
    inbound_ready:bool,stats:Stats,
}
struct Shared{mail:Mutex<Mailbox>,wake:Condvar}
pub(crate) struct Worker{shared:Arc<Shared>,handle:Option<JoinHandle<()>>}
impl Worker{
    /// Caller must supply a transport with finite read/write deadlines.
    /// No credential resolution, DNS, connection or device capture happens here.
    pub(crate) fn start<T:Read+Write+Send+'static>(session:Session<T>)->Result<Self,String>{
        let stats=session.stats();
        if stats.phase!=Phase::Authorized{return Err("Worker requires a fresh authorized session".into())}
        let shared=Arc::new(Shared{mail:Mutex::new(Mailbox{command:Command::Running,packets:VecDeque::new(),bytes:0,inbound_ready:false,stats}),wake:Condvar::new()});
        let state=Arc::clone(&shared);
        let handle=thread::Builder::new().name("moobee-live-publisher".into()).spawn(move||run(session,state))
            .map_err(|_|"Could not start publishing worker")?;
        Ok(Self{shared,handle:Some(handle)})
    }
    pub(crate) fn submit(&self,packet:Packet)->Result<(),String>{
        let mut m=self.shared.mail.lock().map_err(|_|"Publishing worker unavailable")?;
        if m.command!=Command::Running||matches!(m.stats.phase,Phase::Stopped|Phase::Failed){return Err("Publishing worker is inactive".into())}
        let len=packet.payload.len();
        if len<2||len>MAX_PACKET_BYTES||m.packets.len()>=MAX_PENDING_PACKETS||len>MAX_PENDING_BYTES-m.bytes{
            m.command=Command::Fail;m.packets.clear();m.bytes=0;self.shared.wake.notify_one();
            return Err("Publishing mailbox overflow or invalid packet; new session required".into());
        }
        m.bytes+=len;m.packets.push_back(packet);self.shared.wake.notify_one();Ok(())
    }
    /// The transport owner must establish that a complete inbound chunk can be
    /// read before signaling. A socket becoming readable alone is insufficient.
    pub(crate) fn signal_inbound(&self)->Result<(),String>{
        let mut m=self.shared.mail.lock().map_err(|_|"Publishing worker unavailable")?;
        if m.command!=Command::Running||matches!(m.stats.phase,Phase::Stopped|Phase::Failed){return Err("Publishing worker is inactive".into())}
        m.inbound_ready=true;self.shared.wake.notify_one();Ok(())
    }
    pub(crate) fn stats(&self)->Result<WorkerStats,String>{
        let m=self.shared.mail.lock().map_err(|_|"Publishing worker unavailable")?;
        Ok(WorkerStats{session:m.stats,pending_packets:m.packets.len(),pending_bytes:m.bytes,stopping:m.command!=Command::Running})
    }
    pub(crate) fn request_stop(&self){
        if let Ok(mut m)=self.shared.mail.lock(){
            if m.command==Command::Running{m.command=Command::Stop;}
            m.packets.clear();m.bytes=0;m.inbound_ready=false;self.shared.wake.notify_one();
        }
    }
    pub(crate) fn is_finished(&self)->bool{self.handle.as_ref().map_or(true,|h|h.is_finished())}
    /// Nonblocking reaping; never join an in-flight network operation on the UI.
    pub(crate) fn reap(&mut self)->Result<bool,String>{
        if !self.is_finished(){return Ok(false)}
        if let Some(h)=self.handle.take(){h.join().map_err(|_|"Publishing worker terminated unexpectedly")?;}
        Ok(true)
    }
}
impl Drop for Worker{fn drop(&mut self){self.request_stop();}}
fn run<T:Read+Write>(mut session:Session<T>,shared:Arc<Shared>){
    let mut origin=None::<Instant>;
    let mut last_input=Instant::now();
    loop{
        // Move one bounded packet out without holding a lock during encoding/I/O.
        let (command,packet,inbound)={
            let Ok(m)=shared.mail.lock()else{session.stop();return};
            let Ok((mut m,_))=shared.wake.wait_timeout_while(m,POLL,|m|m.command==Command::Running&&m.packets.is_empty()&&!m.inbound_ready)else{session.stop();return};
            let command=m.command;
            let packet=if command==Command::Running{m.packets.pop_front()}else{None};
            if let Some(ref p)=packet{m.bytes-=p.payload.len();}
            let inbound=command==Command::Running&&std::mem::take(&mut m.inbound_ready);
            (command,packet,inbound)
        };
        let result=if command==Command::Stop{session.stop();Ok(())}
            else if command==Command::Fail{session.abort();Ok(())}
            else{( ||->Result<(),String>{
                if let Some(p)=packet{
                    let now=Instant::now();last_input=now;
                    origin.get_or_insert(now);
                    session.submit(p)?;
                }
                if inbound{session.service_inbound()?;}
                if let Some(start)=origin{
                    session.tick(start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64)?;
                }
                if last_input.elapsed()>=MAX_IDLE{session.abort();return Err("Encoded media input stalled".into());}
                Ok(())
            })()};
        if result.is_err(){session.abort();}
        let stats=session.stats();
        let terminal=matches!(stats.phase,Phase::Stopped|Phase::Failed);
        if let Ok(mut m)=shared.mail.lock(){
            m.stats=stats;
            if terminal{
                m.packets.clear();m.bytes=0;m.inbound_ready=false;
                m.command=if stats.phase==Phase::Failed{Command::Fail}else{Command::Stop};
            }
        }else{session.stop();return}
        if terminal{return}
    }
}
