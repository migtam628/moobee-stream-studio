//! Continuous native frame-source experiment (XCap receiver -> composite ->
//! bounded silent MJPEG AVI). Not a realtime H.264/AAC Program recorder.
//! Screen frames have arrival timestamps only: they are NOT native PTS.
//! Real Mac/Windows hardware validation is required before any beta claim.
use serde::Serialize;
use std::{fs::{self, OpenOptions}, path::PathBuf};
use tauri::Manager;

#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::{sync::{atomic::Ordering, mpsc::{Receiver, TryRecvError}}, time::{Duration, Instant}};

const FPS: u32 = 10;
const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const MAX_TOTAL_SOURCE_PIXELS: u64 = 36_000_000;
const MAX_STALE_MS: u64 = 1_500;
const MAX_DRAIN_PER_SOURCE_PER_TICK: usize = 64;

#[derive(Copy, Clone)]
struct EncodingProfile {
    label: &'static str,
    width: u32,
    height: u32,
    fps: u32,
}

fn encoding_profile(value: &str) -> Result<EncodingProfile, String> {
    match value {
        "baseline" => Ok(EncodingProfile {
            label: "baseline", width: WIDTH, height: HEIGHT, fps: FPS,
        }),
        "720p15" => Ok(EncodingProfile {
            label: "720p15", width: 1280, height: 720, fps: 15,
        }),
        _ => Err("Unsupported Program test profile; choose baseline or 720p15".into()),
    }
}


#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinuousProgramReport {
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
    profile: &'static str,
    display_count: usize,
    elapsed_ms: u64,
    measured_sampling_fps: f64,
    source_frames_received: Vec<u32>,
    repeated_source_slots: Vec<u32>,
    late_output_frames: u32,
    max_source_arrival_skew_ms: u64,
}

fn validate_request(display_ids: &[usize], seconds: u32, selected_profile: &str) -> Result<EncodingProfile, String> {
    crate::scene_compositor::validate_ids(display_ids)?;
    if !(2..=8).contains(&seconds) {
        return Err("Continuous Program AVI tests support 2–8 seconds".into());
    }
    let profile = encoding_profile(selected_profile)?;
    if seconds.saturating_mul(profile.fps) > 120 {
        return Err("Requested Program test exceeds AVI's 120-frame safety budget".into());
    }
    Ok(profile)
}

fn validate_frame_geometry(width: u32, height: u32, cumulative: u64) -> Result<u64, String> {
    if width == 0 || height == 0 {
        return Err("A native continuous source delivered an empty frame".into());
    }
    let pixels = u64::from(width).checked_mul(u64::from(height))
        .ok_or("Native frame dimensions overflow")?;
    let total = cumulative.checked_add(pixels).ok_or("Source frame size overflow")?;
    if pixels > MAX_TOTAL_SOURCE_PIXELS || total > MAX_TOTAL_SOURCE_PIXELS {
        return Err("Program display sources exceed the bounded 36-million-pixel capture budget".into());
    }
    Ok(total)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
struct RecorderGuard(Vec<xcap::VideoRecorder>);

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl Drop for RecorderGuard {
    fn drop(&mut self) {
        for recorder in &self.0 {
            let _ = recorder.stop();
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn frame_image(frame: xcap::Frame) -> Result<image::RgbaImage, String> {
    validate_frame_geometry(frame.width, frame.height, 0)?;
    image::RgbaImage::from_raw(frame.width, frame.height, frame.raw)
        .ok_or_else(|| "Invalid native continuous RGBA frame buffer".to_owned())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn receive_first_frame(receiver: &Receiver<xcap::Frame>) -> Result<image::RgbaImage, String> {
    let frame = receiver.recv_timeout(Duration::from_secs(3))
        .map_err(|_| "No native video frames arrived within 3 seconds. Check screen permission, display and capture backend.".to_owned())?;
    frame_image(frame)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_to_dir(
    directory: PathBuf, ids: Vec<usize>, seconds: u32, profile: EncodingProfile,
) -> Result<ContinuousProgramReport, String> {
    use image::{codecs::jpeg::JpegEncoder, DynamicImage};
    let available = xcap::Monitor::all()
        .map_err(|error| format!("Unable to enumerate Program displays: {error}"))?;
    let mut recorders = Vec::with_capacity(ids.len());
    let mut receivers = Vec::with_capacity(ids.len());
    for id in &ids {
        let monitor = available.get(*id)
            .ok_or_else(|| "One Program display is disconnected; refresh displays".to_owned())?;
        let (recorder, receiver) = monitor.video_recorder()
            .map_err(|error| format!("Continuous display source could not start: {error}"))?;
        recorders.push(recorder);
        receivers.push(receiver);
    }
    let guard = RecorderGuard(recorders);
    for recorder in &guard.0 {
        recorder.start().map_err(|error| {
            format!("Failed to start a native video source. Check capture consent: {error}")
        })?;
    }

    // Keep the latest complete frame from each source, not an accumulating
    // recording in memory. Output is clocked independently at nominal 10fps.
    // The receiver channels are owned by the underlying XCap API; our per-tick
    // draining is bounded, and device-side backlog must be hardware-tested.
    let mut latest = Vec::with_capacity(ids.len());
    let mut last_received = Vec::with_capacity(ids.len());
    let mut total_pixels = 0u64;
    for receiver in &receivers {
        let frame = receive_first_frame(receiver)?;
        total_pixels = validate_frame_geometry(frame.width(), frame.height(), total_pixels)?;
        latest.push(frame);
        last_received.push(Instant::now());
    }
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Unable to create private clips directory: {error}"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let final_path = directory.join(format!("clip-{id}.avi"));
    let temporary = directory.join(format!(".clip-{id}.avi.part"));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)
        .map_err(|error| format!("Unable to create private continuous capture output: {error}"))?;

    let started = Instant::now();
    let total_frames = profile.fps * seconds;
    let mut received = vec![1u32; ids.len()];
    let mut repeats = vec![0u32; ids.len()];
    let mut late_frames = 0u32;
    let mut max_skew_ms = 0u64;
    let recording = (|| -> Result<u64, String> {
        let mut writer = crate::avi::AviWriter::new(&mut file, profile.width, profile.height, profile.fps)?;
        for tick in 0..total_frames {
            let target = started + Duration::from_millis(u64::from(tick) * 1000 / u64::from(profile.fps));
            std::thread::sleep(target.saturating_duration_since(Instant::now()));
            for (index, receiver) in receivers.iter().enumerate() {
                let mut was_updated = false;
                for _ in 0..MAX_DRAIN_PER_SOURCE_PER_TICK {
                    match receiver.try_recv() {
                        Ok(frame) => {
                            let image = frame_image(frame)?;
                            let next_pixels = u64::from(image.width()) * u64::from(image.height());
                            let previous_pixels = u64::from(latest[index].width()) *
                                u64::from(latest[index].height());
                            // Reject hotplug/resolution changes that exceed memory budget.
                            if total_pixels - previous_pixels + next_pixels > MAX_TOTAL_SOURCE_PIXELS {
                                return Err("Program displays exceeded the native capture memory budget".into());
                            }
                            total_pixels = total_pixels - previous_pixels + next_pixels;
                            latest[index] = image;
                            last_received[index] = Instant::now();
                            received[index] = received[index].saturating_add(1);
                            was_updated = true;
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) =>
                            return Err("One continuous Program display source disconnected".into()),
                    }
                }
                if !was_updated { repeats[index] = repeats[index].saturating_add(1); }
            }
            let now = Instant::now();
            let most_recent = *last_received.iter().max().unwrap();
            let oldest = *last_received.iter().min().unwrap();
            max_skew_ms = max_skew_ms.max(most_recent.duration_since(oldest).as_millis() as u64);
            if last_received.iter().any(|last| now.duration_since(*last).as_millis() as u64 > MAX_STALE_MS) {
                return Err("Program video input stalled for over 1.5 seconds; incomplete file discarded".into());
            }
            let composed = crate::scene_compositor::composite_grid(&latest, profile.width, profile.height)?;
            let rgb = DynamicImage::ImageRgba8(composed).to_rgb8();
            let mut jpeg = Vec::new();
            JpegEncoder::new_with_quality(&mut jpeg, 72)
                .encode_image(&DynamicImage::ImageRgb8(rgb))
                .map_err(|error| format!("Program frame JPEG encoding failed: {error}"))?;
            writer.write_frame(&jpeg)?;
            if Instant::now() > target + Duration::from_millis(1000 / u64::from(profile.fps)) {
                late_frames = late_frames.saturating_add(1);
            }
        }
        writer.finish()
    })();
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let bytes = match recording {
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
        return Err(format!("Unable to sync continuous Program AVI to disk: {error}"));
    }
    drop(file);
    if let Err(error) = fs::rename(&temporary, &final_path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Unable to publish completed Program AVI: {error}"));
    }
    Ok(ContinuousProgramReport {
        id, path: final_path.to_string_lossy().into_owned(), format: "avi",
        bytes, duration_seconds: seconds, requested_max_seconds: seconds,
        frame_count: total_frames, width: profile.width, height: profile.height, fps: profile.fps,
        profile: profile.label,
        display_count: ids.len(), elapsed_ms,
        measured_sampling_fps: if elapsed_ms == 0 { 0.0 }
            else { f64::from(total_frames) * 1000.0 / elapsed_ms as f64 },
        source_frames_received: received,
        repeated_source_slots: repeats,
        late_output_frames: late_frames,
        max_source_arrival_skew_ms: max_skew_ms,
    })
}

#[tauri::command]
pub async fn capture_continuous_program_avi(
    app: tauri::AppHandle, display_ids: Vec<usize>, seconds: u32, profile: String,
) -> Result<ContinuousProgramReport, String> {
    let selected_profile = validate_request(&display_ids, seconds, &profile)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use crate::clip::{CaptureLock, CAPTURE_BUSY};
        CAPTURE_BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Another native recording or capture diagnostic is already running".to_owned())?;
        let directory = match app.path().app_data_dir() {
            Ok(path) => path.join("test-clips"),
            Err(error) => {
                CAPTURE_BUSY.store(false, Ordering::Release);
                return Err(format!("Unable to locate MooBee clips directory: {error}"));
            }
        };
        tauri::async_runtime::spawn_blocking(move || {
            let _guard = CaptureLock;
            capture_to_dir(directory, display_ids, seconds, selected_profile)
        }).await.map_err(|error| format!("Continuous Program capture worker stopped: {error}"))?
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (app, display_ids, selected_profile);
        Err("Continuous Program AVI capture requires a Mac or Windows desktop build".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_request, validate_frame_geometry, MAX_TOTAL_SOURCE_PIXELS};
    #[test]
    fn validates_display_count_uniqueness_and_duration() {
        assert!(validate_request(&[0], 2, "baseline").is_ok());
        assert!(validate_request(&[0, 1, 2, 3], 8, "baseline").is_ok());
        assert!(validate_request(&[], 4, "baseline").is_err());
        assert!(validate_request(&[1, 1], 4, "baseline").is_err());
        assert!(validate_request(&[0, 1, 2, 3, 4], 4, "baseline").is_err());
        assert!(validate_request(&[0], 9, "baseline").is_err());
        let high = validate_request(&[0, 1], 8, "720p15").unwrap();
        assert_eq!((high.width, high.height, high.fps), (1280, 720, 15));
        assert_eq!(high.label, "720p15");
        assert_eq!(8 * high.fps, 120);
        assert!(validate_request(&[0], 3, "1080p60").is_err());
        assert_eq!(validate_request(&[0], 4, "baseline").unwrap().fps, 10);
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn high_detail_profile_composes_and_encodes_real_720p_jpeg_frames() {
        use image::{codecs::jpeg::JpegEncoder, DynamicImage, Rgba, RgbaImage};
        let profile = validate_request(&[0, 1], 4, "720p15").unwrap();
        let red = RgbaImage::from_pixel(64, 64, Rgba([240, 10, 10, 255]));
        let blue = RgbaImage::from_pixel(64, 64, Rgba([10, 10, 240, 255]));
        let composed = crate::scene_compositor::composite_grid(
            &[red, blue], profile.width, profile.height
        ).unwrap();
        assert_eq!(composed.dimensions(), (1280, 720));
        assert!(composed.get_pixel(320, 360)[0] > 200);
        assert!(composed.get_pixel(960, 360)[2] > 200);
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 72)
            .encode_image(&DynamicImage::ImageRgb8(
                DynamicImage::ImageRgba8(composed).to_rgb8()
            )).unwrap();
        let decoded = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1280, 720));
        let avi = crate::avi::encode_silent_mjpeg_avi(
            &vec![jpeg; 4], profile.width, profile.height, profile.fps
        ).unwrap();
        assert_eq!(&avi[0..4], b"RIFF");
        assert_eq!(&avi[8..12], b"AVI ");
        assert!(avi.windows(4).any(|window| window == b"idx1"));
    }

    #[test]
    fn frames_have_bounded_pixel_budget() {
        assert_eq!(validate_frame_geometry(1920, 1080, 0).unwrap(), 2_073_600);
        assert!(validate_frame_geometry(0, 1080, 0).is_err());
        assert!(validate_frame_geometry(1920, 1080, MAX_TOTAL_SOURCE_PIXELS).is_err());
        assert!(validate_frame_geometry(u32::MAX, u32::MAX, 0).is_err());
    }
}
