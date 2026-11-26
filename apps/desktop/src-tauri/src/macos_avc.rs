//! Native Mac AVC access units; no external encoder executable or MP4 file.
use crate::stream_mux::{self,Packet};
use std::{ffi::{c_void,CStr},os::raw::c_char};
const CAPACITY:usize=2*1024*1024;
const CONFIG:usize=4096;
unsafe extern "C"{
    fn moobee_avc_create(w:i32,h:i32,fps:i32,kbps:i32,error:*mut c_char,capacity:i32)->*mut c_void;
    fn moobee_avc_encode(handle:*mut c_void,rgba:*const u8,length:usize,force_key:i32,out:*mut u8,capacity:usize,sps:*mut u8,pps:*mut u8,config_capacity:usize,sps_len:*mut usize,pps_len:*mut usize,key:*mut i32,error:*mut c_char,error_capacity:i32)->isize;
    fn moobee_avc_destroy(handle:*mut c_void);
}
fn message(error:&[c_char;512])->String{
    let s=unsafe{CStr::from_ptr(error.as_ptr())}.to_string_lossy();
    if s.is_empty(){"Native AVC encoder failed".into()}else{s.into_owned()}
}
fn split_avcc(mut data:&[u8])->Result<Vec<&[u8]>,String>{
    let mut nals=Vec::new();
    while !data.is_empty(){
        let n=u32::from_be_bytes(data.get(..4).ok_or("Truncated AVCC length")?.try_into().unwrap()) as usize;
        data=&data[4..];
        if n==0||n>data.len()||nals.len()>=128{return Err("Invalid AVCC access unit".into())}
        nals.push(&data[..n]);data=&data[n..];
    }
    if nals.is_empty(){return Err("Empty AVCC access unit".into())}
    Ok(nals)
}
pub(crate) struct Encoder{handle:*mut c_void,width:u32,height:u32,fps:u32,index:u32,output:Vec<u8>,config:Option<(Vec<u8>,Vec<u8>)>,failed:bool}
impl Encoder{
    pub(crate) fn new(width:u32,height:u32,fps:u32,kbps:u32)->Result<Self,String>{
        if !matches!((width,height,fps),(640,360,10)|(1280,720,15)|(1920,1080,30))||!matches!(kbps,1500|3000|6000){return Err("Unsupported native AVC profile".into())}
        let mut error=[0;512];
        let handle=unsafe{moobee_avc_create(width as i32,height as i32,fps as i32,kbps as i32,error.as_mut_ptr(),512)};
        if handle.is_null(){return Err(message(&error))}
        Ok(Self{handle,width,height,fps,index:0,output:vec![0;CAPACITY],config:None,failed:false})
    }
    pub(crate) fn encode(&mut self,rgba:&[u8],force_key:bool)->Result<Vec<Packet>,String>{
        if self.failed{return Err("AVC encoder requires a new session".into())}
        let result=self.encode_inner(rgba,force_key);
        if result.is_err(){self.failed=true}
        result
    }
    fn encode_inner(&mut self,rgba:&[u8],force_key:bool)->Result<Vec<Packet>,String>{
        if rgba.len()!=self.width as usize*self.height as usize*4||self.index>=self.fps*3600{return Err("Invalid AVC input frame or session limit".into())}
        let mut error=[0;512];let mut sps=[0u8;CONFIG];let mut pps=[0u8;CONFIG];let(mut sn,mut pn,mut key)=(0usize,0usize,0i32);
        let n=unsafe{moobee_avc_encode(self.handle,rgba.as_ptr(),rgba.len(),i32::from(force_key),self.output.as_mut_ptr(),self.output.len(),sps.as_mut_ptr(),pps.as_mut_ptr(),CONFIG,&mut sn,&mut pn,&mut key,error.as_mut_ptr(),512)};
        if n<=0{return Err(message(&error))}
        if n as usize>self.output.len()||sn==0||pn==0||sn>CONFIG||pn>CONFIG||!matches!(key,0|1){return Err("Invalid native AVC encoder output".into())}
        let mut packets=Vec::with_capacity(2);
        match &self.config{
            None=>{packets.push(stream_mux::avc_sequence(&sps[..sn],&pps[..pn])?);self.config=Some((sps[..sn].to_vec(),pps[..pn].to_vec()));}
            Some((a,b)) if a.as_slice()!=&sps[..sn]||b.as_slice()!=&pps[..pn]=>return Err("AVC configuration changed; restart publishing epoch".into()),
            _=>{}
        }
        let nals=split_avcc(&self.output[..n as usize])?;
        let idr=nals.iter().any(|nal|nal[0]&0x1f==5);
        if idr!=(key==1)||((self.index==0||force_key)&&!idr){return Err("Native AVC keyframe mismatch".into())}
        let time=(u64::from(self.index)*1000/u64::from(self.fps)) as u32;
        packets.push(stream_mux::avc_access_unit(&nals,time,0,idr)?);
        self.index+=1;Ok(packets)
    }
}
impl Drop for Encoder{fn drop(&mut self){unsafe{moobee_avc_destroy(self.handle)}}}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn avcc_lengths_are_bounded(){
        assert_eq!(split_avcc(&[0,0,0,2,0x65,1,0,0,0,1,0x41]).unwrap(),vec![&[0x65,1][..],&[0x41][..]]);
        for b in [&[][..],&[0,0,0,0][..],&[0,0,0,3,0x65][..],&[0,0,0][..]]{assert!(split_avcc(b).is_err())}
        assert!(Encoder::new(1920,1080,60,6000).is_err());
    }
    #[test]
    #[ignore="requires functioning native VideoToolbox encoder"]
    fn synthetic_avc_packets_enter_media_queue(){
        let mut e=Encoder::new(640,360,10,1500).unwrap();
        let mut q=crate::live_media_queue::MediaQueue::default();
        for i in 0..3u8{
            let frame=image::RgbaImage::from_pixel(640,360,image::Rgba([i*20,50,100,255]));
            for p in e.encode(frame.as_raw(),i==0).unwrap(){q.push(p).unwrap();}
        }
        assert_eq!(q.stats().queued_packets,4);
        assert_eq!(q.pop().unwrap().payload[1],0);
        let key=q.pop().unwrap();assert_eq!(key.payload[0],0x17);assert_eq!(key.timestamp_ms,0);
        assert_eq!(q.pop().unwrap().timestamp_ms,100);
    }
}
