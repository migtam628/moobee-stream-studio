//! Real publishing is available only in explicitly enabled developer builds.
//! No credentials cross this start/status boundary; no automatic broadcast.
use serde::Deserialize;
use crate::live_lifecycle::{Controller,Mode};
use std::sync::Arc;
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Destination{id:String,name:String,server_url:String,revision:Option<String>}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub(crate) struct Request{
    session_id:String,confirmation:String,destinations:Vec<Destination>,
    display_ids:Vec<usize>,video_device_id:Option<String>,seconds:u32,profile:String,
    bitrate_kbps:Option<u32>,fallback_on_reconnect:Option<bool>,adaptive_bitrate:Option<bool>,layout:Option<String>,overlay:Option<crate::program_overlay::OverlayInput>,
    audio_inputs:Vec<crate::program_audio::AudioInput>,transforms:Option<Vec<crate::source_transform::SourceTransform>>,
}
fn supported()->bool{cfg!(all(feature="native-live-acceptance",any(target_os="macos",target_os="windows")))}
pub(crate) const MAX_LIVE_SECONDS:u32=7200;
fn validate_intent(id:&str,confirmation:&str,count:usize,seconds:u32)->Result<(),String>{
    if confirmation!="START REAL TEST STREAM"{return Err("Explicit test stream confirmation required".into())}
    if !uuid::Uuid::parse_str(id).is_ok_and(|u|u.to_string()==id)||!(1..=3).contains(&count)||!(2..=MAX_LIVE_SECONDS).contains(&seconds){
        return Err("Choose a valid session, one to three destinations and a 2–7200 second test".into())
    }
    Ok(())
}
#[tauri::command]
pub(crate) fn live_acceptance_available()->bool{supported()}
#[tauri::command]
pub(crate) async fn start_live_acceptance(app:tauri::AppHandle,state:tauri::State<'_,Arc<Controller>>,request:Request)->Result<(),String>{
    if !supported(){return Err("Real streaming acceptance is disabled in this build".into())}
    validate_intent(&request.session_id,&request.confirmation,request.destinations.len(),request.seconds)?;
    if (request.fallback_on_reconnect==Some(true)||request.adaptive_bitrate==Some(true))&&request.destinations.len()!=1{return Err("Bitrate fallback requires one destination".into())}
    #[cfg(all(feature="native-live-acceptance",any(target_os="macos",target_os="windows")))]
    {
        use std::collections::HashSet;
        let mut ids=HashSet::new();
        for d in &request.destinations{
            if !uuid::Uuid::parse_str(&d.id).is_ok_and(|u|u.to_string()==d.id)||!ids.insert(&d.id)||
                d.name.is_empty()||d.name.len()>80||d.server_url.len()>768||d.revision.as_ref().is_some_and(|id|!uuid::Uuid::parse_str(id).is_ok_and(|u|u.to_string()==*id)){return Err("Invalid confirmed destination preview".into())}
        }
        let run=state.inner().start(request.session_id.clone(),Mode::Native)?;
        run.enable_inplace_bitrate(request.adaptive_bitrate.unwrap_or(false));
        let guard=run.clone();
        let result=tauri::async_runtime::spawn_blocking(move||{
            let _sampler=crate::live_soak::Sampler::start(run.clone());
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
                let Request{destinations,display_ids,video_device_id,seconds,profile,bitrate_kbps,fallback_on_reconnect,layout,overlay,audio_inputs,transforms,..}=request;
                if destinations.len()==1{
                    let d=&destinations[0];
                    crate::program_continuous_avi::run_saved_destination_session(&app,run.clone(),&d.id,&d.name,&d.server_url,d.revision.as_deref(),true,
                        display_ids,video_device_id,seconds,&profile,bitrate_kbps,layout.as_deref(),overlay,audio_inputs,transforms,fallback_on_reconnect.unwrap_or(false))
                }else{
                    crate::program_continuous_avi::run_saved_multistream_session(&app,run.clone(),
                        destinations.into_iter().map(|d|(d.id,d.name,d.server_url,d.revision)).collect(),true,
                        display_ids,video_device_id,seconds,&profile,bitrate_kbps,layout.as_deref(),overlay,audio_inputs,transforms)
                }
            }));
            let ok=result.is_ok_and(|r|r.is_ok());
            run.finish(ok);
            if ok{Ok(())}else{Err("Native test stream ended unsuccessfully; review sender health".to_owned())}
        }).await;
        match result{Ok(r)=>r,Err(_)=>{guard.finish(false);Err("Native acceptance worker stopped unexpectedly".into())}}
    }
    #[cfg(not(all(feature="native-live-acceptance",any(target_os="macos",target_os="windows"))))]
    {let _=(app,state,request);Err("Real streaming acceptance is disabled in this build".into())}
}
#[tauri::command]
pub(crate) fn stop_live_acceptance(state:tauri::State<'_,Arc<Controller>>,session_id:String)->bool{
    supported()&&state.stop_native(&session_id)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn requires_explicit_phrase_and_bounded_test(){
        let id=uuid::Uuid::new_v4().to_string();
        assert!(validate_intent(&id,"START REAL TEST STREAM",1,10).is_ok());
        assert!(validate_intent(&id,"START REAL TEST STREAM",1,7200).is_ok());
        for (phrase,n,d) in [("start",1,10),("START REAL TEST STREAM",0,10),("START REAL TEST STREAM",4,10),("START REAL TEST STREAM",1,7201)]{
            assert!(validate_intent(&id,phrase,n,d).is_err());
        }
    }
    #[cfg(not(feature="native-live-acceptance"))]
    #[test]fn default_build_is_disabled(){assert!(!supported());}
}
