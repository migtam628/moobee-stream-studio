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
// Keep all untrusted label text on a single line and escape bidi/control
// characters. The endpoint itself must pass native URL validation.
fn display_label(value:&str)->String{
    value.chars().map(|c|if c.is_ascii_graphic()||c==' '{c.to_string()}else{c.escape_default().to_string()}).collect()
}
fn consent_summary(request:&Request)->Result<String,String>{
    if request.display_ids.len()>4||request.video_device_id.as_ref().is_some_and(|v|v.is_empty()||v.len()>1024||v.chars().any(char::is_control)){
        return Err("Invalid capture source selection".into())
    }
    crate::program_audio::validate(&request.audio_inputs)?;
    if !["360p10","720p15","1080p30","720p30","1080p15"].contains(&request.profile.as_str()) {
        // The Program engine validates the exact supported profile before
        // opening sources. Bound and sanitize arbitrary labels for this dialog.
        if request.profile.is_empty()||request.profile.len()>40||!request.profile.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-'){return Err("Invalid profile".into())}
    }
    let mut detail=format!("This sends actual video and selected audio over the network. Review the source preview in Studio before approving. A destination may be public.\n\nDuration: up to {} seconds\nProfile: {}\n",request.seconds,display_label(&request.profile));
    detail.push_str(&format!("Display sources: {}\nVideo device selected: {}\nAudio inputs: {} ({} unmuted)\nOverlay: {}\n",request.display_ids.len(),if request.video_device_id.is_some(){"yes"}else{"no"},request.audio_inputs.len(),request.audio_inputs.iter().filter(|v|!v.muted&&v.gain>0.).count(),if request.overlay.is_some(){"yes"}else{"no"}));
    detail.push_str(&format!("Reconnect bitrate fallback: {}\nIn-place adaptive bitrate: {}\n\nConfirmed destinations:\n",request.fallback_on_reconnect.unwrap_or(false),request.adaptive_bitrate.unwrap_or(false)));
    for (i,d) in request.destinations.iter().enumerate(){
        if d.name.is_empty()||d.name.len()>80||d.name.chars().any(char::is_control)||d.server_url.len()>768||!d.server_url.is_ascii()||d.server_url.chars().any(|c|c.is_ascii_whitespace()||c.is_control()){
            return Err("Invalid destination preview".into())
        }
        #[cfg(any(target_os="macos",target_os="windows"))]
        crate::rtmp_session::tls::validate_destination(&d.server_url,"consent-validation-only")?;
        detail.push_str(&format!("{}. {}\n   {}\n",i+1,display_label(&d.name),d.server_url));
    }
    Ok(detail)
}
#[cfg(all(feature="native-live-acceptance",any(target_os="macos",target_os="windows")))]
mod native_consent{
    use super::*;
    use std::{sync::atomic::{AtomicBool,Ordering},time::{Duration,Instant}};
    static PROMPT_ACTIVE:AtomicBool=AtomicBool::new(false);
    const CONSENT_WINDOW:Duration=Duration::from_secs(90);
    struct PromptSlot;
    impl PromptSlot{
        fn acquire()->Result<Self,String>{
            PROMPT_ACTIVE.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).map(|_|Self).map_err(|_|"Another native confirmation is open".into())
        }
    }
    impl Drop for PromptSlot{fn drop(&mut self){PROMPT_ACTIVE.store(false,Ordering::Release);}}
    #[cfg(target_os="macos")]
    fn dialog(detail:&str)->bool{
        use std::ffi::{CString,c_char};
        unsafe extern "C"{fn moobee_confirm_test_broadcast(message:*const c_char)->i32;}
        CString::new(detail).is_ok_and(|text|unsafe{moobee_confirm_test_broadcast(text.as_ptr())==1})
    }
    #[cfg(target_os="windows")]
    fn dialog(detail:&str)->bool{
        #[link(name="user32")]
        unsafe extern "system"{fn MessageBoxW(hwnd:*mut std::ffi::c_void,text:*const u16,caption:*const u16,kind:u32)->i32;}
        let text:Vec<u16>=detail.encode_utf16().chain(Some(0)).collect();
        let title:Vec<u16>="MooBee Studio — Authorize real test broadcast?".encode_utf16().chain(Some(0)).collect();
        // MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2 | MB_TASKMODAL | MB_SETFOREGROUND.
        unsafe{MessageBoxW(std::ptr::null_mut(),text.as_ptr(),title.as_ptr(),0x0001_2134)==6}
    }
    pub(super) async fn confirm(app:&tauri::AppHandle,run:crate::live_lifecycle::Run,detail:String)->Result<(),String>{
        let slot=PromptSlot::acquire()?;
        let deadline=Instant::now()+CONSENT_WINDOW;
        let (tx,rx)=tokio::sync::oneshot::channel();
        app.run_on_main_thread(move||{
            let _slot=slot;
            let approved=run.native_active()&&Instant::now()<deadline&&dialog(&detail);
            // A late affirmative click must not authorize an expired/stopped run.
            let _=tx.send(approved&&run.native_active()&&Instant::now()<deadline);
        }).map_err(|_|"Could not display native confirmation")?;
        match tokio::time::timeout(CONSENT_WINDOW,rx).await{
            Ok(Ok(true))=>Ok(()),
            _=>Err("Native broadcast confirmation declined or expired".into()),
        }
    }
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
        let detail=consent_summary(&request)?;
        let run=state.inner().start(request.session_id.clone(),Mode::Native)?;
        if let Err(error)=native_consent::confirm(&app,run.clone(),detail).await{
            run.finish(false);return Err(error)
        }
        if !run.native_active(){run.finish(false);return Err("Test broadcast cancelled before capture".into())}
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
    #[test]fn native_consent_summary_escapes_labels_and_includes_endpoint_without_key(){
        let request:Request=serde_json::from_value(serde_json::json!({
            "sessionId":uuid::Uuid::new_v4().to_string(),"confirmation":"START REAL TEST STREAM",
            "destinations":[{"id":uuid::Uuid::new_v4().to_string(),"name":"Fixture \u{202e} label","serverUrl":"rtmps://example.invalid/live"}],
            "displayIds":[1],"seconds":15,"profile":"1080p30","audioInputs":[]
        })).unwrap();
        let summary=consent_summary(&request).unwrap();
        assert!(summary.contains("rtmps://example.invalid/live"));
        assert!(summary.contains("15 seconds"));
        assert!(!summary.contains('\u{202e}'));
        assert!(!summary.contains("consent-validation-only"));
        assert!(!summary.contains(&request.session_id));
    }
    #[cfg(not(feature="native-live-acceptance"))]
    #[test]fn default_build_is_disabled(){assert!(!supported());}
}
