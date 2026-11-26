//! Bounded per-destination encoded media queue.
//! A queue failure requires a new publishing epoch, not silent frame dropping.
use crate::stream_mux::Packet;
use std::collections::VecDeque;
const MAX_BYTES:usize=8*1024*1024;
const MAX_PACKETS:usize=512;
const MAX_PACKET:usize=2*1024*1024;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub(crate) struct QueueStats{pub queued_packets:usize,pub queued_bytes:usize,pub accepted_packets:u64,pub dequeued_packets:u64,pub restart_required:bool}
pub(crate) struct MediaQueue{
    queue:VecDeque<Packet>,bytes:usize,accepted:u64,dequeued:u64,failed:bool,
    video_config:bool,audio_config:bool,keyframe:bool,last_video:Option<u32>,last_audio:Option<u32>,
}
impl Default for MediaQueue{
    fn default()->Self{Self{queue:VecDeque::new(),bytes:0,accepted:0,dequeued:0,failed:false,video_config:false,audio_config:false,keyframe:false,last_video:None,last_audio:None}}
}
impl MediaQueue{
    pub(crate) fn stats(&self)->QueueStats{
        QueueStats{queued_packets:self.queue.len(),queued_bytes:self.bytes,accepted_packets:self.accepted,dequeued_packets:self.dequeued,restart_required:self.failed}
    }
    pub(crate) fn fail(&mut self){self.queue.clear();self.bytes=0;self.failed=true;}
    pub(crate) fn push(&mut self,packet:Packet)->Result<(),String>{
        if self.failed{return Err("Media queue requires a new publishing epoch".into())}
        let result=self.validate(&packet);
        if let Err(e)=result{self.fail();return Err(e)}
        if self.queue.len()>=MAX_PACKETS||self.bytes+packet.payload.len()>MAX_BYTES{
            self.fail();return Err("Media queue capacity exceeded; restart at a new keyframe".into())
        }
        self.bytes+=packet.payload.len();self.queue.push_back(packet);self.accepted=self.accepted.saturating_add(1);Ok(())
    }
    fn validate(&mut self,p:&Packet)->Result<(),String>{
        let b=&p.payload;
        if b.len()<2||b.len()>MAX_PACKET{return Err("Invalid encoded media packet size".into())}
        match (p.kind,b[0],b[1]){
            (9,0x17,0)=>{
                if b.len()<16||self.video_config||p.timestamp_ms!=0||b[2..5]!=[0,0,0]||b[5]!=1{return Err("Invalid or repeated AVC configuration".into())}
                self.video_config=true;
            }
            (8,0xaf,0)=>{
                if b.as_slice()!=[0xaf,0,0x11,0x90]||self.audio_config||p.timestamp_ms!=0{return Err("Invalid or repeated AAC configuration".into())}
                self.audio_config=true;
            }
            (9,0x17|0x27,1)=>{
                if !self.video_config||b.len()<10||self.last_video.is_some_and(|last|p.timestamp_ms<last){return Err("AVC packet missing configuration or out of order".into())}
                // Verify length-prefixed NAL boundaries and require IDR for
                // the first keyframe rather than trusting the FLV flag alone.
                let mut pos=5;let mut idr=false;let mut count=0;
                while pos<b.len(){
                    let end=pos+4;
                    let n=u32::from_be_bytes(b.get(pos..end).ok_or("Truncated AVC NAL length")?.try_into().unwrap()) as usize;
                    pos=end;
                    let end=pos.checked_add(n).ok_or("AVC NAL length overflow")?;
                    let nal=b.get(pos..end).ok_or("Truncated AVC NAL")?;
                    if nal.is_empty()||nal[0]&0x80!=0{return Err("Invalid AVC NAL".into())}
                    idr|=nal[0]&0x1f==5;pos=end;count+=1;if count>128{return Err("Too many AVC NAL units".into())}
                }
                if !self.keyframe&&!(b[0]==0x17&&idr){return Err("Publishing requires an initial AVC IDR keyframe".into())}
                if b[0]==0x17&&!idr{return Err("AVC keyframe is not an IDR".into())}
                if b[0]==0x27&&idr{return Err("AVC IDR is mislabeled as interframe".into())}
                self.keyframe=true;self.last_video=Some(p.timestamp_ms);
            }
            (8,0xaf,1)=>{
                if !self.audio_config||!self.keyframe||b.len()<3||self.last_audio.is_some_and(|last|p.timestamp_ms<last){return Err("AAC packet missing configuration, video start or monotonic timestamp".into())}
                if b.len()>=4&&b[2]==0xff&&b[3]&0xf6==0xf0{return Err("AAC packet must be raw AAC".into())}
                self.last_audio=Some(p.timestamp_ms);
            }
            _=>return Err("Unsupported live media packet".into())
        }
        Ok(())
    }
    pub(crate) fn peek(&self)->Option<&Packet>{if self.failed{None}else{self.queue.front()}}
    pub(crate) fn pop(&mut self)->Option<Packet>{
        if self.failed{return None}
        let p=self.queue.pop_front()?;self.bytes-=p.payload.len();self.dequeued=self.dequeued.saturating_add(1);Some(p)
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    use crate::stream_mux::{avc_sequence,avc_access_unit,aac_sequence,aac_access_unit};
    fn config()->Packet{avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap()}
    fn key(time:u32)->Packet{avc_access_unit(&[&[0x65,1]],time,0,true).unwrap()}
    fn inter(time:u32)->Packet{avc_access_unit(&[&[0x41,2]],time,0,false).unwrap()}
    #[test]fn requires_configuration_and_initial_idr(){
        let mut q=MediaQueue::default();assert!(q.push(key(0)).is_err());assert!(q.stats().restart_required);
        let mut q=MediaQueue::default();q.push(config()).unwrap();assert!(q.push(inter(0)).is_err());
        let mut q=MediaQueue::default();q.push(config()).unwrap();q.push(aac_sequence()).unwrap();q.push(key(0)).unwrap();q.push(aac_access_unit(&[1,2],0).unwrap()).unwrap();
        assert_eq!(q.stats().queued_packets,4);
        assert_eq!(q.pop().unwrap().payload[1],0);
        assert_eq!(q.stats().dequeued_packets,1);
        assert_eq!(q.stats().queued_packets,3);
    }
    #[test]fn rejects_backward_timestamps_and_invalid_nal(){
        let mut q=MediaQueue::default();q.push(config()).unwrap();q.push(key(20)).unwrap();assert!(q.push(inter(10)).is_err());assert!(q.pop().is_none());
        let mut q=MediaQueue::default();q.push(config()).unwrap();let mut p=key(0);p.payload[8]=255;assert!(q.push(p).is_err());
    }
    #[test]fn overflow_fails_closed_and_clears_queue(){
        let mut q=MediaQueue::default();q.push(config()).unwrap();q.push(key(0)).unwrap();
        for i in 0..MAX_PACKETS-2{q.push(inter(i as u32+1)).unwrap();}
        assert_eq!(q.stats().queued_packets,MAX_PACKETS);
        assert!(q.push(inter(999)).is_err());assert_eq!(q.stats().queued_bytes,0);assert!(q.stats().restart_required);
        assert!(q.push(config()).is_err());
    }
}
