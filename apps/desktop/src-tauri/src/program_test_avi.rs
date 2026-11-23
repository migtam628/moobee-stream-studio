//! Bounded experimental scene-to-file vertical slice. Samples 1–4 native
//! display sources and writes a composited silent MJPEG AVI. Not realtime
//! Program broadcast: no webcam, overlays, audio, or H.264/RTMPS.
use serde::Serialize;
use std::{fs::{self, OpenOptions}, path::PathBuf};
use tauri::Manager;

#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::{sync::atomic::Ordering, time::{Duration, Instant}};

const TEST_FPS: u32 = 10;
const TEST_WIDTH: u32 = 640;
const TEST_HEIGHT: u32 = 360;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramTestAviReport {
    id: String,
    path: String,
    format: &'static str,
    bytes: u64,
    duration_seconds: u32,
    requested_max_seconds: u32,
    frame_count: u32,
    width: u32,
    height: u32,
    fps: u32,
    display_count: usize,
    elapsed_ms: u64,
    measured_sampling_fps: f64,
}

fn check_program_test(display_ids: &[usize], seconds: u32) -> Result<(), String> {
    crate::scene_compositor::validate_ids(display_ids)?;
    if !(2..=8).contains(&seconds) {
        return Err("Composited AVI tests must last between 2 and 8 seconds".into());
    }
    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_program_test_to_dir(
    directory: PathBuf,
    display_ids: Vec<usize>,
    seconds: u32,
) -> Result<ProgramTestAviReport, String> {
    use image::{codecs::jpeg::JpegEncoder, DynamicImage};

    let available = xcap::Monitor::all()
        .map_err(|error| format!("Could not enumerate scene displays: {error}"))?;
    let monitors: Vec<&xcap::Monitor> = display_ids.iter().map(|id| {
        available.get(*id).ok_or_else(|| {
            "A Program display has been disconnected. Refresh displays and retry.".to_owned()
        })
    }).collect::<Result<_, _>>()?;

    fs::create_dir_all(&directory)
        .map_err(|error| format!("Unable to create private clips directory: {error}"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let destination = directory.join(format!("clip-{id}.avi"));
    let temporary = directory.join(format!(".clip-{id}.avi.part"));
    let mut file = OpenOptions::new().write(true).create_new(true)
        .open(&temporary).map_err(|error| format!("Unable to create private AVI: {error}"))?;

    // File remains unpublished unless all frames, index and disk sync succeed.
    // Each frame captures displays sequentially, not with synchronized clocks.
    // If native screenshot capture misses deadlines, wall-clock and fixed AVI
    // playback time can differ; this is reported as a test limitation.
    let started = Instant::now();
    let total_frames = seconds * TEST_FPS;
    let record = (|| -> Result<u64, String> {
        let mut writer = crate::avi::AviWriter::new(&mut file, TEST_WIDTH, TEST_HEIGHT, TEST_FPS)?;
        for index in 0..total_frames {
            let target = started + Duration::from_millis(
                u64::from(index) * 1000 / u64::from(TEST_FPS)
            );
            std::thread::sleep(target.saturating_duration_since(Instant::now()));
            let mut images = Vec::with_capacity(monitors.len());
            for monitor in &monitors {
                let image = monitor.capture_image().map_err(|error| {
                    format!("Program display capture failed; check screen permission: {error}")
                })?;
                if image.width() == 0 || image.height() == 0 {
                    return Err("Native display produced an empty video frame".into());
                }
                images.push(image);
            }
            let composite = crate::scene_compositor::composite_grid(
                &images, TEST_WIDTH, TEST_HEIGHT
            )?;
            let mut jpeg = Vec::new();
            let rgb = DynamicImage::ImageRgba8(composite).to_rgb8();
            JpegEncoder::new_with_quality(&mut jpeg, 72)
                .encode_image(&DynamicImage::ImageRgb8(rgb))
                .map_err(|error| format!("Composited JPEG encoding failed: {error}"))?;
            writer.write_frame(&jpeg)?;
        }
        writer.finish()
    })();
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let bytes = match record {
        Ok(bytes) => bytes,
        Err(error) => {
            drop(file);
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
    };
    if let Err(error) = file.sync_all() {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(format!("Could not synchronize composed AVI: {error}"));
    }
    drop(file);
    if let Err(error) = fs::rename(&temporary, &destination) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Could not publish finalized AVI: {error}"));
    }
    Ok(ProgramTestAviReport {
        id, path: destination.to_string_lossy().into_owned(), format: "avi",
        bytes, duration_seconds: seconds, requested_max_seconds: seconds, frame_count: total_frames,
        width: TEST_WIDTH, height: TEST_HEIGHT, fps: TEST_FPS,
        display_count: monitors.len(), elapsed_ms,
        measured_sampling_fps: if elapsed_ms > 0 {
            f64::from(total_frames) * 1000.0 / elapsed_ms as f64
        } else { 0.0 },
    })
}

#[tauri::command]
pub async fn capture_program_test_avi(
    app: tauri::AppHandle,
    display_ids: Vec<usize>,
    seconds: u32,
) -> Result<ProgramTestAviReport, String> {
    check_program_test(&display_ids, seconds)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use crate::clip::{CaptureLock, CAPTURE_BUSY};
        CAPTURE_BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Another native recording or capture diagnostic is running".to_owned())?;
        let directory = match app.path().app_data_dir() {
            Ok(dir) => dir.join("test-clips"),
            Err(error) => {
                CAPTURE_BUSY.store(false, Ordering::Release);
                return Err(format!("Unable to locate private clips directory: {error}"));
            }
        };
        tauri::async_runtime::spawn_blocking(move || {
            let _capture_guard = CaptureLock;
            capture_program_test_to_dir(directory, display_ids, seconds)
        }).await.map_err(|error| format!("Program AVI test worker stopped: {error}"))?
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (app, display_ids);
        Err("Native Program AVI tests require the installed Mac or Windows app".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{check_program_test, TEST_FPS, TEST_HEIGHT, TEST_WIDTH};
    #[test]
    fn program_avi_bounds_and_unique_source_validation() {
        assert!(check_program_test(&[0], 2).is_ok());
        assert!(check_program_test(&[0, 1, 2, 3], 8).is_ok());
        assert!(check_program_test(&[], 4).is_err());
        assert!(check_program_test(&[0, 0], 4).is_err());
        assert!(check_program_test(&[0, 1, 2, 3, 4], 4).is_err());
        assert!(check_program_test(&[25], 4).is_err());
        assert!(check_program_test(&[0], 1).is_err());
        assert!(check_program_test(&[0], 9).is_err());
        assert_eq!((TEST_FPS, TEST_WIDTH, TEST_HEIGHT), (10, 640, 360));
    }
}
