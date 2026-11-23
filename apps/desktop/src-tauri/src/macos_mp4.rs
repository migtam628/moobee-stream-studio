//! Experimental native macOS ScreenCaptureKit H.264 MP4 recording.
//! Bundled Swift code uses Apple's system codec and recording output directly.
//! Independent of OBS/Streamlabs and external FFmpeg executables.
//! Does not yet capture the saved MooBee program scene or webcam PiP.
use serde::Serialize;
use tauri::Manager;

#[cfg(target_os = "macos")]
use std::{
    ffi::{CStr, CString},
    fs::{self, OpenOptions},
    os::raw::{c_char, c_int},
    sync::atomic::Ordering,
};

const MIN_SECONDS: u32 = 3;
const MAX_SECONDS: u32 = 120;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mp4CaptureReport {
    id: String,
    path: String,
    format: &'static str,
    bytes: u64,
    duration_seconds: u32,
    has_requested_system_audio: bool,
    has_requested_microphone: bool,
    // Do not claim an audio track exists before a real media-file probe.
    audio_track_verified: bool,
}

fn validate(seconds: u32, display_id: usize) -> Result<(), String> {
    if !(MIN_SECONDS..=MAX_SECONDS).contains(&seconds) || display_id > 24 {
        Err("Native macOS MP4 recording duration must be 3–120 seconds and display must be valid".into())
    } else {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn moobee_macos_mp4_supported() -> c_int;
    fn moobee_macos_request_stop_mp4() -> c_int;
    fn moobee_macos_record_mp4(
        output_path: *const c_char,
        display_id: u32,
        duration_seconds: c_int,
        system_audio: c_int,
        microphone: c_int,
        error_message: *mut c_char,
        error_capacity: c_int,
    ) -> c_int;
}

#[cfg(target_os = "macos")]
fn record_mp4_to_private_folder(
    folder: std::path::PathBuf,
    display_id: usize,
    seconds: u32,
    system_audio: bool,
    microphone: bool,
) -> Result<Mp4CaptureReport, String> {
    fs::create_dir_all(&folder).map_err(|error| format!("Could not create the clips folder: {error}"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let destination = folder.join(format!("clip-{id}.mp4"));
    // Same-directory rename prevents an incomplete file from being listed.
    let temporary = folder.join(format!(".clip-{id}.mp4.part"));
    let path = CString::new(temporary.to_string_lossy().as_bytes())
        .map_err(|_| "Invalid native MP4 destination path".to_owned())?;
    let mut error = [0 as c_char; 512];

    // The UI stores an XCap monitor-list index. Resolve to macOS CGDirectDisplayID
    // before invoking ScreenCaptureKit; their enumeration orders may differ.
    let monitor = xcap::Monitor::all()
        .map_err(|error| format!("Could not enumerate macOS displays: {error}"))?
        .into_iter()
        .nth(display_id)
        .ok_or_else(|| "Selected display is no longer connected".to_owned())?;
    let mac_display_id = monitor.id()
        .map_err(|error| format!("Could not read macOS display identity: {error}"))?;
    if mac_display_id == 0 {
        return Err("Selected monitor has no valid macOS display identity".into());
    }

    // This synchronous FFI call is always executed via spawn_blocking, not the
    // Tauri GUI thread; ScreenCaptureKit executes its async task on MainActor.
    let result = unsafe {
        moobee_macos_record_mp4(
            path.as_ptr(),
            mac_display_id,
            seconds as c_int,
            i32::from(system_audio),
            i32::from(microphone),
            error.as_mut_ptr(),
            error.len() as c_int,
        )
    };
    if result != 0 {
        // If the native task timed out, it may still be finalizing. Keep the
        // hidden partial file for diagnosis instead of racing the writer.
        if result != -2 {
            let _ = fs::remove_file(&temporary);
        }
        let message = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
        return Err(format!("Native macOS recording failed: {message}"));
    }

    let mut file = OpenOptions::new().read(true).write(true).open(&temporary)
        .map_err(|error| format!("Native recorder created no readable/syncable MP4 file: {error}"))?;
    let len = file.metadata().map_err(|error| error.to_string())?.len();
    if !(4096..=256 * 1024 * 1024).contains(&len) {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err("MP4 file is empty, incomplete, or exceeds the 256 MiB test limit".into());
    }
    // Do not expose a partial output as a finished clip just because it starts
    // with 'ftyp'. Verify all top-level boxes fit inside the file and that the
    // recording includes both movie metadata (moov) and media payload (mdat).
    // This is structural validation only, not independent playback/audio QA.
    if let Err(message) = crate::mp4_validation::validate_mp4_boxes(&mut file, len) {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(format!("Native recording produced an incomplete MP4: {message}"));
    }
    file.sync_all().map_err(|error| format!("Could not synchronize recorded MP4: {error}"))?;
    drop(file);
    fs::rename(&temporary, &destination)
        .map_err(|error| format!("Could not publish finalized local MP4: {error}"))?;

    Ok(Mp4CaptureReport {
        id,
        path: destination.to_string_lossy().into_owned(),
        format: "mp4",
        bytes: len,
        duration_seconds: seconds,
        has_requested_system_audio: system_audio,
        has_requested_microphone: microphone,
        audio_track_verified: false,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsMp4Availability {
    pub(crate) supported: bool,
    required_macos: &'static str,
}

#[tauri::command]
pub fn macos_mp4_availability() -> MacOsMp4Availability {
    #[cfg(target_os = "macos")]
    let supported = unsafe { moobee_macos_mp4_supported() == 1 };
    #[cfg(not(target_os = "macos"))]
    let supported = false;

    MacOsMp4Availability { supported, required_macos: "15.0" }
}

/// Request a graceful stop for the active macOS MP4 session. The finalization
/// and atomic publish still occur in the original capture command.
#[tauri::command]
pub fn stop_macos_mp4() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(unsafe { moobee_macos_request_stop_mp4() != 0 })
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Native MP4 stop is available only on macOS".into())
    }
}

#[tauri::command]
pub async fn capture_macos_mp4(
    app: tauri::AppHandle,
    display_id: usize,
    seconds: u32,
    system_audio: bool,
    microphone: bool,
) -> Result<Mp4CaptureReport, String> {
    validate(seconds, display_id)?;
    #[cfg(target_os = "macos")]
    {
        use crate::clip::{CaptureLock, CAPTURE_BUSY};
        CAPTURE_BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Another native capture/diagnostic is already active".to_owned())?;
        let folder = match app.path().app_data_dir() {
            Ok(path) => path.join("test-clips"),
            Err(error) => {
                CAPTURE_BUSY.store(false, Ordering::Release);
                return Err(format!("Could not access the local recording folder: {error}"));
            }
        };
        tauri::async_runtime::spawn_blocking(move || {
            let _guard = CaptureLock;
            record_mp4_to_private_folder(folder, display_id, seconds, system_audio, microphone)
        }).await.map_err(|error| format!("Native MP4 worker stopped: {error}"))?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, system_audio, microphone);
        Err("This first H.264 MP4 adapter requires the installed macOS 15+ app; Windows native MP4 is still in development".into())
    }
}

#[cfg(test)]
mod tests {
    use super::validate;
    #[test]
    fn test_recording_limits() {
        assert!(validate(3, 0).is_ok());
        assert!(validate(15, 1).is_ok());
        assert!(validate(120, 1).is_ok());
        assert!(validate(2, 0).is_err());
        assert!(validate(121, 0).is_err());
        assert!(validate(10, 25).is_err());
    }
}
