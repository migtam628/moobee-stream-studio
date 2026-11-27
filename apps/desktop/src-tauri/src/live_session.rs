//! One authorized destination's session lifecycle. No implicit network
//! connection, device capture, installer, or UI invocation.
use crate::{live_media_queue::MediaQueue,live_packet_pacer::{Pacer,Tick},rtmp_session::Publisher,stream_mux::Packet};
use std::io::{Read,Write};
use crate::live_reconnect::Failure;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum Phase{Authorized,Sending,Stopped,Failed}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct Stats{
    pub phase:Phase,pub queued_packets:usize,pub queued_bytes:usize,
    pub sent_packets:u64,pub sent_payload_bytes:u64,pub restart_required:bool,pub failure:Option<Failure>,
}
pub(crate) struct Session<T:Read+Write>{
    publisher:Option<Publisher<T>>,queue:MediaQueue,pacer:Pacer,phase:Phase,failure:Option<Failure>,
}
impl<T:Read+Write> Session<T>{
    /// Publisher::establish must complete the server authorization first.
    /// The owner must explicitly create a new session for every reconnect.
    pub(crate) fn new(publisher:Publisher<T>)->Result<Self,String>{
        if !publisher.authorized(){return Err("Publisher is not authorized".into())}
        Ok(Self{publisher:Some(publisher),queue:MediaQueue::default(),pacer:Pacer::default(),phase:Phase::Authorized,failure:None})
    }
    pub(crate) fn stats(&self)->Stats{
        let q=self.queue.stats();let (sent_packets,sent_payload_bytes)=self.pacer.stats();
        Stats{phase:self.phase,queued_packets:q.queued_packets,queued_bytes:q.queued_bytes,
            sent_packets,sent_payload_bytes,restart_required:self.phase==Phase::Failed,failure:self.failure}
    }
    fn require_active(&self)->Result<(),String>{
        if matches!(self.phase,Phase::Authorized|Phase::Sending)&&self.publisher.is_some(){Ok(())}
        else{Err("Publishing session is no longer active".into())}
    }
    fn fail(&mut self){self.queue.fail();self.publisher.take();self.phase=Phase::Failed;}
    fn fail_transport(&mut self){self.failure=self.publisher.as_ref().and_then(|p|p.transport_failure());self.fail();}
    pub(crate) fn abort(&mut self){if matches!(self.phase,Phase::Authorized|Phase::Sending){self.fail();}}
    pub(crate) fn submit(&mut self,packet:Packet)->Result<(),String>{
        self.require_active()?;
        if self.queue.push(packet).is_err(){self.fail();return Err("Invalid or overflowing encoded media; new session required".into())}
        Ok(())
    }
    pub(crate) fn tick(&mut self,elapsed_ms:u64)->Result<Tick,String>{
        self.require_active()?;
        let result=self.publisher.as_mut().unwrap().pump(&mut self.pacer,&mut self.queue,elapsed_ms);
        match result{
            Ok(t)=>{if t.sent>0{self.phase=Phase::Sending;}Ok(t)},
            Err(_)=>{self.fail_transport();Err("Publishing media pump failed; new session required".into())}
        }
    }
    /// Only call when the owner can read a complete inbound chunk within the
    /// configured I/O deadline. A read timeout is terminal, not retryable.
    pub(crate) fn service_inbound(&mut self)->Result<(),String>{
        self.require_active()?;
        if self.publisher.as_mut().unwrap().service_inbound().is_err(){
            self.fail();return Err("Publishing server or transport closed the session".into())
        }
        Ok(())
    }
    /// Explicit terminal stop: discard queued media and close the owned
    /// transport. Graceful RTMP unpublish commands are future work.
    pub(crate) fn stop(&mut self){
        if matches!(self.phase,Phase::Stopped|Phase::Failed){return}
        self.pacer.stop(&mut self.queue);
        self.publisher.take();
        self.phase=Phase::Stopped;
    }
}
#[cfg(any(target_os="macos",target_os="windows"))]
impl Session<native_tls::TlsStream<crate::rtmp_session::tls::DeadlineSocket>>{
    pub(crate) fn poll_tls_inbound(&mut self)->Result<(),String>{
        self.require_active()?;
        if self.publisher.as_mut().unwrap().poll_tls_inbound().is_err(){
            self.fail_transport();return Err("RTMPS inbound service failed; new session required".into())
        }
        Ok(())
    }
}
impl<T:Read+Write> Drop for Session<T>{fn drop(&mut self){self.stop()}}
