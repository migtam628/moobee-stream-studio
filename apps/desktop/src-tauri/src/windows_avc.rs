//! Direct synchronous Windows Media Foundation H.264 packets.
//! No local container, external encoder process, or network output.
use crate::{stream_mux::{self,Packet},windows_mp4_sink::rgba_to_nv12};
use std::{ffi::{c_void,CStr},os::raw::c_char};
const MAX:usize=2*1024*1024;
const CONFIG:usize=8192;
unsafe extern "C"{
    fn moobee_win_avc_create(w:u32,h:u32,fps:u32,kbps:u32,error:*mut c_char,capacity:i32)->*mut c_void;
    fn moobee_win_avc_encode(handle:*mut c_void,nv12:*const u8,length:usize,force_key:i32,out:*mut u8,capacity:u32,size:*mut u32,config:*mut u8,config_cap:u32,config_len:*mut u32,error:*mut c_char,error_capacity:i32)->i32;
    fn moobee_win_avc_destroy(handle:*mut c_void);
}
fn message(error:&[c_char;512])->String{
    let s=unsafe{CStr::from_ptr(error.as_ptr())}.to_string_lossy();
    if s.is_empty(){"Windows native AVC encoder failed".into()}else{s.into_owned()}
}
// Scan each byte at most once. Accept three/four-byte start codes and zero
// padding, reject empty units, nonzero preamble and invalid NAL headers.
fn annex_b(data:&[u8])->Result<Vec<&[u8]>,String>{
    if data.len()>MAX||data.is_empty(){return Err("Invalid Annex B buffer".into())}
    let mut starts=Vec::new();let mut zeros=0usize;
    for (i,&byte) in data.iter().enumerate(){
        if byte==0{zeros+=1;continue}
        if byte==1&&zeros>=2{
            starts.push((i-zeros,i+1));
            if starts.len()>128{return Err("Too many Annex B NAL units".into())}
        }
        zeros=0;
    }
    let Some(first)=starts.first()else{return Err("Missing Annex B start code".into())};
    if data[..first.0].iter().any(|b|*b!=0){return Err("Invalid Annex B preamble".into())}
    let mut nals=Vec::with_capacity(starts.len());
    for (i,&(_,start)) in starts.iter().enumerate(){
        let end=starts.get(i+1).map_or(data.len().saturating_sub(zeros),|s|s.0);
        let nal=data.get(start..end).filter(|n|!n.is_empty()).ok_or("Empty Annex B unit")?;
        if nal[0]&0x80!=0||!(1..=23).contains(&(nal[0]&31)){return Err("Invalid Annex B NAL header".into())}
        nals.push(nal);
    }
    Ok(nals)
}
#[derive(Default)]
struct Packetizer{sps:Option<Vec<u8>>,pps:Option<Vec<u8>>,index:u32}
impl Packetizer{
    fn parameter_sets(&mut self,nals:&[&[u8]])->Result<(),String>{
        for nal in nals{
            let slot=match nal[0]&31{7=>&mut self.sps,8=>&mut self.pps,_=>continue};
            if nal.len()>4096{return Err("Oversized AVC parameter set".into())}
            if let Some(old)=slot.as_ref(){
                if old.as_slice()!=*nal{return Err("AVC parameter set changed; restart epoch".into())}
            }else{*slot=Some(nal.to_vec())}
        }
        Ok(())
    }
    fn frame(&mut self,config:&[u8],bytes:&[u8],fps:u32,force:bool)->Result<Vec<Packet>,String>{
        if !config.is_empty(){self.parameter_sets(&annex_b(config)?)?;}
        let nals=annex_b(bytes)?;
        self.parameter_sets(&nals)?;
        let sps=self.sps.as_ref().ok_or("Missing Windows AVC SPS")?;
        let pps=self.pps.as_ref().ok_or("Missing Windows AVC PPS")?;
        let media=nals.into_iter().filter(|n|!matches!(n[0]&31,7|8)).collect::<Vec<_>>();
        let idr=media.iter().any(|n|n[0]&31==5);
        if !media.iter().any(|n|matches!(n[0]&31,1|5))||((self.index==0||force)&&!idr){
            return Err("Missing AVC picture or required IDR".into())
        }
        let mut packets=Vec::with_capacity(2);
        if self.index==0{packets.push(stream_mux::avc_sequence(sps,pps)?);}
        packets.push(stream_mux::avc_access_unit(&media,(u64::from(self.index)*1000/u64::from(fps)) as u32,0,idr)?);
        if packets.iter().any(|p|p.payload.len()>MAX){return Err("Oversized Windows AVC packet".into())}
        self.index+=1;Ok(packets)
    }
}
pub(crate) struct Encoder{handle:*mut c_void,width:u32,height:u32,fps:u32,nv12:Vec<u8>,output:Vec<u8>,packetizer:Packetizer,failed:bool}
impl Encoder{
    pub(crate) fn new(w:u32,h:u32,fps:u32,kbps:u32)->Result<Self,String>{
        if !matches!((w,h,fps),(640,360,10)|(1280,720,15)|(1920,1080,30))||!matches!(kbps,1500|3000|6000){return Err("Unsupported Windows AVC profile".into())}
        let mut error=[0;512];
        let handle=unsafe{moobee_win_avc_create(w,h,fps,kbps,error.as_mut_ptr(),512)};
        if handle.is_null(){return Err(message(&error))}
        Ok(Self{handle,width:w,height:h,fps,nv12:vec![0;(w*h*3/2)as usize],output:vec![0;MAX],packetizer:Packetizer::default(),failed:false})
    }
    pub(crate) fn encode(&mut self,rgba:&[u8],force_key:bool)->Result<Vec<Packet>,String>{
        if self.failed{return Err("Windows AVC encoder requires fresh epoch".into())}
        let result=self.encode_inner(rgba,force_key);
        if result.is_err(){self.failed=true}
        result
    }
    fn encode_inner(&mut self,rgba:&[u8],force_key:bool)->Result<Vec<Packet>,String>{
        rgba_to_nv12(rgba,self.width as usize,self.height as usize,&mut self.nv12)?;
        let mut error=[0;512];let mut config=[0u8;CONFIG];let(mut n,mut cn)=(0u32,0u32);
        let force=force_key||self.packetizer.index==0;
        let rc=unsafe{moobee_win_avc_encode(self.handle,self.nv12.as_ptr(),self.nv12.len(),i32::from(force),self.output.as_mut_ptr(),MAX as u32,&mut n,config.as_mut_ptr(),CONFIG as u32,&mut cn,error.as_mut_ptr(),512)};
        if rc!=0{return Err(message(&error))}
        if n==0||n as usize>MAX||cn as usize>CONFIG{return Err("Invalid Windows AVC output bounds".into())}
        self.packetizer.frame(&config[..cn as usize],&self.output[..n as usize],self.fps,force)
    }
}
impl Drop for Encoder{fn drop(&mut self){unsafe{moobee_win_avc_destroy(self.handle)}}}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn annex_b_and_packet_epoch_validation(){
        let config=[0,0,0,1,0x67,0x42,0,0x28,0,0,1,0x68,0xee];
        let key=[0,0,1,0x65,0xaa];
        let inter=[0,0,0,1,0x41,0xbb];
        assert_eq!(annex_b(&config).unwrap(),vec![&config[4..8],&config[11..]]);
        for b in [&[][..],&[1,2,3][..],&[0,0,1][..],&[2,0,0,1,0x65][..],&[0,0,1,0xe5][..]]{assert!(annex_b(b).is_err());}
        let mut p=Packetizer::default();let mut q=crate::live_media_queue::MediaQueue::default();
        for packet in p.frame(&config,&key,10,true).unwrap(){q.push(packet).unwrap();}
        for packet in p.frame(&config,&inter,10,false).unwrap(){q.push(packet).unwrap();}
        assert_eq!(q.stats().queued_packets,3);
        assert_eq!(q.pop().unwrap().payload[1],0);assert_eq!(q.pop().unwrap().timestamp_ms,0);assert_eq!(q.pop().unwrap().timestamp_ms,100);
        let mut changed=config;changed[7]=0x29;
        assert!(p.frame(&changed,&key,10,true).is_err());
        assert!(Packetizer::default().frame(&config,&inter,10,false).is_err());
    }
    #[test]
    #[ignore="requires functioning synchronous Windows Media Foundation H264 encoder"]
    fn native_windows_avc_into_live_queue(){
        let mut e=Encoder::new(640,360,10,1500).unwrap();
        let mut q=crate::live_media_queue::MediaQueue::default();
        for i in 0..21u8{
            let frame=image::RgbaImage::from_pixel(640,360,image::Rgba([i*4,60,90,255]));
            for p in e.encode(frame.as_raw(),i==0||i==20).unwrap(){q.push(p).unwrap();}
            while q.pop().is_some(){}
        }
        assert_eq!(q.stats().accepted_packets,22);
    }
}
