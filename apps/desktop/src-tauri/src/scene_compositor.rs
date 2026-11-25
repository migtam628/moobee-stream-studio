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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SceneLayout { Grid, PictureInPicture }

impl SceneLayout {
    pub(crate) fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("grid") {
            "grid" => Ok(Self::Grid),
            "pip" => Ok(Self::PictureInPicture),
            _ => Err("Unsupported native scene layout".into()),
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) fn composite_grid(frames: &[RgbaImage], width: u32, height: u32) -> Result<RgbaImage, String> {
    composite_layout(frames, width, height, SceneLayout::Grid)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) fn composite_layout(frames: &[RgbaImage], width: u32, height: u32, layout: SceneLayout) -> Result<RgbaImage, String> {
    let layers: Vec<&RgbaImage> = frames.iter().collect();
    composite_layers(&layers, width, height, layout)
}

#[cfg(target_os = "macos")]
pub(crate) fn composite_with_camera(displays: &[RgbaImage], camera: &RgbaImage, width: u32, height: u32) -> Result<RgbaImage, String> {
    if displays.is_empty() || displays.len() > 3 { return Err("Webcam PiP needs one to three displays".into()); }
    // Camera becomes the last inset. All source buffers remain borrowed.
    let mut layers: Vec<&RgbaImage> = displays.iter().collect();
    layers.push(camera);
    composite_layers(&layers, width, height, SceneLayout::PictureInPicture)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn composite_layers(frames: &[&RgbaImage], width: u32, height: u32, layout: SceneLayout) -> Result<RgbaImage, String> {
    if frames.is_empty() || frames.len() > MAX_PREVIEW_SOURCES ||
       width < 16 || height < 16 || width > 1920 || height > 1080 {
        return Err("Invalid scene dimensions or source count".into());
    }
    if frames.iter().any(|frame| frame.width() == 0 || frame.height() == 0) {
        return Err("Scene source is empty".into());
    }
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([13, 15, 26, 255]));
    let columns = if frames.len() == 1 { 1u32 } else { 2u32 };
    let rows = if frames.len() <= 2 { 1u32 } else { 2u32 };
    for (i, frame) in frames.iter().enumerate() {
        let (left, top, tile_width, tile_height) = if layout == SceneLayout::PictureInPicture {
            if i == 0 {
                (0, 0, width, height)
            } else {
                // Later sources are opaque inset layers in source-list order.
                // Use at most 3 stacked insets, wholly inside the canvas.
                let margin = (height / 40).min(width / 10).max(1);
                let w = (width / 3).max(1);
                let h = ((height - margin * 4) / 3).min((w * 9 / 16).max(1)).max(1);
                (width - w - margin, height - margin - i as u32 * h - (i as u32 - 1) * margin, w, h)
            }
        } else {
            let w = width / columns;
            let h = height / rows;
            (i as u32 % columns * w, i as u32 / columns * h, w, h)
        };
        let scale = f64::min(f64::from(tile_width) / f64::from(frame.width()),
            f64::from(tile_height) / f64::from(frame.height()));
        let w = (f64::from(frame.width()) * scale).round().max(1.0) as u32;
        let h = (f64::from(frame.height()) * scale).round().max(1.0) as u32;
        let scaled = imageops::resize(*frame, w.min(tile_width), h.min(tile_height), imageops::FilterType::Triangle);
        if layout == SceneLayout::PictureInPicture && i > 0 {
            for y in top..top + tile_height {
                for x in left..left + tile_width {
                    canvas.put_pixel(x, y, Rgba([13, 15, 26, 255]));
                }
            }
        }
        let x = left + (tile_width - scaled.width()) / 2;
        let y = top + (tile_height - scaled.height()) / 2;
        imageops::overlay(&mut canvas, &scaled, i64::from(x), i64::from(y));
    }
    Ok(canvas)
}

/// Return a PNG data URL of the selected scene's captured displays.
/// Only integer monitor-list indices are accepted, never arbitrary file paths.
#[tauri::command]
pub async fn capture_scene_snapshot(display_ids: Vec<usize>, layout: Option<String>, transforms: Option<Vec<crate::source_transform::SourceTransform>>) -> Result<String, String> {
    validate_ids(&display_ids)?;
    crate::source_transform::validate_list(transforms.as_deref(),display_ids.len())?;
    let layout = SceneLayout::parse(layout.as_deref())?;
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
        let frame = if let Some(ref items)=transforms { crate::source_transform::composite(&frames,OUTPUT_WIDTH,OUTPUT_HEIGHT,items)? } else { composite_layout(&frames, OUTPUT_WIDTH, OUTPUT_HEIGHT, layout)? };
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
    use super::{validate_ids, SceneLayout};
    #[test]
    fn rejects_invalid_layout() {
        assert_eq!(SceneLayout::parse(None).unwrap(), SceneLayout::Grid);
        assert_eq!(SceneLayout::parse(Some("pip")).unwrap(), SceneLayout::PictureInPicture);
        assert!(SceneLayout::parse(Some("unknown")).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn webcam_layer_is_borrowed_and_visible_in_pip() {
        use super::composite_with_camera;
        use image::{Rgba, RgbaImage};
        let display = RgbaImage::from_pixel(160, 90, Rgba([240,0,0,255]));
        let camera = RgbaImage::from_pixel(64, 48, Rgba([0,240,0,255]));
        let canvas = composite_with_camera(&[display], &camera, 640, 360).unwrap();
        assert_eq!(canvas.get_pixel(100,180)[0], 240);
        assert_eq!(canvas.get_pixel(520,300)[1], 240);
        assert!(composite_with_camera(&[], &camera, 640, 360).is_err());
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn pip_renders_ordered_primary_and_insets_without_distortion() {
        use super::composite_layout;
        use image::{Rgba, RgbaImage};
        let red = RgbaImage::from_pixel(160, 90, Rgba([250,0,0,255]));
        let blue = RgbaImage::from_pixel(160, 90, Rgba([0,0,250,255]));
        let green = RgbaImage::from_pixel(160, 90, Rgba([0,250,0,255]));
        let canvas = composite_layout(&[red, blue, green], 640, 360, SceneLayout::PictureInPicture).unwrap();
        assert_eq!(canvas.get_pixel(100, 180)[0], 250);
        assert_eq!(canvas.get_pixel(520, 300)[2], 250);
        assert_eq!(canvas.get_pixel(520, 180)[1], 250);
        // Exercise smallest allowed dimensions and all four layers.
        let tiny = RgbaImage::from_pixel(1, 1, Rgba([1,2,3,255]));
        assert!(composite_layout(&[tiny.clone(),tiny.clone(),tiny.clone(),tiny.clone()],16,16,SceneLayout::PictureInPicture).is_ok());
        assert!(composite_layout(&[tiny.clone(),tiny.clone(),tiny.clone(),tiny],16,1080,SceneLayout::PictureInPicture).is_ok());
    }


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
