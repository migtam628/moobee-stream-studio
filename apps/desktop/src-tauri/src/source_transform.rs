//! Validated normalized source geometry for preview and output.
//! Coordinates and crop use basis points (10000 = 100%).
use serde::{Deserialize,Serialize};
#[derive(Clone,Copy,Debug,Deserialize,Serialize,PartialEq,Eq)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub(crate) struct SourceTransform {
    pub x:u32,pub y:u32,pub width:u32,pub height:u32,
    pub crop_left:u32,pub crop_top:u32,pub crop_right:u32,pub crop_bottom:u32,
    pub rotation:u32,pub fit:bool,
}
impl SourceTransform {
    pub(crate) fn validate(&self)->Result<(),String>{
        if !(250..=10000).contains(&self.width)||!(250..=10000).contains(&self.height)||
           self.x>10000-self.width||self.y>10000-self.height||
           [self.crop_left,self.crop_top,self.crop_right,self.crop_bottom].iter().any(|v|*v>9000)||
           self.crop_left+self.crop_right>9000||self.crop_top+self.crop_bottom>9000||
           ![0,90,180,270].contains(&self.rotation){
            return Err("Invalid source geometry, crop or rotation".into());
        }
        Ok(())
    }
}
pub(crate) fn validate_list(items:Option<&[SourceTransform]>,count:usize)->Result<(),String>{
    if let Some(items)=items {
        if count==0||count>4||items.len()!=count{return Err("One transform required for each enabled source (maximum four)".into())}
        for item in items{item.validate()?}
    }
    Ok(())
}
#[cfg(any(target_os="macos",target_os="windows"))]
pub(crate) fn composite(frames:&[image::RgbaImage],width:u32,height:u32,items:&[SourceTransform])->Result<image::RgbaImage,String>{
    use image::{imageops,Rgba,RgbaImage};
    validate_list(Some(items),frames.len())?;
    if !(16..=1920).contains(&width)||!(16..=1080).contains(&height) {return Err("Invalid transform canvas".into())}
    let mut canvas=RgbaImage::from_pixel(width,height,Rgba([13,15,26,255]));
    for (frame,t) in frames.iter().zip(items){
        let (sw,sh)=frame.dimensions();
        if sw==0||sh==0||u64::from(sw)*u64::from(sh)>36_000_000{return Err("Invalid source frame".into())}
        let scale=|value:u32,total:u32|->u32{(u64::from(value)*u64::from(total)/10000) as u32};
        let l=scale(t.crop_left,sw);let r=scale(t.crop_right,sw);
        let top=scale(t.crop_top,sh);let bottom=scale(t.crop_bottom,sh);
        let cropped=imageops::crop_imm(frame,l,top,(sw-l-r).max(1),(sh-top-bottom).max(1));
        let rotated;
        let source = match t.rotation {
            90=>{rotated=imageops::rotate90(&*cropped); &rotated},
            180=>{rotated=imageops::rotate180(&*cropped); &rotated},
            270=>{rotated=imageops::rotate270(&*cropped); &rotated},
            _=>{rotated=cropped.to_image(); &rotated}
        };
        let x=scale(t.x,width);let y=scale(t.y,height);
        let w=scale(t.width,width).max(1).min(width-x);
        let h=scale(t.height,height).max(1).min(height-y);
        let (rw,rh)=if t.fit {
            let ratio=(f64::from(w)/f64::from(source.width())).min(f64::from(h)/f64::from(source.height()));
            (((f64::from(source.width())*ratio).round() as u32).clamp(1,w),((f64::from(source.height())*ratio).round() as u32).clamp(1,h))
        }else{(w,h)};
        let scaled=imageops::resize(source,rw,rh,imageops::FilterType::Triangle);
        // Black letterboxing is opaque inside the layer rectangle.
        for py in y..y+h{for px in x..x+w{canvas.put_pixel(px,py,Rgba([13,15,26,255]));}}
        imageops::overlay(&mut canvas,&scaled,i64::from(x+(w-rw)/2),i64::from(y+(h-rh)/2));
    }
    Ok(canvas)
}
#[cfg(test)]
mod tests{
    use super::*;
    fn full()->SourceTransform{SourceTransform{x:0,y:0,width:10000,height:10000,crop_left:0,crop_top:0,crop_right:0,crop_bottom:0,rotation:0,fit:true}}
    #[test]fn validates_geometry(){
        assert!(full().validate().is_ok());let mut t=full();t.x=1;assert!(t.validate().is_err());
        let mut t=full();t.crop_left=5000;t.crop_right=5001;assert!(t.validate().is_err());
        let mut t=full();t.rotation=45;assert!(t.validate().is_err());
        assert!(validate_list(Some(&[full()]),2).is_err());
    }
    #[cfg(any(target_os="macos",target_os="windows"))]
    #[test]fn moves_resizes_crops_rotates_and_layers_pixels(){
        use image::{Rgba,RgbaImage};
        let red=RgbaImage::from_pixel(16,16,Rgba([255,0,0,255]));
        let mut pattern=RgbaImage::from_pixel(16,16,Rgba([0,255,0,255]));
        for y in 0..16{for x in 8..16{pattern.put_pixel(x,y,Rgba([0,0,255,255]));}}
        let mut t=full();t.x=5000;t.y=5000;t.width=5000;t.height=5000;t.crop_left=5000;t.rotation=90;t.fit=false;
        let canvas=composite(&[red,pattern],64,64,&[full(),t]).unwrap();
        assert_eq!(canvas.get_pixel(10,10).0,[255,0,0,255]);
        assert_eq!(canvas.get_pixel(48,48).0,[0,0,255,255]);
    }
}
