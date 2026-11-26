//! Windows Media Foundation async camera source; compatible with the shared preview and recorder.
use std::{ffi::{c_void, CStr}, os::raw::{c_char, c_int}, time::{Duration, Instant}};
const CAPACITY: usize = 1280 * 720 * 4;

unsafe extern "C" {
    fn moobee_win_video_list(emit:unsafe extern "C" fn(*mut c_void,*const u16,usize,*const u16,usize,i32)->i32,context:*mut c_void,error:*mut c_char,capacity:c_int)->c_int;
    fn moobee_win_video_open(id:*const u16,error:*mut c_char,capacity:c_int)->*mut c_void;
    fn moobee_win_video_poll(handle:*mut c_void,destination:*mut u8,capacity:usize,width:*mut i32,height:*mut i32,error:*mut c_char,error_capacity:c_int)->c_int;
    fn moobee_win_video_close(handle:*mut c_void);
}
fn detail(error:&[c_char;512])->String {
    let detail=unsafe{CStr::from_ptr(error.as_ptr())}.to_string_lossy();
    if detail.is_empty(){"Native Windows video capture failed".into()}else{detail.into_owned()}
}

pub(crate) struct ProgramCamera {
    handle: *mut c_void,
    scratch: Vec<u8>,
}
impl ProgramCamera {
    pub(crate) fn start_selected(id: Option<&str>) -> Result<Self, String> {
        let id=id.ok_or("Select a named Windows video capture input")?;
        if !valid_device_id(id){return Err("Invalid video input ID".into())}
        let device:Vec<u16>=id.encode_utf16().chain(std::iter::once(0)).collect();
        let mut error=[0 as c_char;512];
        let handle=unsafe{moobee_win_video_open(device.as_ptr(),error.as_mut_ptr(),512)};
        if handle.is_null(){return Err(detail(&error))}
        Ok(Self{handle,scratch:vec![0;CAPACITY]})
    }
    pub(crate) fn poll(&mut self)->Result<Option<image::RgbaImage>,String>{
        let(mut w,mut h)=(0i32,0i32);let mut error=[0 as c_char;512];
        let rc=unsafe{moobee_win_video_poll(self.handle,self.scratch.as_mut_ptr(),self.scratch.len(),&mut w,&mut h,error.as_mut_ptr(),512)};
        if rc==0{return Ok(None)}
        if rc!=1{return Err(detail(&error))}
        if !(1..=1280).contains(&w)||!(1..=720).contains(&h){return Err("Invalid Windows video frame geometry".into())}
        let bytes=w as usize*h as usize*4;
        let frame=image::RgbaImage::from_raw(w as u32,h as u32,self.scratch[..bytes].to_vec()).ok_or("Invalid camera RGBA buffer")?;
        Ok(Some(frame))
    }

    pub(crate) fn first_frame(&mut self) -> Result<image::RgbaImage, String> {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Some(frame) = self.poll()? { return Ok(frame); }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err("No video frames arrived; check HDMI signal, USB connection, device availability and Camera permission".into())
    }
}
impl Drop for ProgramCamera {
    fn drop(&mut self) { unsafe { moobee_win_video_close(self.handle) } }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoInput {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) is_default: bool,
}

pub(crate) fn valid_device_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 1024 && !id.chars().any(char::is_control)
}

unsafe extern "C" fn collect_device(context:*mut c_void,id:*const u16,id_len:usize,name:*const u16,name_len:usize,is_default:i32)->i32{
    if context.is_null()||id.is_null()||name.is_null()||id_len==0||id_len>1024||name_len>4096{return -1}
    let devices=unsafe{&mut *context.cast::<Vec<VideoInput>>()};
    let Ok(id)=String::from_utf16(unsafe{std::slice::from_raw_parts(id,id_len)}) else{return -1};
    if devices.len()>=256||!valid_device_id(&id)||devices.iter().any(|d|d.id==id){return -1}
    let name=String::from_utf16_lossy(unsafe{std::slice::from_raw_parts(name,name_len)});
    devices.push(VideoInput{id,name,is_default:is_default!=0});
    0
}
pub(crate) fn list_inputs()->Result<Vec<VideoInput>,String>{
    let mut devices=Vec::<VideoInput>::new();let mut error=[0 as c_char;512];
    if unsafe{moobee_win_video_list(collect_device,(&mut devices as *mut Vec<VideoInput>).cast(),error.as_mut_ptr(),512)}!=0{return Err(detail(&error))}
    Ok(devices)
}

pub(crate) async fn list_video_inputs() -> Result<Vec<VideoInput>, String> {
    tauri::async_runtime::spawn_blocking(list_inputs).await.map_err(|e| e.to_string())?
}

pub(crate) async fn test_video_input(device_id: String) -> Result<String, String> {
    if !valid_device_id(&device_id) { return Err("Invalid video input ID".into()); }
    tauri::async_runtime::spawn_blocking(move || {
        use crate::clip::{CaptureLock, CAPTURE_BUSY};
        use std::sync::atomic::Ordering;
        use base64::Engine as _;
        CAPTURE_BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Another native recording or diagnostic is running".to_owned())?;
        let _guard = CaptureLock;
        let mut source = ProgramCamera::start_selected(Some(&device_id))?;
        let frame = source.first_frame()?;
        drop(source);
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(frame).write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())))
    }).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_device_ids_are_not_display_indices_or_paths() {
        assert!(valid_device_id("0x1234567890"));
        assert!(!valid_device_id(""));
        assert!(!valid_device_id("bad\0id"));
        assert!(!valid_device_id(&"x".repeat(1025)));
    }
}
