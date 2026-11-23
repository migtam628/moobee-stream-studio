//! First-party *still-frame* multi-display scene preview.
//! This is a compositing primitive, not a real-time video encoder or RTMP
//! output. It combines up to four native display captures into one frame.
//! The future Program video compositor must share source geometry and timing,
//! rather than treating this snapshot function as a stream.

#[cfg(any(target_os = "windows", target_os = "macos"))]
use image::{imageops, ImageFormat, Rgba, RgbaImage};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use base64::Engine as _;

const MAX_PREVIEW_SOURCES: usize = 4;
const OUTPUT_WIDTH: u32 = 960;
const OUTPUT_HEIGHT: u32 = 540;

pub(crate) fn validate_ids(ids: &[usize]) -> Result<(), String> {
    if ids.is_empty() || ids.len() > MAX_PREVIEW_SOURCES || ids.iter().any(|id| *id > 24) ||
        ids.iter().enumerate().any(|(index, id)| ids[..index].contains(id)) {
        return Err("Native scene preview supports one to four valid display sources".into());
    }
    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) fn composite_grid(frames: &[RgbaImage], width: u32, height: u32) -> Result<RgbaImage, String> {
    if frames.is_empty() || frames.len() > MAX_PREVIEW_SOURCES ||
       width < 16 || height < 16 || width > 1920 || height > 1080 {
        return Err("Invalid scene preview dimensions or source count".into());
    }
    if frames.iter().any(|frame| frame.width() == 0 || frame.height() == 0) {
        return Err("Scene preview source is empty".into());
    }
    let columns = if frames.len() == 1 { 1u32 } else { 2u32 };
    let rows = if frames.len() <= 2 { 1u32 } else { 2u32 };
    let tile_width = width / columns;
    let tile_height = height / rows;
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([13, 15, 26, 255]));
    for (i, frame) in frames.iter().enumerate() {
        let col = i as u32 % columns;
        let row = i as u32 / columns;
        let scale = f64::min(
            f64::from(tile_width) / f64::from(frame.width()),
            f64::from(tile_height) / f64::from(frame.height()),
        );
        let scaled_width = (f64::from(frame.width()) * scale).round().max(1.0) as u32;
        let scaled_height = (f64::from(frame.height()) * scale).round().max(1.0) as u32;
        let scaled = imageops::resize(
            frame, scaled_width.min(tile_width), scaled_height.min(tile_height),
            imageops::FilterType::Triangle,
        );
        let x = col * tile_width + (tile_width - scaled.width()) / 2;
        let y = row * tile_height + (tile_height - scaled.height()) / 2;
        imageops::overlay(&mut canvas, &scaled, i64::from(x), i64::from(y));
    }
    Ok(canvas)
}

/// Return a PNG data URL of the selected scene's captured displays.
/// Only integer monitor-list indices are accepted, never arbitrary file paths.
#[tauri::command]
pub async fn capture_scene_snapshot(display_ids: Vec<usize>) -> Result<String, String> {
    validate_ids(&display_ids)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        tauri::async_runtime::spawn_blocking(move || {
        let monitors = xcap::Monitor::all()
            .map_err(|error| format!("Unable to enumerate scene displays: {error}"))?;
        let mut frames = Vec::with_capacity(display_ids.len());
        for id in display_ids {
            let monitor = monitors.get(id)
                .ok_or_else(|| "A scene display is no longer connected".to_owned())?;
            frames.push(monitor.capture_image()
                .map_err(|error| format!("Screen Recording access or capture failed: {error}"))?);
        }
        let frame = composite_grid(&frames, OUTPUT_WIDTH, OUTPUT_HEIGHT)?;
        let mut buffer = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(frame)
            .write_to(&mut buffer, ImageFormat::Png)
            .map_err(|error| format!("Unable to encode scene preview PNG: {error}"))?;
        Ok(format!("data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(buffer.into_inner())))
        }).await.map_err(|error| format!("Scene preview worker stopped: {error}"))?
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = display_ids;
        Err("Native multi-display preview is available on macOS and Windows only".into())
    }
}

#[cfg(test)]
mod tests {
    use super::validate_ids;

    #[test]
    fn rejects_empty_too_many_and_invalid_display_ids() {
        assert!(validate_ids(&[]).is_err());
        assert!(validate_ids(&[0]).is_ok());
        assert!(validate_ids(&[0, 1, 2, 3]).is_ok());
        assert!(validate_ids(&[0, 1, 2, 3, 4]).is_err());
        assert!(validate_ids(&[25]).is_err());
        assert!(validate_ids(&[0, 0]).is_err());
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn composites_two_distinct_display_frames_into_single_canvas() {
        use super::composite_grid;
        use image::{Rgba, RgbaImage};
        let red = RgbaImage::from_pixel(32, 32, Rgba([250, 0, 0, 255]));
        let blue = RgbaImage::from_pixel(32, 32, Rgba([0, 0, 250, 255]));
        let output = composite_grid(&[red, blue], 200, 100).unwrap();
        assert_eq!(output.dimensions(), (200, 100));
        assert!(output.get_pixel(50, 50)[0] > 200);
        assert!(output.get_pixel(150, 50)[2] > 200);
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn letterboxes_frame_without_distorting_aspect_ratio() {
        use super::composite_grid;
        use image::{Rgba, RgbaImage};
        let red = RgbaImage::from_pixel(100, 20, Rgba([250, 0, 0, 255]));
        let output = composite_grid(&[red], 200, 100).unwrap();
        assert!(output.get_pixel(100, 50)[0] > 200);
        assert_eq!(output.get_pixel(100, 0)[0], 13);
    }
}
