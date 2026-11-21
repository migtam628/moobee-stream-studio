//! Experimental standalone *silent GIF* capture. This is NOT the production video recorder.
 //! MooBee owns the capture and file output; no OBS, FFmpeg process, or external API is required.
use serde::Serialize;
use std::{fs, path::PathBuf};
use tauri::Manager;

#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) static CAPTURE_BUSY: AtomicBool = AtomicBool::new(false);
const MAX_CLIP_BYTES: u64 = 32 * 1024 * 1024;

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) struct CaptureLock;

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl Drop for CaptureLock {
    fn drop(&mut self) {
        CAPTURE_BUSY.store(false, Ordering::Release);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipInfo {
    id: String,
    path: String,
    bytes: u64,
    modified_ms: u128,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipCreated {
    id: String,
    path: String,
    bytes: u64,
    format: &'static str,
    duration_seconds: u32,
    frame_count: u32,
    width: u32,
    height: u32,
}

fn check_seconds(seconds: u32) -> Result<(), String> {
    if (2..=10).contains(&seconds) {
        Ok(())
    } else {
        Err("Test clips must be between 2 and 10 seconds".to_owned())
    }
}

fn clips_folder(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("test-clips"))
}

fn clip_path(app: &tauri::AppHandle, id: &str) -> Result<PathBuf, String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "Invalid clip identifier")?;
    if parsed.to_string() != id {
        return Err("Invalid clip identifier".to_owned());
    }
    Ok(clips_folder(app)?.join(format!("clip-{id}.gif")))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_gif(directory: PathBuf, display_id: usize, seconds: u32) -> Result<ClipCreated, String> {
    use image::{codecs::gif::GifEncoder, Delay, DynamicImage, Frame, imageops::FilterType};

    let monitor = xcap::Monitor::all()
        .map_err(|error| format!("Unable to enumerate displays: {error}"))?
        .into_iter()
        .nth(display_id)
        .ok_or_else(|| "Display not found. Refresh the monitor list.".to_owned())?;

    // This intentionally samples at just 2 fps, 640x360 maximum, with no audio.
    // It is a measurable native capture/encode/file-output slice, not 1080p60 capture.
    let frame_count = seconds * 2;
    let mut frames = Vec::with_capacity(frame_count as usize);
    let started = Instant::now();
    let mut dimensions = (0, 0);
    for index in 0..frame_count {
        let deadline = started + Duration::from_millis(500 * u64::from(index));
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        let screen = monitor
            .capture_image()
            .map_err(|error| format!("Screen capture failed. Check screen recording permission: {error}"))?;
        let frame = DynamicImage::ImageRgba8(screen)
            .resize(640, 360, FilterType::Triangle)
            .to_rgba8();
        dimensions = frame.dimensions();
        frames.push(Frame::from_parts(
            frame, 0, 0, Delay::from_numer_denom_ms(500, 1),
        ));
    }

    let mut encoded = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut encoded, 10);
        for frame in frames {
            encoder.encode_frame(frame).map_err(|error| format!("GIF encoding failed: {error}"))?;
        }
    }
    if encoded.is_empty() || encoded.len() as u64 > MAX_CLIP_BYTES {
        return Err("No valid test clip was generated".into());
    }

    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let destination = directory.join(format!("clip-{id}.gif"));
    let temporary = directory.join(format!(".clip-{id}.tmp"));
    fs::write(&temporary, &encoded).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&temporary, &destination) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Could not save test clip: {error}"));
    }

    Ok(ClipCreated {
        id,
        path: destination.to_string_lossy().into_owned(),
        bytes: encoded.len() as u64,
        format: "gif",
        duration_seconds: seconds,
        frame_count,
        width: dimensions.0,
        height: dimensions.1,
    })
}

#[tauri::command]
pub async fn capture_test_clip(
    app: tauri::AppHandle,
    display_id: usize,
    seconds: u32,
) -> Result<ClipCreated, String> {
    check_seconds(seconds)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        CAPTURE_BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Another test clip capture is already in progress".to_owned())?;
        let directory = match clips_folder(&app) {
            Ok(path) => path,
            Err(error) => {
                CAPTURE_BUSY.store(false, Ordering::Release);
                return Err(error);
            }
        };
        tauri::async_runtime::spawn_blocking(move || {
            let _lock = CaptureLock;
            capture_gif(directory, display_id, seconds)
        })
        .await
        .map_err(|error| format!("Capture worker stopped: {error}"))?
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (app, display_id);
        Err("Native test clips are currently supported on Windows and macOS only".into())
    }
}

#[tauri::command]
pub fn list_test_clips(app: tauri::AppHandle) -> Result<Vec<ClipInfo>, String> {
    let directory = clips_folder(&app)?;
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut clips = Vec::new();
    for item in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = item.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(id) = name.strip_prefix("clip-").and_then(|n| n.strip_suffix(".gif")) {
            if uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id) {
                let meta = entry.metadata().map_err(|error| error.to_string())?;
                if meta.is_file() && meta.len() <= MAX_CLIP_BYTES {
                    let modified_ms = meta
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |duration| duration.as_millis());
                    clips.push(ClipInfo {
                        id: id.to_owned(),
                        path: entry.path().to_string_lossy().into_owned(),
                        bytes: meta.len(),
                        modified_ms,
                    });
                }
            }
        }
    }
    clips.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms).then_with(|| b.id.cmp(&a.id)));
    clips.truncate(50);
    Ok(clips)
}

#[tauri::command]
pub fn read_test_clip(app: tauri::AppHandle, clip_id: String) -> Result<String, String> {
    use base64::Engine as _;
    let path = clip_path(&app, &clip_id)?;
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_CLIP_BYTES {
        return Err("Clip is missing or too large to preview".into());
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    if !bytes.starts_with(b"GIF8") {
        return Err("File is not a supported GIF clip".into());
    }
    Ok(format!(
        "data:image/gif;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::{check_seconds, MAX_CLIP_BYTES};
    #[test]
    fn test_clip_duration_is_bounded() {
        assert!(check_seconds(2).is_ok());
        assert!(check_seconds(10).is_ok());
        assert!(check_seconds(0).is_err());
        assert!(check_seconds(11).is_err());
        assert_eq!(MAX_CLIP_BYTES, 32 * 1024 * 1024);
    }
}
