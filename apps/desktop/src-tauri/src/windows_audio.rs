use crate::{audio_mix,program_audio::{self,AudioInput}};
use std::{ffi::{CStr,c_void},os::raw::c_char};
unsafe extern "C"{
    fn moobee_wasapi_open(loopback:i32,endpoint:*const u16,error:*mut c_char,capacity:i32)->*mut c_void;
    fn moobee_wasapi_list_inputs(emit:unsafe extern "C" fn(*mut c_void,*const u16,usize,*const u16,usize,i32)->i32,context:*mut c_void,error:*mut c_char,capacity:i32)->i32;
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
    Ok(())
}
#[derive(serde::Serialize)]
#[serde(rename_all="camelCase")]
struct Endpoint { id:String, name:String, is_default:bool }

fn endpoint_id(value:Option<&str>)->Option<Vec<u16>>{
    value.map(|id|id.encode_utf16().chain(std::iter::once(0)).collect())
}

// The bridge calls synchronously with live COM-owned strings. Copy before returning.
unsafe extern "C" fn collect_endpoint(context:*mut c_void,id:*const u16,id_len:usize,name:*const u16,name_len:usize,is_default:i32)->i32{
    if context.is_null()||id.is_null()||name.is_null()||id_len==0||id_len>1024||name_len>4096{return -1}
    let endpoints=unsafe{&mut *context.cast::<Vec<Endpoint>>()};
    if endpoints.len()>=256{return -1}
    let Ok(id)=String::from_utf16(unsafe{std::slice::from_raw_parts(id,id_len)}) else{return -1};
    if id.len()>1024||id.chars().any(char::is_control)||endpoints.iter().any(|e|e.id==id){return -1}
    let name=String::from_utf16_lossy(unsafe{std::slice::from_raw_parts(name,name_len)});
    endpoints.push(Endpoint{id,name,is_default:is_default!=0});
    0
}

pub(crate) fn list()->Result<serde_json::Value,String>{
    let mut endpoints=Vec::<Endpoint>::new();let mut error=[0;512];
    let rc=unsafe{moobee_wasapi_list_inputs(collect_endpoint,(&mut endpoints as *mut Vec<Endpoint>).cast(),error.as_mut_ptr(),512)};
    if rc!=0{return Err(detail(&error))}
    serde_json::to_value(endpoints).map_err(|e|e.to_string())
}

struct Source(*mut c_void);
impl Source{
    fn open(input:&AudioInput)->Result<Self,String>{
        let mut error=[0;512];
        let id=endpoint_id(input.device_id.as_deref());
        let h=unsafe{moobee_wasapi_open(i32::from(input.kind=="desktop"),id.as_ref().map_or(std::ptr::null(),|v|v.as_ptr()),error.as_mut_ptr(),512)};
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
        let (mix,missing,_)=self.read_levels(frames,&self.inputs)?;
        Ok((mix,missing))
    }
    fn read_levels(&self,frames:usize,settings:&[AudioInput])->Result<(audio_mix::MixResult,u64,program_audio::AudioLevels),String>{
        if !(1..=4800).contains(&frames)||settings.len()!=self.sources.len(){return Err("Invalid Windows audio block".into())}
        let pcm=self.sources.iter().map(|s|s.read(frames)).collect::<Result<Vec<_>,_>>()?;
        let missing=pcm.iter().map(|(_,n)|u64::from(*n)).sum();
        let views=pcm.iter().zip(settings).map(|((samples,_),i)|audio_mix::Input{samples,gain:i.gain,muted:i.muted}).collect::<Vec<_>>();
        let mix=audio_mix::mix_stereo(frames,&views)?;
        let input_peaks=pcm.iter().zip(settings).map(|((samples,_),i)|if i.muted{0.}else{samples.iter().fold(0f32,|a,b|a.max(b.abs()*i.gain)).min(1.)}).collect();
        let levels=program_audio::AudioLevels{input_peaks,mixed_peak:mix.peak,clipped_samples:mix.clipped};
        Ok((mix,missing,levels))
    }
}
// The COM endpoints are created, polled and destroyed on the same dedicated MTA thread.
use std::{sync::{Arc,Mutex,OnceLock,atomic::{AtomicBool,Ordering}},thread::{self,JoinHandle},time::{Duration,Instant}};
const LEASE:Duration=Duration::from_secs(5);
const BLOCK:Duration=Duration::from_millis(20);
struct MeterState{stop:AtomicBool,last_use:Mutex<Instant>,settings:Mutex<Vec<AudioInput>>,result:Mutex<Option<Result<program_audio::AudioLevels,String>>>}
struct MeterWorker{state:Arc<MeterState>,thread:JoinHandle<()>}
static METER:OnceLock<Mutex<Option<MeterWorker>>>=OnceLock::new();
fn meter()->&'static Mutex<Option<MeterWorker>>{METER.get_or_init(||Mutex::new(None))}
fn same_sources(a:&[AudioInput],b:&[AudioInput])->bool{a.len()==b.len()&&a.iter().zip(b).all(|(a,b)|a.kind==b.kind&&a.device_id==b.device_id)}
fn stop_worker(slot:&mut Option<MeterWorker>){
    if let Some(worker)=slot.take(){worker.state.stop.store(true,Ordering::Release);let _=worker.thread.join();}
}
pub(crate) fn stop()->Result<(),String>{stop_worker(&mut *meter().lock().map_err(|_|"Windows audio meter unavailable")?);Ok(())}
pub(crate) fn levels(inputs:Vec<AudioInput>)->Result<Option<program_audio::AudioLevels>,String>{
    validate(&inputs)?;
    let mut slot=meter().lock().map_err(|_|"Windows audio meter unavailable")?;
    if inputs.is_empty(){stop_worker(&mut slot);return Ok(None)}
    let restart=slot.as_ref().map_or(true,|w|!same_sources(&w.state.settings.lock().unwrap(),&inputs)||w.state.last_use.lock().unwrap().elapsed()>=LEASE);
    if restart{
        stop_worker(&mut slot);
        let state=Arc::new(MeterState{stop:AtomicBool::new(false),last_use:Mutex::new(Instant::now()),settings:Mutex::new(inputs.clone()),result:Mutex::new(None)});
        let shared=state.clone();
        let thread=thread::Builder::new().name("moobee-wasapi-meter".into()).spawn(move||{
            let result=(||->Result<(),String>{
                let initial=shared.settings.lock().unwrap().clone();
                let mixer=Mixer::open(initial)?;
                while !shared.stop.load(Ordering::Acquire)&&shared.last_use.lock().unwrap().elapsed()<LEASE{
                    let tick=Instant::now();
                    let settings=shared.settings.lock().unwrap().clone();
                    let (_,_,levels)=mixer.read_levels(960,&settings)?;
                    *shared.result.lock().unwrap()=Some(Ok(levels));
                    thread::sleep(BLOCK.saturating_sub(tick.elapsed()));
                }
                Ok(())
            })();
            *shared.result.lock().unwrap()=result.err().map(Err);
        }).map_err(|e|e.to_string())?;
        *slot=Some(MeterWorker{state,thread});
    }
    let w=slot.as_ref().unwrap();
    *w.state.last_use.lock().unwrap()=Instant::now();
    *w.state.settings.lock().unwrap()=inputs;
    let result=w.state.result.lock().unwrap().clone();
    result.transpose()
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn named_and_default_endpoint_validation(){
        let mut mic=AudioInput{kind:"device".into(),device_id:None,gain:1.,muted:false};
        let desktop=AudioInput{kind:"desktop".into(),device_id:None,gain:0.5,muted:false};
        assert!(validate(&[mic.clone(),desktop]).is_ok());
        assert!(validate(&[mic.clone(),mic.clone()]).is_err());
        mic.device_id=Some("named".into());assert!(validate(&[mic.clone()]).is_ok());
        let mut card=mic.clone();card.device_id=Some("usb-hdmi".into());
        let desktop=AudioInput{kind:"desktop".into(),device_id:None,gain:0.5,muted:false};
        assert!(validate(&[mic.clone(),card,desktop.clone()]).is_ok());
        assert!(validate(&[mic.clone(),mic]).is_err());
        let mut invalid=desktop;invalid.device_id=Some("render".into());assert!(validate(&[invalid]).is_err());
    }
    #[test]fn gain_updates_keep_same_meter_sources(){
        let a=AudioInput{kind:"device".into(),device_id:Some("card".into()),gain:1.,muted:false};
        let mut b=a.clone();b.gain=0.25;b.muted=true;
        assert!(same_sources(&[a.clone()],&[b.clone()]));
        b.device_id=Some("mic".into());assert!(!same_sources(&[a],&[b]));
    }
    #[test]fn endpoint_ids_are_opaque_utf16(){
        assert_eq!(endpoint_id(None),None);
        assert_eq!(endpoint_id(Some("USB-🎧")),Some("USB-🎧".encode_utf16().chain([0]).collect()));
    }
    #[test]fn enumeration_copies_unicode_and_rejects_duplicates(){
        let mut out=Vec::<Endpoint>::new();let id:Vec<u16>="usb-1".encode_utf16().collect();let name:Vec<u16>="HDMI 音声".encode_utf16().collect();
        let context=(&mut out as *mut Vec<Endpoint>).cast();
        assert_eq!(unsafe{collect_endpoint(context,id.as_ptr(),id.len(),name.as_ptr(),name.len(),1)},0);
        assert_eq!(out[0].id,"usb-1");assert_eq!(out[0].name,"HDMI 音声");assert!(out[0].is_default);
        assert_eq!(unsafe{collect_endpoint(context,id.as_ptr(),id.len(),name.as_ptr(),name.len(),0)},-1);
    }

}
