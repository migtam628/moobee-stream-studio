//! Measure the independent desktop capture source before wiring a real-time encoder.
//! This diagnostic intentionally NEVER declares recording/streaming available.
use serde::Serialize;

#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::{
    sync::atomic::Ordering,
    sync::mpsc::RecvTimeoutError,
    time::{Duration, Instant},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureProbeReport {
    display_id: usize,
    duration_ms: u64,
    frame_count: u64,
    width: u32,
    height: u32,
    average_fps: f64,
    first_frame_ms: Option<u64>,
    longest_frame_gap_ms: Option<u64>,
    mean_frame_gap_ms: Option<f64>,
}

fn validate_duration(seconds: u32) -> Result<(), String> {
    if (2..=5).contains(&seconds) {
        Ok(())
    } else {
        Err("Frame diagnostics must run for 2–5 seconds".into())
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn inspect_display(display_id: usize, seconds: u32) -> Result<CaptureProbeReport, String> {
    let monitor = xcap::Monitor::all()
        .map_err(|error| format!("Could not enumerate displays: {error}"))?
        .into_iter()
        .nth(display_id)
        .ok_or_else(|| "Display not found. Refresh available displays.".to_owned())?;

    let (recorder, receiver) = monitor
        .video_recorder()
        .map_err(|error| format!("The native video frame source could not start: {error}"))?;
    recorder.start().map_err(|error| format!("Native screen recording permission or capture failed: {error}"))?;

    // Ensure the capture backend always stops, including when receive fails.
    struct StopOnDrop<'a>(&'a xcap::VideoRecorder);
    impl Drop for StopOnDrop<'_> {
        fn drop(&mut self) {
            let _ = self.0.stop();
        }
    }
    let _stop = StopOnDrop(&recorder);

    let started = Instant::now();
    let duration = Duration::from_secs(u64::from(seconds));
    let mut frames = 0u64;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut first_frame_ms = None;
    let mut last_at: Option<Instant> = None;
    let mut longest_gap_ms: Option<u64> = None;
    let mut total_gap_ms = 0.0f64;

    while started.elapsed() < duration {
        let remaining = duration.saturating_sub(started.elapsed());
        let wait = remaining.min(Duration::from_millis(250));
        if wait.is_zero() {
            break;
        }
        match receiver.recv_timeout(wait) {
            Ok(frame) => {
                if frame.width == 0 || frame.height == 0 || frame.raw.is_empty() {
                    continue;
                }
                let received_at = Instant::now();
                if first_frame_ms.is_none() {
                    first_frame_ms = Some(received_at.duration_since(started).as_millis() as u64);
                }
                if let Some(previous) = last_at {
                    let gap = received_at.duration_since(previous).as_secs_f64() * 1000.0;
                    total_gap_ms += gap;
                    let as_ms = gap.round() as u64;
                    longest_gap_ms = Some(longest_gap_ms.unwrap_or(0).max(as_ms));
                }
                last_at = Some(received_at);
                width = frame.width;
                height = frame.height;
                frames += 1;
                // Do not retain raw frames. This is a source performance probe, NOT a recorder.
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    let duration_ms = started.elapsed().as_millis() as u64;
    if frames == 0 {
        return Err("No native video frames were delivered. Check Screen Recording permission and the selected display.".into());
    }
    let mean_frame_gap_ms = if frames > 1 {
        Some(total_gap_ms / (frames - 1) as f64)
    } else {
        None
    };
    Ok(CaptureProbeReport {
        display_id,
        duration_ms,
        frame_count: frames,
        width,
        height,
        average_fps: frames as f64 / (duration_ms as f64 / 1000.0).max(0.001),
        first_frame_ms,
        longest_frame_gap_ms: longest_gap_ms,
        mean_frame_gap_ms,
    })
}

#[tauri::command]
pub async fn probe_native_capture(display_id: usize, seconds: u32) -> Result<CaptureProbeReport, String> {
    validate_duration(seconds)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use crate::clip::{CaptureLock, CAPTURE_BUSY};
        CAPTURE_BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "A native test clip or frame diagnostic is already running".to_owned())?;
        tauri::async_runtime::spawn_blocking(move || {
            let _lock = CaptureLock;
            inspect_display(display_id, seconds)
        })
        .await
        .map_err(|error| format!("Native frame diagnostic worker stopped: {error}"))?
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = display_id;
        Err("Continuous native frame diagnostics currently require Windows or macOS".into())
    }
}

#[cfg(test)]
mod tests {
    use super::validate_duration;

    #[test]
    fn only_accepts_bounded_frame_diagnostics() {
        assert!(validate_duration(2).is_ok());
        assert!(validate_duration(5).is_ok());
        assert!(validate_duration(1).is_err());
        assert!(validate_duration(6).is_err());
    }
}
