use std::{ffi::{c_void,CStr}, os::raw::{c_char,c_int}};
unsafe extern "C" {
    fn moobee_program_mic_authorize(error: *mut c_char, capacity: c_int) -> c_int;
    fn moobee_program_mic_create(error: *mut c_char, capacity: c_int) -> *mut c_void;
    fn moobee_program_mic_read(handle: *mut c_void, destination: *mut f32,
        frames: usize, error: *mut c_char, capacity: c_int) -> c_int;
    fn moobee_program_mic_destroy(handle: *mut c_void);
}
fn message(error: &[c_char;512]) -> String {
    let value = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
    if value.is_empty() { "Native microphone failed".into() } else { value.into_owned() }
}
pub(crate) struct ProgramMicrophone(*mut c_void);
impl ProgramMicrophone {
    pub(crate) fn authorize() -> Result<(),String> {
        let mut error = [0;512];
        if unsafe { moobee_program_mic_authorize(error.as_mut_ptr(),512) } == 0 { Ok(()) } else { Err(message(&error)) }
    }
    pub(crate) fn start() -> Result<Self,String> {
        let mut error = [0;512];
        let handle = unsafe { moobee_program_mic_create(error.as_mut_ptr(),512) };
        if handle.is_null() { Err(message(&error)) } else { Ok(Self(handle)) }
    }
    pub(crate) fn read(&mut self, frames: usize, gain: f32) -> Result<crate::audio_mix::MixResult,String> {
        if frames == 0 || frames > 4800 { return Err("Invalid microphone block".into()); }
        let mut pcm = vec![0f32;frames*2];
        let mut error = [0;512];
        if unsafe { moobee_program_mic_read(self.0,pcm.as_mut_ptr(),frames,error.as_mut_ptr(),512) } != 0 {
            return Err(message(&error));
        }
        crate::audio_mix::mix_stereo(frames,&[crate::audio_mix::Input { samples: &pcm, gain, muted: false }])
    }
}
impl Drop for ProgramMicrophone {
    fn drop(&mut self) { unsafe { moobee_program_mic_destroy(self.0) } }
}
