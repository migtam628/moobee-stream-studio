//! Same-thread producer for composed RGBA and aligned mixed PCM.
//! The caller owns native capture, timing and user authorization.
use crate::{macos_avc,macos_aac,media_clock::MediaClock,stream_mux::Packet,live_publish_worker::Worker};
pub(crate) struct Encoder{
    video:macos_avc::Encoder,audio:Option<macos_aac::Encoder>,clock:MediaClock,
    fps:u32,tick:u32,failed:bool,
}
impl Encoder{
    pub(crate) fn new(width:u32,height:u32,fps:u32,kbps:u32,audio:bool)->Result<Self,String>{
        Ok(Self{video:macos_avc::Encoder::new(width,height,fps,kbps)?,
            audio:if audio{Some(macos_aac::Encoder::new()?)}else{None},
            clock:MediaClock::new(fps)?,fps,tick:0,failed:false})
    }
    pub(crate) fn tick(&mut self,rgba:&[u8],pcm:Option<&[f32]>)->Result<Vec<Packet>,String>{
        if self.failed{return Err("Live encoder requires a new epoch".into())}
        let result=self.encode_tick(rgba,pcm);
        if result.is_err(){self.failed=true}
        result
    }
    fn encode_tick(&mut self,rgba:&[u8],pcm:Option<&[f32]>)->Result<Vec<Packet>,String>{
        let range=self.clock.audio_range(self.tick,48000)?;
        let expected=(range.end-range.start) as usize*2;
        if self.audio.is_some()!=pcm.is_some()||pcm.is_some_and(|samples|samples.len()!=expected||samples.iter().any(|v|!v.is_finite()||v.abs()>1.)){
            return Err("Live PCM must match the current video tick".into())
        }
        let mut packets=self.video.encode(rgba,self.tick%(self.fps*2)==0)?;
        if let Some(a)=self.audio.as_mut(){
            if self.tick==0{
                // Configuration must precede both tracks' media; first video
                // remains IDR and is sent before the first AAC access unit.
                packets.insert(1,a.configuration());
            }
            packets.extend(a.push(pcm.unwrap())?);
        }
        self.tick+=1;Ok(packets)
    }
    pub(crate) fn submit_tick(&mut self,worker:&Worker,rgba:&[u8],pcm:Option<&[f32]>)->Result<(),String>{
        let result=self.tick(rgba,pcm).and_then(|batch|worker.submit_batch(batch));
        if result.is_err(){self.failed=true;worker.request_failure();}
        result
    }
    pub(crate) fn pending_audio_frames(&self)->u64{self.audio.as_ref().map_or(0,|a|a.pending_frames())}
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    #[ignore="requires native macOS AVC and AAC encoders"]
    fn composed_ticks_produce_queue_compatible_av_packets(){
        let mut e=Encoder::new(640,360,10,1500,true).unwrap();
        let mut q=crate::live_media_queue::MediaQueue::default();
        let rgba=image::RgbaImage::from_pixel(640,360,image::Rgba([30,60,90,255]));
        let mut video=0;let mut audio=0;
        for tick in 0..20{
            let pcm=vec![0.1;9600];
            for packet in e.tick(rgba.as_raw(),Some(&pcm)).unwrap(){
                if packet.kind==9&&packet.payload[1]==1{video+=1;assert_eq!(packet.timestamp_ms,tick*100);}
                if packet.kind==8&&packet.payload[1]==1{audio+=1;}
                q.push(packet).unwrap();
            }
            while q.pop().is_some(){}
        }
        assert_eq!(video,20);assert!(audio>80);assert!(e.pending_audio_frames()<9216);
        assert!(e.tick(rgba.as_raw(),None).is_err());
        assert!(e.tick(rgba.as_raw(),Some(&vec![0.1;9600])).is_err());
    }
}
