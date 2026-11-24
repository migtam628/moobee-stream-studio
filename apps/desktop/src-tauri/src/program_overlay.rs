use serde::Deserialize;
const MAX_WIDTH: u32 = 480;
const MAX_HEIGHT: u32 = 270;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OverlayInput {
    width: u32,
    height: u32,
    rgba_base64: String,
}
impl OverlayInput {
    fn expected_bytes(&self) -> Result<usize, String> {
        if self.width == 0 || self.height == 0 || self.width > MAX_WIDTH || self.height > MAX_HEIGHT {
            return Err("Graphic overlay exceeds 480×270 pixel budget".into());
        }
        let bytes = self.width as usize * self.height as usize * 4;
        if self.rgba_base64.len() != ((bytes + 2) / 3) * 4 {
            return Err("Invalid graphic overlay payload size".into());
        }
        Ok(bytes)
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(crate) fn decode(self) -> Result<image::RgbaImage, String> {
        use base64::Engine as _;
        let expected = self.expected_bytes()?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(self.rgba_base64)
            .map_err(|_| "Invalid graphic overlay encoding")?;
        if bytes.len() != expected { return Err("Invalid overlay RGBA length".into()); }
        image::RgbaImage::from_raw(self.width, self.height, bytes).ok_or("Invalid overlay pixels".into())
    }
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn apply(canvas: &mut image::RgbaImage, graphic: &image::RgbaImage) {
    let margin = (canvas.height() / 40).min(canvas.width() / 10).max(1);
    let max_w = (canvas.width() / 3).max(1);
    let max_h = (canvas.height() / 3).max(1);
    let scale = 1.0f64.min(f64::from(max_w) / f64::from(graphic.width()))
        .min(f64::from(max_h) / f64::from(graphic.height()));
    let width = (f64::from(graphic.width()) * scale).round().max(1.0) as u32;
    let height = (f64::from(graphic.height()) * scale).round().max(1.0) as u32;
    let fitted = image::imageops::resize(graphic, width, height, image::imageops::FilterType::Triangle);
    image::imageops::overlay(canvas, &fitted, i64::from(margin), i64::from(margin));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_bounds_and_exact_payload() {
        assert!(OverlayInput { width: 481, height: 1, rgba_base64: String::new() }.expected_bytes().is_err());
        assert!(OverlayInput { width: 1, height: 1, rgba_base64: String::new() }.expected_bytes().is_err());
        assert_eq!(OverlayInput { width: 1, height: 1, rgba_base64: "AAAAAA==".into() }.expected_bytes().unwrap(),4);
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn alpha_blends_graphic_into_native_canvas() {
        use image::{RgbaImage,Rgba};
        let mut canvas = RgbaImage::from_pixel(640,360,Rgba([0,0,200,255]));
        let graphic = RgbaImage::from_pixel(16,16,Rgba([200,0,0,128]));
        apply(&mut canvas,&graphic);
        let p = canvas.get_pixel(10,10);
        assert!(p[0] > 90 && p[0] < 110 && p[2] > 90 && p[2] < 110);
        assert_eq!(canvas.get_pixel(100,100)[2],200);
    }
}
