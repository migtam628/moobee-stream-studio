//! Experimental short silent MJPEG AVI capture with independently bundled Rust code.
//! NOT the production recorder. No audio, arbitrary scene composition, or streaming.
use serde::Serialize;
use tauri::Manager;
use std::{fs, path::PathBuf};

#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::{
    sync::atomic::Ordering,
    sync::mpsc::RecvTimeoutError,
    time::{Duration, Instant},
};

const AVI_MAX_DURATION_SECONDS: u32 = 8;
const AVI_FPS: u32 = 10;
const AVI_WIDTH: u32 = 640;
const AVI_HEIGHT: u32 = 360;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AviCaptureReport {
    id: String,
    path: String,
    format: &'static str,
    bytes: u64,
    duration_seconds: u32,
    frame_count: u32,
    source_frame_count: u32,
    width: u32,
    height: u32,
    fps: u32,
}

fn check_duration(seconds: u32) -> Result<(), String> {
    if (2..=AVI_MAX_DURATION_SECONDS).contains(&seconds) {
        Ok(())
    } else {
        Err("Silent AVI test recordings must last between 2 and 8 seconds".into())
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn jpg_from_frame(frame: xcap::Frame) -> Result<Vec<u8>, String> {
    use image::{
        codecs::jpeg::JpegEncoder, imageops::{overlay, FilterType},
        DynamicImage, Rgb, RgbImage, RgbaImage,
    };
    let raw = RgbaImage::from_raw(frame.width, frame.height, frame.raw)
        .ok_or_else(|| "Unexpected native RGBA frame layout".to_owned())?;
    let scaled = DynamicImage::ImageRgba8(raw)
        .resize(AVI_WIDTH, AVI_HEIGHT, FilterType::Triangle)
        .to_rgb8();
    let mut canvas = RgbImage::from_pixel(AVI_WIDTH, AVI_HEIGHT, Rgb([15, 14, 22]));
    let x = (AVI_WIDTH - scaled.width()) as i64 / 2;
    let y = (AVI_HEIGHT - scaled.height()) as i64 / 2;
    overlay(&mut canvas, &scaled, x, y);
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 72)
        .encode_image(&DynamicImage::ImageRgb8(canvas))
        .map_err(|error| format!("Native video JPEG encoding failed: {error}"))?;
    Ok(jpeg)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_silent_avi_to_dir(
    directory: PathBuf,
    display_id: usize,
    seconds: u32,
) -> Result<AviCaptureReport, String> {
    let monitor = xcap::Monitor::all()
        .map_err(|error| format!("Display enumeration failed: {error}"))?
        .into_iter()
        .nth(display_id)
        .ok_or_else(|| "Display was not found. Refresh available monitors.".to_owned())?;
    let (recorder, frames) = monitor.video_recorder()
        .map_err(|error| format!("Continuous video source could not be created: {error}"))?;
    recorder.start().map_err(|error| format!(
        "Could not start native screen capture; check Screen Recording permission: {error}"
    ))?;
    struct StopCapture<'a>(&'a xcap::VideoRecorder);
    impl Drop for StopCapture<'_> {
        fn drop(&mut self) {
            let _ = self.0.stop();
        }
    }
    let _guard = StopCapture(&recorder);

    // Wait for the first actual input frame before the fixed-rate AVI clock starts.
    let mut newest = frames.recv_timeout(Duration::from_secs(2))
        .map_err(|_| "No native video frames arrived within 2 seconds; check screen capture permissions".to_owned())?;
    if newest.width == 0 || newest.height == 0 || newest.raw.is_empty() {
        return Err("Native capture delivered an invalid initial frame".into());
    }
    let started = Instant::now();
    let frame_count = AVI_FPS * seconds;
    let mut sampled = Vec::with_capacity(frame_count as usize);
    let mut source_frame_count = 1u32;

    for i in 0..frame_count {
        let target = started + Duration::from_millis(u64::from(i) * 1000 / u64::from(AVI_FPS));
        // Keep just the most recently received raw frame, avoiding unbounded video buffers.
        while Instant::now() < target {
            let remaining = target.saturating_duration_since(Instant::now());
            let wait = remaining.min(Duration::from_millis(30));
            match frames.recv_timeout(wait) {
                Ok(frame) if frame.width > 0 && frame.height > 0 && !frame.raw.is_empty() => {
                    newest = frame;
                    source_frame_count += 1;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("Continuous capture source disconnected before completion".into());
                }
                _ => {}
            }
        }
        // Drain at most 64 ready frames to use the freshest without blocking indefinitely.
        for _ in 0..64 {
            match frames.try_recv() {
                Ok(frame) if frame.width > 0 && frame.height > 0 && !frame.raw.is_empty() => {
                    newest = frame;
                    source_frame_count += 1;
                }
                _ => break,
            }
        }
        // XCap's raw frame buffer is RGBA. Encode this sample at a fixed 640x360 target.
        // Duplicating the previous capture if input stalls maintains AVI timing.
        sampled.push(jpg_from_frame(xcap::Frame {
            width: newest.width,
            height: newest.height,
            raw: newest.raw.clone(),
        })?);
    }
    let _observed_elapsed = started.elapsed();
    let avi = crate::avi::encode_silent_mjpeg_avi(
        &sampled, AVI_WIDTH, AVI_HEIGHT, AVI_FPS
    )?;

    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let dest = directory.join(format!("clip-{id}.avi"));
    let tmp = directory.join(format!(".clip-{id}.avi.part"));
    fs::write(&tmp, &avi).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&tmp, &dest) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Saving local AVI failed: {error}"));
    }
    Ok(AviCaptureReport {
        id,
        path: dest.to_string_lossy().to_string(),
        format: "avi",
        bytes: avi.len() as u64,
        duration_seconds: seconds,
        frame_count,
        source_frame_count,
        width: AVI_WIDTH,
        height: AVI_HEIGHT,
        fps: AVI_FPS,
    })
}

#[tauri::command]
pub async fn capture_silent_avi(
    app: tauri::AppHandle,
    display_id: usize,
    seconds: u32,
) -> Result<AviCaptureReport, String> {
    check_duration(seconds)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use crate::clip::{CaptureLock, CAPTURE_BUSY};
        CAPTURE_BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "A native capture or input probe is already running".to_owned())?;
        let directory = match app.path().app_data_dir() {
            Ok(path) => path.join("test-clips"),
            Err(error) => {
                CAPTURE_BUSY.store(false, Ordering::Release);
                return Err(format!("Could not access local application data: {error}"));
            }
        };
        tauri::async_runtime::spawn_blocking(move || {
            let _exclusive_capture = CaptureLock;
            capture_silent_avi_to_dir(directory, display_id, seconds)
        }).await.map_err(|error| format!("Native AVI capture worker stopped: {error}"))?
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (app, display_id);
        Err("Native AVI capture currently requires the installed Windows or macOS app".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{check_duration, AVI_FPS, AVI_HEIGHT, AVI_WIDTH};
    #[test]
    fn avi_sampling_is_safely_bounded() {
        assert!(check_duration(2).is_ok());
        assert!(check_duration(8).is_ok());
        assert!(check_duration(1).is_err());
        assert!(check_duration(9).is_err());
        assert_eq!((AVI_FPS, AVI_WIDTH, AVI_HEIGHT), (10, 640, 360));
    }
}
