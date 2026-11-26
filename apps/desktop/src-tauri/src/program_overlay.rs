use serde::Deserialize;
const MAX_WIDTH: u32 = 480;
const MAX_HEIGHT: u32 = 270;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Placement { x_percent: u32, y_percent: u32, width_percent: u32 }
impl Placement {
    fn validate(self) -> Result<Self,String> {
        if self.x_percent > 100 || self.y_percent > 100 || !(10..=50).contains(&self.width_percent) {
            return Err("Graphic placement outside supported range".into());
        }
        Ok(self)
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OverlayInput {
    width: u32,
    height: u32,
    rgba_base64: String,
    #[serde(default)]
    placement: Option<Placement>,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) struct PreparedOverlay { image: image::RgbaImage, left: u32, top: u32 }

impl OverlayInput {
    fn expected_bytes(&self) -> Result<usize, String> {
        if self.width == 0 || self.height == 0 || self.width > MAX_WIDTH || self.height > MAX_HEIGHT {
            return Err("Graphic overlay exceeds 480×270 pixel budget".into());
        }
        if let Some(placement) = self.placement { placement.validate()?; }
        let bytes = self.width as usize * self.height as usize * 4;
        if self.rgba_base64.len() != ((bytes + 2) / 3) * 4 {
            return Err("Invalid graphic overlay payload size".into());
        }
        Ok(bytes)
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(crate) fn prepare(self, width: u32, height: u32) -> Result<PreparedOverlay, String> {
        use base64::Engine as _;
        let expected = self.expected_bytes()?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(self.rgba_base64)
            .map_err(|_| "Invalid graphic overlay encoding")?;
        if bytes.len() != expected { return Err("Invalid overlay RGBA length".into()); }
        let image = image::RgbaImage::from_raw(self.width, self.height, bytes).ok_or("Invalid overlay pixels")?;
        prepare_image(&image,width,height,self.placement)
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn prepare_image(graphic: &image::RgbaImage, width:u32, height:u32, placement:Option<Placement>) -> Result<PreparedOverlay,String> {
    if width < 16 || height < 16 || width > 1920 || height > 1080 || graphic.width()==0 || graphic.height()==0 {
        return Err("Invalid overlay canvas".into());
    }
    // Legacy clients retain the original top-left, no-upscale overlay.
    let margin = (height / 40).min(width / 10).max(1);
    let (max_w,max_h,x,y,upscale) = match placement {
        Some(value) => { let value=value.validate()?; (width * value.width_percent / 100,height / 2,value.x_percent,value.y_percent,true) }
        None => (width/3,height/3,0,0,false)
    };
    let limit_w = max_w.max(1).min(width-2*margin);
    let limit_h = max_h.max(1).min(height-2*margin);
    let scale = (f64::from(limit_w)/f64::from(graphic.width())).min(f64::from(limit_h)/f64::from(graphic.height()));
    let scale = if upscale {scale} else {scale.min(1.0)};
    let w = ((f64::from(graphic.width())*scale).round() as u32).clamp(1,limit_w);
    let h = ((f64::from(graphic.height())*scale).round() as u32).clamp(1,limit_h);
    let left = margin + (width-2*margin-w)*x/100;
    let top = margin + (height-2*margin-h)*y/100;
    Ok(PreparedOverlay{image:image::imageops::resize(graphic,w,h,image::imageops::FilterType::Triangle),left,top})
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn apply(canvas: &mut image::RgbaImage, graphic: &PreparedOverlay) {
    // Resize once per capture, not every frame.
    image::imageops::overlay(canvas,&graphic.image,i64::from(graphic.left),i64::from(graphic.top));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_bounds_and_exact_payload() {
        assert!(OverlayInput { width: 481, height: 1, rgba_base64: String::new(), placement: None }.expected_bytes().is_err());
        assert!(OverlayInput { width: 1, height: 1, rgba_base64: String::new(), placement: None }.expected_bytes().is_err());
        assert_eq!(OverlayInput { width: 1, height: 1, rgba_base64: "AAAAAA==".into(), placement: None }.expected_bytes().unwrap(),4);
        assert!(Placement{x_percent:101,y_percent:0,width_percent:25}.validate().is_err());
        assert!(Placement{x_percent:0,y_percent:0,width_percent:51}.validate().is_err());
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn alpha_blends_graphic_into_native_canvas() {
        use image::{RgbaImage,Rgba};
        let mut canvas = RgbaImage::from_pixel(640,360,Rgba([0,0,200,255]));
        let graphic = RgbaImage::from_pixel(16,16,Rgba([200,0,0,128]));
        apply(&mut canvas,&prepare_image(&graphic,640,360,None).unwrap());
        let p = canvas.get_pixel(10,10);
        assert!(p[0] > 90 && p[0] < 110 && p[2] > 90 && p[2] < 110);
        assert_eq!(canvas.get_pixel(100,100)[2],200);
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn position_and_size_move_real_encoded_pixels_inside_canvas() {
        use image::{Rgba,RgbaImage};
        let graphic = RgbaImage::from_pixel(120,60,Rgba([240,0,0,255]));
        let prepared = prepare_image(&graphic,640,360,Some(Placement{x_percent:100,y_percent:100,width_percent:25})).unwrap();
        assert_eq!(prepared.image.dimensions(),(160,80));
        assert_eq!((prepared.left,prepared.top),(471,271));
        let mut canvas = RgbaImage::from_pixel(640,360,Rgba([0,0,200,255]));
        apply(&mut canvas,&prepared);
        assert_eq!(canvas.get_pixel(550,310)[0],240);
        assert_eq!(canvas.get_pixel(10,10)[2],200);
        let p = prepare_image(&graphic,16,16,Some(Placement{x_percent:100,y_percent:100,width_percent:50})).unwrap();
        assert!(p.left+p.image.width()<=16 && p.top+p.image.height()<=16);
    }
}
