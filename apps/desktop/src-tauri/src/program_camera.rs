//! macOS-only opt-in camera capture for the composed MP4 test.
use std::{ffi::{c_void, CStr, CString}, os::raw::{c_char, c_int}, time::{Duration, Instant}};
const CAPACITY: usize = 1280 * 720 * 4;

unsafe extern "C" {
    fn moobee_list_video_inputs(buffer: *mut c_char, capacity: c_int) -> c_int;
    fn moobee_program_camera_create(device_id: *const c_char, error: *mut c_char, capacity: c_int) -> *mut c_void;
    fn moobee_program_camera_copy(handle: *mut c_void, destination: *mut u8, capacity: usize,
        after: u64, width: *mut i32, height: *mut i32, sequence: *mut u64) -> c_int;
    fn moobee_program_camera_destroy(handle: *mut c_void);
}

pub(crate) struct ProgramCamera {
    handle: *mut c_void,
    sequence: u64,
    scratch: Vec<u8>,
}
impl ProgramCamera {
    pub(crate) fn start() -> Result<Self, String> {
        Self::start_selected(None)
    }
    pub(crate) fn start_selected(id: Option<&str>) -> Result<Self, String> {
        let device = id.map(CString::new).transpose().map_err(|_| "Invalid video device ID")?;
        let mut error = [0 as c_char; 512];
        let handle = unsafe { moobee_program_camera_create(device.as_ref().map_or(std::ptr::null(), |id| id.as_ptr()), error.as_mut_ptr(), 512) };
        if handle.is_null() {
            let detail = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
            return Err(if detail.is_empty() { "Native video input start failed".into() } else { detail.into_owned() });
        }
        Ok(Self { handle, sequence: 0, scratch: vec![0; CAPACITY] })
    }
    pub(crate) fn poll(&mut self) -> Result<Option<image::RgbaImage>, String> {
        let (mut w, mut h, mut seq) = (0i32, 0i32, 0u64);
        let result = unsafe { moobee_program_camera_copy(self.handle, self.scratch.as_mut_ptr(),
            self.scratch.len(), self.sequence, &mut w, &mut h, &mut seq) };
        if result == 0 { return Ok(None); }
        if result != 1 || !(1..=1280).contains(&w) || !(1..=720).contains(&h) {
            return Err("Video input returned an unsupported frame; use a compatible 720p or lower input".into());
        }
        let bytes = w as usize * h as usize * 4;
        let frame = image::RgbaImage::from_raw(w as u32, h as u32, self.scratch[..bytes].to_vec())
            .ok_or("Invalid camera RGBA buffer")?;
        self.sequence = seq;
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
    fn drop(&mut self) { unsafe { moobee_program_camera_destroy(self.handle) } }
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

pub(crate) fn list_inputs() -> Result<Vec<VideoInput>, String> {
    let mut buffer = vec![0 as c_char; 262_144];
    if unsafe { moobee_list_video_inputs(buffer.as_mut_ptr(), buffer.len() as c_int) } != 0 {
        return Err("Unable to enumerate native video inputs".into());
    }
    let json = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_str().map_err(|_| "Invalid device inventory")?;
    let devices: Vec<VideoInput> = serde_json::from_str(json).map_err(|_| "Invalid native device inventory")?;
    Ok(devices.into_iter().filter(|d| valid_device_id(&d.id)).collect())
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
