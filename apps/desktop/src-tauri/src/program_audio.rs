//! Native named-input mix, used by the Program writer and opt-in level monitor.
use serde::{Deserialize,Serialize};
#[derive(Clone,Debug,Deserialize,PartialEq)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub(crate) struct AudioInput {
    pub kind: String,
    pub device_id: Option<String>,
    pub gain: f32,
    pub muted: bool,
}
pub(crate) fn validate(inputs: &[AudioInput]) -> Result<(),String> {
    if inputs.len()>3 { return Err("At most three audio sources".into()); }
    if inputs.iter().filter(|i|i.kind=="device").count()>1 && inputs.iter().any(|i|i.kind=="device" && i.device_id.is_none()) { return Err("Choose distinct named devices for multiple audio inputs".into()); }
    for (i,input) in inputs.iter().enumerate() {
        let valid = match input.kind.as_str() {
            "device" => input.device_id.as_ref().map_or(true, |id| !id.is_empty() && id.len()<=1024 && !id.chars().any(char::is_control)),
            "desktop" => input.device_id.is_none(),
            _ => false,
        };
        if !valid || !input.gain.is_finite() || !(0.0..=2.0).contains(&input.gain) ||
            inputs[..i].iter().any(|a| a.kind==input.kind && a.device_id==input.device_id) {
            return Err("Invalid or duplicate audio source".into());
        }
    }
    Ok(())
}
#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct AudioLevels { pub input_peaks: Vec<f32>, pub mixed_peak: f32, pub clipped_samples: usize }

#[cfg(target_os="macos")]
mod mac {
    use super::*;
    use std::{ffi::{CStr,CString,c_void},os::raw::{c_char,c_int},sync::{Mutex,Arc,OnceLock,atomic::{AtomicBool,Ordering}},thread::{self,JoinHandle},time::{Instant,Duration}};
    unsafe extern "C" {
        fn moobee_list_audio_inputs(buffer:*mut c_char, capacity:c_int)->c_int;
        fn moobee_audio_source_create(kind:*const c_char,device:*const c_char,error:*mut c_char,capacity:c_int)->*mut c_void;
        fn moobee_audio_source_read(handle:*mut c_void,destination:*mut f32,frames:usize,error:*mut c_char,capacity:c_int)->c_int;
        fn moobee_audio_source_reset(handle:*mut c_void);
        fn moobee_audio_source_destroy(handle:*mut c_void);
    }
    fn detail(error:&[c_char;512])->String {
        let value=unsafe{CStr::from_ptr(error.as_ptr())}.to_string_lossy();
        if value.is_empty() {"Native audio source failed".into()}else{value.into_owned()}
    }
    struct Source(*mut c_void);
    impl Source {
        fn open(input:&AudioInput)->Result<Self,String> {
            let kind=CString::new(input.kind.as_str()).map_err(|_|"Invalid kind")?;
            let id=input.device_id.as_deref().map(CString::new).transpose().map_err(|_|"Invalid device")?;
            let mut error=[0;512];
            let handle=unsafe{moobee_audio_source_create(kind.as_ptr(),id.as_ref().map_or(std::ptr::null(),|v|v.as_ptr()),error.as_mut_ptr(),512)};
            if handle.is_null(){Err(detail(&error))}else{Ok(Self(handle))}
        }
        fn read(&self,frames:usize)->Result<Vec<f32>,String>{
            let mut pcm=vec![0.;frames*2];let mut error=[0;512];
            if unsafe{moobee_audio_source_read(self.0,pcm.as_mut_ptr(),frames,error.as_mut_ptr(),512)}!=0 {return Err(detail(&error));}
            Ok(pcm)
        }
    }
    impl Drop for Source{fn drop(&mut self){unsafe{moobee_audio_source_destroy(self.0)}}}
    pub(crate) fn list()->Result<serde_json::Value,String>{
        let mut buffer=vec![0 as c_char;262144];
        if unsafe{moobee_list_audio_inputs(buffer.as_mut_ptr(),buffer.len() as c_int)}!=0 {return Err("Cannot enumerate audio inputs".into());}
        serde_json::from_slice(unsafe{CStr::from_ptr(buffer.as_ptr())}.to_bytes()).map_err(|e|e.to_string())
    }
    pub(crate) struct Mixer { sources:Vec<Source> }
    impl Mixer{
        pub(crate) fn open(inputs:&[AudioInput])->Result<Self,String>{
            validate(inputs)?;
            let sources=inputs.iter().map(Source::open).collect::<Result<Vec<_>,_>>()?;
            for s in &sources{unsafe{moobee_audio_source_reset(s.0)}}
            Ok(Self{sources})
        }
        pub(crate) fn read(&self,frames:usize,inputs:&[AudioInput])->Result<(crate::audio_mix::MixResult,AudioLevels),String>{
            if !(1..=4800).contains(&frames)||inputs.len()!=self.sources.len(){return Err("Invalid audio block".into());}
            let pcm=self.sources.iter().map(|s|s.read(frames)).collect::<Result<Vec<_>,_>>()?;
            let input_peaks=pcm.iter().zip(inputs).map(|(p,i)|if i.muted{0.}else{p.iter().fold(0f32,|a,b|a.max(b.abs()*i.gain)).min(1.)}).collect();
            let views=pcm.iter().zip(inputs).map(|(p,i)|crate::audio_mix::Input{samples:p,gain:i.gain,muted:i.muted}).collect::<Vec<_>>();
            let mix=crate::audio_mix::mix_stereo(frames,&views)?;
            let levels=AudioLevels{input_peaks,mixed_peak:mix.peak,clipped_samples:mix.clipped};
            Ok((mix,levels))
        }
    }
    struct Shared{stop:AtomicBool,last_use:Mutex<Instant>,inputs:Mutex<Vec<AudioInput>>,result:Mutex<Option<Result<AudioLevels,String>>>}
    struct Worker{shared:Arc<Shared>,handle:JoinHandle<()>}
    static METER:OnceLock<Mutex<Option<Worker>>>=OnceLock::new();
    fn meter()->&'static Mutex<Option<Worker>>{METER.get_or_init(||Mutex::new(None))}
    fn stop_worker(slot:&mut Option<Worker>){if let Some(w)=slot.take(){w.shared.stop.store(true,Ordering::Release);let _=w.handle.join();}}
    pub(crate) fn stop()->Result<(),String>{
        stop_worker(&mut *meter().lock().map_err(|_|"Audio meter unavailable")?);Ok(())
    }
    pub(crate) fn levels(inputs:Vec<AudioInput>)->Result<Option<AudioLevels>,String>{
        validate(&inputs)?;
        let mut slot=meter().lock().map_err(|_|"Audio meter unavailable")?;
        if inputs.is_empty(){stop_worker(&mut slot);return Ok(None)}
        let replace=slot.as_ref().map_or(true,|w|{
            let old=w.shared.inputs.lock().unwrap();
            old.len()!=inputs.len()||old.iter().zip(&inputs).any(|(a,b)|a.kind!=b.kind||a.device_id!=b.device_id)
                || (w.handle.is_finished() && w.shared.last_use.lock().unwrap().elapsed()>Duration::from_secs(5))
        });
        if replace{
            stop_worker(&mut slot);
            let shared=Arc::new(Shared{stop:AtomicBool::new(false),last_use:Mutex::new(Instant::now()),inputs:Mutex::new(inputs.clone()),result:Mutex::new(None)});
            let state=shared.clone();
            let handle=thread::Builder::new().name("moobee-audio-meter".into()).spawn(move||{
                let result=(||->Result<(),String>{
                    let initial=state.inputs.lock().unwrap().clone();
                    let mixer=Mixer::open(&initial)?;
                    while !state.stop.load(Ordering::Acquire)&&state.last_use.lock().unwrap().elapsed()<Duration::from_secs(5){
                        let settings=state.inputs.lock().unwrap().clone();
                        let (_,levels)=mixer.read(960,&settings)?;
                        *state.result.lock().unwrap()=Some(Ok(levels));
                    }
                    Ok(())
                })();
                *state.result.lock().unwrap()=result.err().map(Err);
            }).map_err(|e|e.to_string())?;
            *slot=Some(Worker{shared,handle});
        }
        let w=slot.as_ref().unwrap();
        *w.shared.last_use.lock().unwrap()=Instant::now();
        *w.shared.inputs.lock().unwrap()=inputs;
        let result=w.shared.result.lock().unwrap().clone();
        result.transpose()
    }
}
#[cfg(target_os="macos")]
pub(crate) use mac::{Mixer,list,levels,stop};
#[cfg(test)]
mod tests{
    use super::*;
    fn device(id:&str)->AudioInput{AudioInput{kind:"device".into(),device_id:Some(id.into()),gain:1.,muted:false}}
    #[test]fn validates_named_sources_and_limits(){
        assert!(validate(&[device("usb"),device("mic")]).is_ok());
        assert!(validate(&[device("usb"),device("usb")]).is_err());
        let mut default=device("mic");default.device_id=None;assert!(validate(&[default,device("usb")]).is_err());
        assert!(validate(&[device("bad\0id")]).is_err());
        let mut i=device("mic");i.gain=f32::NAN;assert!(validate(&[i]).is_err());
    }
}
