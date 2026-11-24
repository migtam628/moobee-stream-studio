//! macOS-only opt-in camera capture for the composed MP4 test.
use std::{ffi::{c_void, CStr}, os::raw::{c_char, c_int}, time::{Duration, Instant}};
const CAPACITY: usize = 1280 * 720 * 4;

unsafe extern "C" {
    fn moobee_program_camera_create(error: *mut c_char, capacity: c_int) -> *mut c_void;
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
        let mut error = [0 as c_char; 512];
        let handle = unsafe { moobee_program_camera_create(error.as_mut_ptr(), 512) };
        if handle.is_null() {
            let detail = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
            return Err(if detail.is_empty() { "Native webcam start failed".into() } else { detail.into_owned() });
        }
        Ok(Self { handle, sequence: 0, scratch: vec![0; CAPACITY] })
    }
    pub(crate) fn poll(&mut self) -> Result<Option<image::RgbaImage>, String> {
        let (mut w, mut h, mut seq) = (0i32, 0i32, 0u64);
        let result = unsafe { moobee_program_camera_copy(self.handle, self.scratch.as_mut_ptr(),
            self.scratch.len(), self.sequence, &mut w, &mut h, &mut seq) };
        if result == 0 { return Ok(None); }
        if result != 1 || !(1..=1280).contains(&w) || !(1..=720).contains(&h) {
            return Err("Webcam returned an unsupported frame; choose a compatible default camera".into());
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
        Err("No webcam frame arrived; check the default camera and permission".into())
    }
}
impl Drop for ProgramCamera {
    fn drop(&mut self) { unsafe { moobee_program_camera_destroy(self.handle) } }
}
