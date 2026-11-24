//! Mac-only synchronous AVFoundation sink used by the native capture worker.
//! This writes composed display video only; no audio track is requested.
use std::{ffi::{CStr, CString, c_void}, fs::OpenOptions, os::raw::{c_char,c_int}, path::Path};

unsafe extern "C" {
    fn moobee_program_writer_create(path: *const c_char, width: c_int, height: c_int,
        fps: c_int, microphone: c_int, error: *mut c_char, capacity: c_int) -> *mut c_void;
    fn moobee_program_writer_append(handle: *mut c_void, rgba: *const u8, length: usize,
        error: *mut c_char, capacity: c_int) -> c_int;
    fn moobee_program_writer_append_audio(handle: *mut c_void, pcm: *const f32, frames: usize, error: *mut c_char, capacity: c_int) -> c_int;
    fn moobee_program_writer_finish(handle: *mut c_void, error: *mut c_char, capacity: c_int) -> c_int;
    fn moobee_program_writer_destroy(handle: *mut c_void);
}

pub(crate) struct ProgramMp4Sink(*mut c_void);
fn message(error: &[c_char; 512]) -> String {
    let detail = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
    if detail.is_empty() { "Native composed H.264 writer failed".into() } else { detail.into_owned() }
}
impl ProgramMp4Sink {
    pub(crate) fn new(path: &Path, width: u32, height: u32, fps: u32) -> Result<Self, String> {
        Self::new_with_audio(path,width,height,fps,false)
    }
    pub(crate) fn new_with_audio(path: &Path, width: u32, height: u32, fps: u32, microphone: bool) -> Result<Self,String> {
        let path = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| "Invalid MP4 path")?;
        let mut error = [0; 512];
        let handle = unsafe { moobee_program_writer_create(path.as_ptr(), width as c_int,
            height as c_int, fps as c_int, if microphone { 1 } else { 0 }, error.as_mut_ptr(), 512) };
        if handle.is_null() { Err(message(&error)) } else { Ok(Self(handle)) }
    }

    pub(crate) fn append(&mut self, rgba: &[u8]) -> Result<(), String> {
        let mut error = [0; 512];
        let result = unsafe { moobee_program_writer_append(self.0, rgba.as_ptr(), rgba.len(),
            error.as_mut_ptr(), 512) };
        if result == 0 { Ok(()) } else { Err(message(&error)) }
    }

    pub(crate) fn append_audio(&mut self, pcm: &[f32]) -> Result<(), String> {
        if pcm.is_empty() || pcm.len() % 2 != 0 || pcm.len() > 9600 { return Err("Invalid stereo PCM block".into()); }
        let mut error = [0;512];
        let result = unsafe { moobee_program_writer_append_audio(self.0, pcm.as_ptr(), pcm.len()/2, error.as_mut_ptr(),512) };
        if result == 0 { Ok(()) } else { Err(message(&error)) }
    }
    pub(crate) fn finish(&mut self) -> Result<(), (String, bool)> {
        let mut error = [0; 512];
        let result = unsafe { moobee_program_writer_finish(self.0, error.as_mut_ptr(), 512) };
        if result == 0 { Ok(()) } else { Err((message(&error), result == -2)) }
    }
}
impl Drop for ProgramMp4Sink {
    fn drop(&mut self) { unsafe { moobee_program_writer_destroy(self.0) } }
}

pub(crate) fn validate_finished(path: &Path) -> Result<(u64, f64), String> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)
        .map_err(|e| format!("Cannot open finalized MP4: {e}"))?;
    let bytes = file.metadata().map_err(|e| e.to_string())?.len();
    if !(4096..=64 * 1024 * 1024).contains(&bytes) { return Err("Composed MP4 is empty or exceeds 64 MiB".into()); }
    crate::mp4_validation::validate_mp4_boxes(&mut file, bytes)?;
    let duration = crate::mp4_validation::movie_duration_seconds(&mut file, bytes)?
        .filter(|v| v.is_finite() && *v > 0.0).ok_or("Missing valid MP4 duration")?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok((bytes, duration))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsupported_dimensions_before_opening_output() {
        let path = std::env::temp_dir().join(format!("moobee-invalid-{}.mp4", uuid::Uuid::new_v4()));
        assert!(ProgramMp4Sink::new(&path, 1920, 1080, 60).is_err());
        assert!(!path.exists());
    }

    // Requires a functioning Apple H.264 encoder. Run explicitly on a real Mac:
    // cargo test --lib program_mp4_sink::tests -- --ignored
    #[test]
    #[ignore = "requires an available macOS H.264 encoder; real-device acceptance"]
    fn synthetic_composed_mp4_finalizes_with_expected_duration() {
        let dir = std::env::temp_dir().join(format!("moobee-program-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("test.mp4");
        let result = (|| -> Result<(), String> {
            let mut sink = ProgramMp4Sink::new(&path, 640, 360, 10)?;
            for frame in 0u8..20 {
                let red = image::RgbaImage::from_fn(320, 180, |x, y| {
                    image::Rgba([((x + u32::from(frame) * 7) % 256) as u8, (y % 256) as u8, 30, 255])
                });
                let blue = image::RgbaImage::from_fn(320, 180, |x, y| {
                    image::Rgba([20, ((y + u32::from(frame) * 9) % 256) as u8, (x % 256) as u8, 255])
                });
                let canvas = crate::scene_compositor::composite_grid(&[red, blue], 640, 360)?;
                sink.append(canvas.as_raw())?;
            }
            sink.finish().map_err(|(message, _)| message)?;
            drop(sink);
            let (_, seconds) = validate_finished(&path)?;
            assert!((seconds - 2.0).abs() < 0.2);
            Ok(())
        })();
        // A failed/timed-out encoder may still own its temporary file; leave
        // this unique test directory for investigation on failure.
        if result.is_ok() { std::fs::remove_dir_all(&dir).unwrap(); }
        result.unwrap();
    }
}
