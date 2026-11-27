//! Mac Program-to-RTMPS sink. Same-thread native encoder; network I/O is
//! owned by the publisher worker. No file output and no UI command.
use crate::{live_program_encoder::Encoder,live_publish_worker::Worker,live_session::Phase};
use std::time::{Duration,Instant};
pub(crate) struct Sink{
    encoder:Encoder,worker:Worker,audio:bool,pending:Option<image::RgbaImage>,
    frames:u32,submitted_packets:u64,submitted_bytes:u64,finished:bool,
    retry_failure:Option<crate::live_reconnect::Failure>,
}
impl Sink{
    pub(crate) fn open(width:u32,height:u32,fps:u32,kbps:u32,audio:bool,
        connect:impl FnOnce()->Result<Worker,String>)->Result<Self,String>{
        let encoder=Encoder::new(width,height,fps,kbps,audio)?;
        let worker=connect()?;
        Ok(Self{encoder,worker,audio,pending:None,frames:0,submitted_packets:0,submitted_bytes:0,finished:false,retry_failure:None})
    }
    pub(crate) fn append(&mut self,canvas:image::RgbaImage)->Result<(),String>{
        if self.finished||self.pending.is_some(){return Err("Live Program tick is incomplete or closed".into())}
        if self.audio{self.pending=Some(canvas);Ok(())}else{self.encode(canvas,None)}
    }
    pub(crate) fn append_audio(&mut self,pcm:&[f32])->Result<(),String>{
        let canvas=self.pending.take().ok_or("No live video frame for audio tick")?;
        self.encode(canvas,Some(pcm))
    }
    fn encode(&mut self,canvas:image::RgbaImage,pcm:Option<&[f32]>)->Result<(),String>{
        if self.worker.is_finished(){self.note_terminal_transport();return Err("Live publishing worker ended".into())}
        let packets=self.encoder.tick(canvas.as_raw(),pcm)?;
        let count=packets.len() as u64;
        let bytes=packets.iter().map(|p|p.payload.len() as u64).sum::<u64>();
        if let Err(e)=self.worker.submit_batch(packets){self.note_terminal_transport();return Err(e)}
        self.submitted_packets+=count;self.submitted_bytes+=bytes;self.frames+=1;
        Ok(())
    }
    fn note_terminal_transport(&mut self){
        if let Ok(s)=self.worker.stats(){
            if s.session.phase==Phase::Failed&&s.session.failure==Some(crate::live_reconnect::Failure::TransientTransport){
                self.retry_failure=s.session.failure;
            }
        }
    }
    pub(crate) fn retry_failure(&self)->Option<crate::live_reconnect::Failure>{self.retry_failure}
    pub(crate) fn finish(&mut self)->Result<(),String>{
        if self.finished{return Ok(())}
        if self.pending.is_some()||self.frames<2{return Err("Incomplete live Program timeline".into())}
        self.worker.request_drain();
        let deadline=Instant::now()+Duration::from_secs(5);
        while !self.worker.reap()?{
            if Instant::now()>=deadline{self.worker.request_failure();return Err("Live publisher did not finish draining".into())}
            std::thread::sleep(Duration::from_millis(10));
        }
        let s=self.worker.stats()?;
        if s.session.phase!=Phase::Stopped||s.session.sent_packets!=self.submitted_packets||
            s.session.sent_payload_bytes!=self.submitted_bytes||s.pending_packets!=0||s.session.queued_packets!=0{
            return Err("Live publisher did not send all admitted packets".into())
        }
        self.finished=true;Ok(())
    }
    pub(crate) fn stats(&self)->Result<crate::live_publish_worker::WorkerStats,String>{self.worker.stats()}
    pub(crate) fn sent_bytes(&self)->u64{self.worker.stats().map_or(0,|s|s.session.sent_payload_bytes)}
    pub(crate) fn sent_packets(&self)->u64{self.worker.stats().map_or(0,|s|s.session.sent_packets)}
}
impl Drop for Sink{
    fn drop(&mut self){if !self.finished{self.worker.request_failure();}}
}
