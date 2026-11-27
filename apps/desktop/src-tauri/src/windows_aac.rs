//! Native AAC-LC 48 kHz stereo packets for the shared live media muxer.
use crate::stream_mux::{self,Packet};
use std::{ffi::{c_void,CStr},os::raw::c_char};
const FRAMES:usize=1024;
const CAP:usize=16384;
unsafe extern "C"{
    fn moobee_win_aac_create(error:*mut c_char,capacity:i32)->*mut c_void;
    fn moobee_win_aac_push(handle:*mut c_void,pcm:*const i16,frames:usize,error:*mut c_char,capacity:i32)->i32;
    fn moobee_win_aac_poll(handle:*mut c_void,out:*mut u8,capacity:u32,size:*mut u32,pts:*mut i64,error:*mut c_char,error_capacity:i32)->i32;
    fn moobee_win_aac_destroy(handle:*mut c_void);
}
fn detail(error:&[c_char;512])->String{
    let s=unsafe{CStr::from_ptr(error.as_ptr())}.to_string_lossy();
    if s.is_empty(){"Windows native AAC encoder failed".into()}else{s.into_owned()}
}
#[derive(Default)]
struct Timeline{submitted:u64,emitted:u64,origin:Option<i64>}
impl Timeline{
    fn emit(&mut self,pts:i64,raw:&[u8])->Result<Packet,String>{
        if self.emitted+FRAMES as u64>self.submitted{return Err("AAC output exceeds submitted sample timeline".into())}
        let origin=*self.origin.get_or_insert(pts);
        let relative=pts.checked_sub(origin).ok_or("AAC timestamp overflow")?;
        let expected=(self.emitted*10_000_000/48_000) as i64;
        // Check native spacing, allowing 1 ms of timestamp rounding. The
        // wire clock is nominal and does not claim compensation for priming.
        if relative.abs_diff(expected)>10_000{return Err("Unexpected AAC output timestamp spacing".into())}
        let p=stream_mux::aac_access_unit(raw,(self.emitted*1000/48000) as u32)?;
        self.emitted+=FRAMES as u64;Ok(p)
    }
}
pub(crate) struct Encoder{handle:*mut c_void,pcm:Vec<i16>,output:Vec<u8>,timeline:Timeline,failed:bool}
impl Encoder{
    pub(crate) fn new()->Result<Self,String>{
        let mut error=[0;512];let handle=unsafe{moobee_win_aac_create(error.as_mut_ptr(),512)};
        if handle.is_null(){return Err(detail(&error))}
        Ok(Self{handle,pcm:Vec::with_capacity(11648),output:vec![0;CAP],timeline:Timeline::default(),failed:false})
    }
    pub(crate) fn configuration(&self)->Packet{stream_mux::aac_sequence()}
    pub(crate) fn push(&mut self,pcm:&[f32])->Result<Vec<Packet>,String>{
        if self.failed{return Err("Windows AAC encoder requires fresh epoch".into())}
        let result=self.push_inner(pcm);
        if result.is_err(){self.failed=true;self.pcm.clear()}
        result
    }
    fn push_inner(&mut self,pcm:&[f32])->Result<Vec<Packet>,String>{
        if pcm.is_empty()||pcm.len()%2!=0||pcm.len()>9600||pcm.iter().any(|v|!v.is_finite()||v.abs()>1.){return Err("Invalid stereo PCM block".into())}
        if self.timeline.submitted+(self.pcm.len()+pcm.len()) as u64/2>48000*u64::from(crate::live_acceptance::MAX_LIVE_SECONDS){return Err("AAC publishing epoch exceeds two hours".into())}
        self.pcm.extend(crate::windows_mp4_sink::pcm16(pcm)?);
        let mut packets=Vec::new();
        while self.pcm.len()>=FRAMES*2{
            let mut error=[0;512];
            if unsafe{moobee_win_aac_push(self.handle,self.pcm.as_ptr(),FRAMES,error.as_mut_ptr(),512)}!=0{return Err(detail(&error))}
            self.pcm.drain(..FRAMES*2);self.timeline.submitted+=FRAMES as u64;
            let mut drained=false;
            for _ in 0..9{
                let(mut size,mut pts)=(0u32,0i64);
                let rc=unsafe{moobee_win_aac_poll(self.handle,self.output.as_mut_ptr(),CAP as u32,&mut size,&mut pts,error.as_mut_ptr(),512)};
                if rc==0{drained=true;break}
                if rc!=1{return Err(detail(&error))}
                if size==0||size as usize>CAP||packets.len()>=16{return Err("Invalid or excessive Windows AAC packets".into())}
                packets.push(self.timeline.emit(pts,&self.output[..size as usize])?);
            }
            if !drained||self.timeline.submitted-self.timeline.emitted>8192{return Err("AAC encoder not producing timely bounded output".into())}
        }
        Ok(packets)
    }
    pub(crate) fn pending_frames(&self)->u64{self.timeline.submitted-self.timeline.emitted+self.pcm.len() as u64/2}
}
impl Drop for Encoder{fn drop(&mut self){unsafe{moobee_win_aac_destroy(self.handle)}}}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn sample_clock_rejects_excessive_or_reordered_packets(){
        let mut t=Timeline{submitted:2048,..Timeline::default()};
        let p=t.emit(-213333,&[1,2]).unwrap();assert_eq!(p.timestamp_ms,0);
        let p=t.emit(0,&[3,4]).unwrap();assert_eq!(p.timestamp_ms,21);
        assert!(t.emit(213333,&[5]).is_err());
        let mut t=Timeline{submitted:2048,..Timeline::default()};t.emit(0,&[1]).unwrap();
        assert!(t.emit(0,&[2]).is_err());
    }
    #[test]
    #[ignore="requires native Windows AAC-LC encoder"]
    fn native_windows_aac_produces_raw_packets(){
        let mut e=Encoder::new().unwrap();let mut times=vec![];
        for tick in 0..20{
            let pcm=(0..4800).flat_map(|i|{let v=0.1*((tick*4800+i)as f32*440.*std::f32::consts::TAU/48000.).sin();[v,v]}).collect::<Vec<_>>();
            for p in e.push(&pcm).unwrap(){times.push(p.timestamp_ms);assert_eq!(p.payload[0..2],[0xaf,1]);}
        }
        assert!(times.len()>80);assert_eq!(times[0],0);
        assert!(times.windows(2).all(|w|(21..=22).contains(&(w[1]-w[0]))));
        assert!(e.pending_frames()<9216);
    }
}
