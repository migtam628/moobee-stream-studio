use crate::{audio_mix,program_audio::{self,AudioInput}};
use std::{ffi::{CStr,c_void},os::raw::c_char};
unsafe extern "C"{
    fn moobee_wasapi_open(loopback:i32,error:*mut c_char,capacity:i32)->*mut c_void;
    fn moobee_wasapi_read(handle:*mut c_void,out:*mut f32,frames:usize,missing:*mut u32,error:*mut c_char,capacity:i32)->i32;
    fn moobee_wasapi_discard(handle:*mut c_void,error:*mut c_char,capacity:i32)->i32;
    fn moobee_wasapi_close(handle:*mut c_void);
}
fn detail(error:&[c_char;512])->String{
    let s=unsafe{CStr::from_ptr(error.as_ptr())}.to_string_lossy();
    if s.is_empty(){"WASAPI capture failed".into()}else{s.into_owned()}
}
pub(crate) fn validate(inputs:&[AudioInput])->Result<(),String>{
    program_audio::validate(inputs)?;
    if inputs.iter().any(|i|i.device_id.is_some())||inputs.len()>2{return Err("Windows audio supports the default microphone and default playback endpoint only".into())}
    Ok(())
}
struct Source(*mut c_void);
impl Source{
    fn open(input:&AudioInput)->Result<Self,String>{
        let mut error=[0;512];
        let h=unsafe{moobee_wasapi_open(i32::from(input.kind=="desktop"),error.as_mut_ptr(),512)};
        if h.is_null(){Err(detail(&error))}else{Ok(Self(h))}
    }
    fn read(&self,frames:usize)->Result<(Vec<f32>,u32),String>{
        let mut samples=vec![0.;frames*2];let mut error=[0;512];let mut missing=0;
        if unsafe{moobee_wasapi_read(self.0,samples.as_mut_ptr(),frames,&mut missing,error.as_mut_ptr(),512)}!=0{return Err(detail(&error))}
        Ok((samples,missing))
    }
    fn discard(&self)->Result<(),String>{
        let mut error=[0;512];
        if unsafe{moobee_wasapi_discard(self.0,error.as_mut_ptr(),512)}!=0{return Err(detail(&error))}
        Ok(())
    }
}
impl Drop for Source{fn drop(&mut self){unsafe{moobee_wasapi_close(self.0)}}}
pub(crate) struct Mixer{sources:Vec<Source>,inputs:Vec<AudioInput>}
impl Mixer{
    pub(crate) fn open(inputs:Vec<AudioInput>)->Result<Self,String>{
        validate(&inputs)?;
        let sources=inputs.iter().map(Source::open).collect::<Result<Vec<_>,_>>()?;
        let mixer=Self{sources,inputs};mixer.discard()?;Ok(mixer)
    }
    pub(crate) fn discard(&self)->Result<(),String>{for source in &self.sources{source.discard()?;}Ok(())}
    pub(crate) fn read(&self,frames:usize)->Result<(audio_mix::MixResult,u64),String>{
        if !(1..=4800).contains(&frames){return Err("Invalid Windows audio block".into())}
        let pcm=self.sources.iter().map(|s|s.read(frames)).collect::<Result<Vec<_>,_>>()?;
        let missing=pcm.iter().map(|(_,n)|u64::from(*n)).sum();
        let views=pcm.iter().zip(&self.inputs).map(|((samples,_),i)|audio_mix::Input{samples,gain:i.gain,muted:i.muted}).collect::<Vec<_>>();
        Ok((audio_mix::mix_stereo(frames,&views)?,missing))
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn default_endpoints_only(){
        let mut mic=AudioInput{kind:"device".into(),device_id:None,gain:1.,muted:false};
        let desktop=AudioInput{kind:"desktop".into(),device_id:None,gain:0.5,muted:false};
        assert!(validate(&[mic.clone(),desktop]).is_ok());
        assert!(validate(&[mic.clone(),mic.clone()]).is_err());
        mic.device_id=Some("named".into());assert!(validate(&[mic]).is_err());
    }
}
