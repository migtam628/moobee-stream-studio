//! Native Mac/Windows Program-to-RTMPS sink. Same-thread native encoder; network I/O is
//! owned by the publisher worker. No file output and no UI command.
use crate::{live_program_encoder::Encoder,live_publish_worker::Worker,live_fanout::Fanout};
pub(crate) struct Sink{
    encoder:Encoder,fanout:Fanout,audio:bool,pending:Option<image::RgbaImage>,
    frames:u32,finished:bool,fps:u32,timing:crate::live_soak::Timing,
}
impl Sink{
    pub(crate) fn open(width:u32,height:u32,fps:u32,kbps:u32,audio:bool,
        connect:impl FnOnce()->Result<Worker,String>)->Result<Self,String>{
        Self::open_fanout(width,height,fps,kbps,audio,||Fanout::single(connect()?))
    }
    pub(crate) fn open_fanout(width:u32,height:u32,fps:u32,kbps:u32,audio:bool,
        connect:impl FnOnce()->Result<Fanout,String>)->Result<Self,String>{
        let encoder=Encoder::new(width,height,fps,kbps,audio)?;
        let fanout=connect()?;
        Ok(Self{encoder,fanout,audio,pending:None,frames:0,finished:false,fps,timing:crate::live_soak::Timing::default()})
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
        let start=std::time::Instant::now();
        let packets=self.encoder.tick(canvas.as_raw(),pcm)?;
        self.timing.observe(&packets,self.fps,self.audio,self.encoder.pending_audio_frames(),start.elapsed().as_millis() as u64);
        self.fanout.submit(packets)?;
        self.frames+=1;Ok(())
    }
    pub(crate) fn timing(&self)->crate::live_soak::Timing{self.timing}
    pub(crate) fn retry_failure(&self)->Option<crate::live_reconnect::Failure>{self.fanout.retry_failure()}
    pub(crate) fn destinations(&self)->Vec<crate::live_fanout::DestinationStats>{self.fanout.destinations()}
    pub(crate) fn finish(&mut self)->Result<(),String>{
        if self.finished{return Ok(())}
        if self.pending.is_some()||self.frames<2{return Err("Incomplete live Program timeline".into())}
        self.fanout.finish()?;
        self.finished=true;Ok(())
    }
    pub(crate) fn stats(&self)->Result<crate::live_publish_worker::WorkerStats,String>{self.fanout.stats()}
    pub(crate) fn sent_bytes(&self)->u64{self.fanout.stats().map_or(0,|s|s.session.sent_payload_bytes)}
    pub(crate) fn sent_packets(&self)->u64{self.fanout.stats().map_or(0,|s|s.session.sent_packets)}
}
