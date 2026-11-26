//! Live AAC-LC bridge: align mixed PCM to 1024-sample encoder blocks.
use crate::stream_mux::{self,Packet};
use std::{ffi::{c_void,CStr},os::raw::c_char};
const FRAMES:usize=1024;
const CAPACITY:usize=4*(16384+4);
unsafe extern "C"{
    fn moobee_aac_create(error:*mut c_char,capacity:i32)->*mut c_void;
    fn moobee_aac_encode(handle:*mut c_void,pcm:*const f32,frames:usize,out:*mut u8,capacity:usize,error:*mut c_char,error_capacity:i32)->isize;
    fn moobee_aac_destroy(handle:*mut c_void);
}
fn detail(error:&[c_char;512])->String{
    let s=unsafe{CStr::from_ptr(error.as_ptr())}.to_string_lossy();
    if s.is_empty(){"Native AAC encoder failed".into()}else{s.into_owned()}
}
fn packets(mut b:&[u8])->Result<Vec<&[u8]>,String>{
    let mut result=Vec::new();
    while !b.is_empty(){
        let size=u32::from_be_bytes(b.get(..4).ok_or("Truncated AAC packet length")?.try_into().unwrap()) as usize;
        b=&b[4..];
        if size==0||size>16384||size>b.len()||result.len()>=4{return Err("Invalid AAC packet geometry".into())}
        result.push(&b[..size]);b=&b[size..];
    }
    Ok(result)
}
pub(crate) struct Encoder{handle:*mut c_void,pcm:Vec<f32>,scratch:Vec<u8>,submitted:u64,emitted:u64,failed:bool}
impl Encoder{
    pub(crate) fn new()->Result<Self,String>{
        let mut error=[0;512];let handle=unsafe{moobee_aac_create(error.as_mut_ptr(),512)};
        if handle.is_null(){return Err(detail(&error))}
        Ok(Self{handle,pcm:Vec::with_capacity(11648),scratch:vec![0;CAPACITY],submitted:0,emitted:0,failed:false})
    }
    /// Configuration must be queued before media. Encoder startup delay and
    /// final partial block discard are not claimed to be sample-accurate.
    pub(crate) fn configuration(&self)->Packet{stream_mux::aac_sequence()}
    pub(crate) fn push(&mut self,pcm:&[f32])->Result<Vec<Packet>,String>{
        if self.failed{return Err("AAC encoder requires a new publishing epoch".into())}
        let result=self.push_inner(pcm);
        if result.is_err(){self.failed=true;self.pcm.clear()}
        result
    }
    fn push_inner(&mut self,pcm:&[f32])->Result<Vec<Packet>,String>{
        if pcm.is_empty()||pcm.len()%2!=0||pcm.len()>9600||pcm.iter().any(|v|!v.is_finite()||v.abs()>1.){return Err("Invalid bounded stereo PCM block".into())}
        if self.submitted+self.pcm.len() as u64/2+pcm.len() as u64/2>48000*3600{return Err("AAC publishing epoch exceeds one hour".into())}
        self.pcm.extend_from_slice(pcm);
        let mut out=Vec::new();
        while self.pcm.len()>=FRAMES*2{
            let mut error=[0;512];
            let n=unsafe{moobee_aac_encode(self.handle,self.pcm.as_ptr(),FRAMES,self.scratch.as_mut_ptr(),self.scratch.len(),error.as_mut_ptr(),512)};
            if n<0{return Err(detail(&error))}
            if n as usize>self.scratch.len(){return Err("Invalid AAC bridge output length".into())}
            self.pcm.drain(..FRAMES*2);
            self.submitted+=FRAMES as u64;
            for raw in packets(&self.scratch[..n as usize])?{
                if self.emitted+FRAMES as u64>self.submitted{return Err("AAC output exceeds submitted sample timeline".into())}
                let time=(self.emitted*1000/48000) as u32;
                out.push(stream_mux::aac_access_unit(raw,time)?);
                self.emitted+=FRAMES as u64;
            }
            if self.submitted-self.emitted>8192{return Err("AAC encoder is not producing timely output".into())}
        }
        Ok(out)
    }
    pub(crate) fn pending_frames(&self)->u64{self.submitted-self.emitted+self.pcm.len() as u64/2}
}
impl Drop for Encoder{fn drop(&mut self){unsafe{moobee_aac_destroy(self.handle)}}}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn bounded_packet_extraction(){
        assert_eq!(packets(&[0,0,0,2,1,2,0,0,0,1,3]).unwrap(),vec![&[1,2][..],&[3][..]]);
        assert!(packets(&[]).unwrap().is_empty());
        for b in [&[0,0,0,0][..],&[0,0,0,2,1][..],&[0,0,0][..]]{assert!(packets(b).is_err())}
    }
    #[test]
    #[ignore="requires native macOS AAC encoder"]
    fn synthetic_pcm_produces_raw_aac_packets(){
        let mut e=Encoder::new().unwrap();
        let mut q=crate::live_media_queue::MediaQueue::default();
        q.push(crate::stream_mux::avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap()).unwrap();
        q.push(e.configuration()).unwrap();
        q.push(crate::stream_mux::avc_access_unit(&[&[0x65,1]],0,0,true).unwrap()).unwrap();
        let mut times=Vec::new();
        for tick in 0..20{
            let mut pcm=Vec::new();
            for i in 0..4800{let v=0.1*(2.*std::f32::consts::PI*440.*(tick*4800+i) as f32/48000.).sin();pcm.extend([v,v]);}
            for p in e.push(&pcm).unwrap(){times.push(p.timestamp_ms);q.push(p).unwrap();}
        }
        assert!(times.len()>80);
        assert_eq!(times[0],0);
        assert!(times.windows(2).all(|p|p[1]-p[0]>=21&&p[1]-p[0]<=22));
        assert!(e.pending_frames()<9216);
    }
}
