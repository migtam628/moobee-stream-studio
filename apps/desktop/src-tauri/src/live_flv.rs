//! Local-only diagnostic of the exact AVC/AAC media packets intended for RTMPS.
use crate::{live_media_queue::MediaQueue,stream_mux::Packet};
use std::io::Write;
const LIMIT:u64=64*1024*1024;
pub(crate) struct Writer<W:Write>{out:W,queue:MediaQueue,bytes:u64,video:u32,audio:u32,with_audio:bool,failed:bool}
impl<W:Write> Writer<W>{
    pub(crate) fn new(mut out:W,with_audio:bool)->Result<Self,String>{
        out.write_all(&[b'F',b'L',b'V',1,if with_audio{5}else{1},0,0,0,9,0,0,0,0]).map_err(|e|e.to_string())?;
        Ok(Self{out,queue:MediaQueue::default(),bytes:13,video:0,audio:0,with_audio,failed:false})
    }
    pub(crate) fn append(&mut self,packet:Packet)->Result<(),String>{
        if self.failed{return Err("FLV diagnostic failed".into())}
        let result=self.append_inner(packet);
        if result.is_err(){self.failed=true}
        result
    }
    fn append_inner(&mut self,p:Packet)->Result<(),String>{
        if p.kind==8&&!self.with_audio{return Err("Unexpected AAC packet".into())}
        self.queue.push(p)?;
        let p=self.queue.pop().ok_or("Missing validated FLV packet")?;
        let tag=p.flv_tag()?;
        if self.bytes+tag.len() as u64>LIMIT{return Err("FLV exceeds 64 MiB".into())}
        self.out.write_all(&tag).map_err(|e|e.to_string())?;
        self.bytes+=tag.len() as u64;
        if p.payload[1]==1{if p.kind==9{self.video+=1;}else{self.audio+=1;}}
        Ok(())
    }
    pub(crate) fn finish(&mut self)->Result<u64,String>{
        if self.failed||self.video<2||(self.with_audio&&self.audio==0){return Err("Incomplete live packet diagnostic".into())}
        self.out.flush().map_err(|e|e.to_string())?;Ok(self.bytes)
    }
}
#[cfg(target_os="macos")]
pub(crate) struct NativeSink{
    writer:Writer<std::fs::File>,encoder:crate::live_program_encoder::Encoder,
    audio:bool,pending:Option<image::RgbaImage>,frames:u32,
}
#[cfg(target_os="macos")]
impl NativeSink{
    pub(crate) fn open(path:&std::path::Path,width:u32,height:u32,fps:u32,kbps:u32,audio:bool)->Result<Self,String>{
        let encoder=crate::live_program_encoder::Encoder::new(width,height,fps,kbps,audio)?;
        let file=std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e|e.to_string())?;
        Ok(Self{writer:Writer::new(file,audio)?,encoder,audio,pending:None,frames:0})
    }
    pub(crate) fn append(&mut self,canvas:image::RgbaImage)->Result<(),String>{
        if self.pending.is_some(){return Err("Previous FLV tick is missing audio".into())}
        if self.audio{self.pending=Some(canvas);Ok(())}else{self.encode(canvas,None)}
    }
    pub(crate) fn append_audio(&mut self,pcm:&[f32])->Result<(),String>{
        let canvas=self.pending.take().ok_or("No video frame for FLV audio tick")?;
        self.encode(canvas,Some(pcm))
    }
    fn encode(&mut self,canvas:image::RgbaImage,pcm:Option<&[f32]>)->Result<(),String>{
        for packet in self.encoder.tick(canvas.as_raw(),pcm)?{self.writer.append(packet)?;}
        self.frames+=1;Ok(())
    }
    pub(crate) fn finish(&mut self)->Result<(),String>{
        if self.pending.is_some()||self.frames!=self.writer.video{return Err("Incomplete FLV frame timeline".into())}
        self.writer.finish()?;self.writer.out.sync_all().map_err(|e|e.to_string())
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    use crate::stream_mux::{avc_sequence,avc_access_unit,aac_sequence,aac_access_unit};
    #[test]fn local_flv_header_and_validated_tags(){
        let mut w=Writer::new(Vec::new(),true).unwrap();
        w.append(avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap()).unwrap();
        w.append(aac_sequence()).unwrap();
        w.append(avc_access_unit(&[&[0x65,1]],0,0,true).unwrap()).unwrap();
        w.append(aac_access_unit(&[1,2],0).unwrap()).unwrap();
        w.append(avc_access_unit(&[&[0x41,2]],100,0,false).unwrap()).unwrap();
        assert_eq!(w.finish().unwrap() as usize,w.out.len());
        assert_eq!(&w.out[..13],&[b'F',b'L',b'V',1,5,0,0,0,9,0,0,0,0]);
        assert_eq!(w.video,2);assert_eq!(w.audio,1);
    }
    #[test]fn rejects_incomplete_or_invalid_packet_stream(){
        let mut w=Writer::new(Vec::new(),false).unwrap();
        assert!(w.append(avc_access_unit(&[&[0x65,1]],0,0,true).unwrap()).is_err());
        assert!(w.finish().is_err());
        let mut w=Writer::new(Vec::new(),false).unwrap();
        assert!(w.append(aac_sequence()).is_err());
        assert!(w.finish().is_err());
    }
}
