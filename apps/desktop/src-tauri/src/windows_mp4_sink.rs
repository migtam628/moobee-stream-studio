//! Synchronous Windows Media Foundation H.264/AAC sink with optional 48 kHz stereo PCM.
use std::{ffi::{c_void,CStr},os::{raw::c_char,windows::ffi::OsStrExt},path::{Path,PathBuf}};
unsafe extern "C" {
    fn moobee_win_writer_create(path:*const u16,width:u32,height:u32,fps:u32,bitrate:u32,audio:i32,error:*mut c_char,capacity:i32)->*mut c_void;
    fn moobee_win_writer_append(handle:*mut c_void,nv12:*const u8,len:usize,error:*mut c_char,capacity:i32)->i32;
    fn moobee_win_writer_append_audio(handle:*mut c_void,pcm:*const i16,frames:usize,error:*mut c_char,capacity:i32)->i32;
    fn moobee_win_writer_finish(handle:*mut c_void,error:*mut c_char,capacity:i32)->i32;
    fn moobee_win_writer_destroy(handle:*mut c_void);
    fn moobee_win_disk_space(path:*const u16,bytes:*mut u64)->i32;
}
fn wide(path:&Path)->Result<Vec<u16>,String>{
    let mut v:Vec<u16>=path.as_os_str().encode_wide().collect();
    if v.contains(&0){return Err("Invalid Windows recording path".into())}
    v.push(0);Ok(v)
}
fn check_space(path:&Path,minimum:u64)->Result<(),String>{
    let mut bytes=0;
    if unsafe{moobee_win_disk_space(wide(path)?.as_ptr(),&mut bytes)}!=0{return Err("Unable to determine recording volume free space".into())}
    if bytes<minimum{return Err("Insufficient recording disk space".into())}
    Ok(())
}
fn message(error:&[c_char;512])->String{
    let s=unsafe{CStr::from_ptr(error.as_ptr())}.to_string_lossy();
    if s.is_empty(){"Windows Media Foundation encoder failed".into()}else{s.into_owned()}
}
fn validate_profile(w:u32,h:u32,fps:u32,bitrate:u32)->Result<(),String>{
    if !matches!((w,h,fps),(640,360,10)|(1280,720,15)|(1920,1080,30))||!matches!(bitrate,1500|3000|6000){return Err("Unsupported Windows MP4 profile".into())}
    Ok(())
}
// BT.601 limited-range 4:2:0. Average each 2x2 chroma block; no unbounded queue.
pub(crate) fn rgba_to_nv12(rgba:&[u8],w:usize,h:usize,out:&mut [u8])->Result<(),String>{
    let pixels=w.checked_mul(h).ok_or("Invalid frame dimensions")?;
    if w==0||h==0||w%2!=0||h%2!=0||pixels>1920*1080||rgba.len()!=pixels*4||out.len()!=pixels*3/2{return Err("Invalid NV12 input geometry".into())}
    let clamp=|v:i32|v.clamp(0,255) as u8;
    for y in (0..h).step_by(2){for x in (0..w).step_by(2){
        let(mut rs,mut gs,mut bs)=(0,0,0);
        for dy in 0..2{for dx in 0..2{
            let i=(y+dy)*w+x+dx;let r=i32::from(rgba[i*4]);let g=i32::from(rgba[i*4+1]);let b=i32::from(rgba[i*4+2]);
            out[i]=clamp(((66*r+129*g+25*b+128)>>8)+16);
            rs+=r;gs+=g;bs+=b;
        }}
        let(r,g,b)=((rs+2)/4,(gs+2)/4,(bs+2)/4);
        let uv=pixels+(y/2)*w+x;
        out[uv]=clamp(((-38*r-74*g+112*b+128)>>8)+128);
        out[uv+1]=clamp(((112*r-94*g-18*b+128)>>8)+128);
    }}
    Ok(())
}
pub(crate) fn pcm16(pcm:&[f32])->Result<Vec<i16>,String>{
    if pcm.is_empty()||pcm.len()%2!=0||pcm.len()>9600||pcm.iter().any(|v|!v.is_finite()){return Err("Invalid stereo PCM block".into())}
    Ok(pcm.iter().map(|v|(v.clamp(-1.,1.)*32768.).round().clamp(-32768.,32767.) as i16).collect())
}
pub(crate) struct WindowsMp4Sink{handle:*mut c_void,width:u32,height:u32,fps:u32,count:u32,directory:PathBuf,nv12:Vec<u8>}
impl WindowsMp4Sink{
    pub(crate) fn new(path:&Path,width:u32,height:u32,fps:u32,bitrate:u32)->Result<Self,String>{
        Self::new_with_audio(path,width,height,fps,bitrate,false)
    }
    pub(crate) fn new_with_audio(path:&Path,width:u32,height:u32,fps:u32,bitrate:u32,audio:bool)->Result<Self,String>{
        validate_profile(width,height,fps,bitrate)?;
        let directory=path.parent().ok_or("Missing output folder")?.to_path_buf();
        check_space(&directory,96*1024*1024)?;
        if std::fs::symlink_metadata(path).is_ok(){return Err("Recording path already exists".into())}
        let mut error=[0;512];
        let handle=unsafe{moobee_win_writer_create(wide(path)?.as_ptr(),width,height,fps,bitrate,i32::from(audio),error.as_mut_ptr(),512)};
        if handle.is_null(){return Err(message(&error))}
        Ok(Self{handle,width,height,fps,count:0,directory,nv12:vec![0;(width*height*3/2) as usize]})
    }
    pub(crate) fn append(&mut self,rgba:&[u8])->Result<(),String>{
        if self.count%self.fps==0{check_space(&self.directory,16*1024*1024)?;}
        rgba_to_nv12(rgba,self.width as usize,self.height as usize,&mut self.nv12)?;
        let mut error=[0;512];
        if unsafe{moobee_win_writer_append(self.handle,self.nv12.as_ptr(),self.nv12.len(),error.as_mut_ptr(),512)}!=0{return Err(message(&error))}
        self.count+=1;Ok(())
    }
    pub(crate) fn append_audio(&mut self,pcm:&[f32])->Result<(),String>{
        let pcm=pcm16(pcm)?;
        let mut error=[0;512];
        if unsafe{moobee_win_writer_append_audio(self.handle,pcm.as_ptr(),pcm.len()/2,error.as_mut_ptr(),512)}!=0{return Err(message(&error))}
        Ok(())
    }
    pub(crate) fn finish(&mut self)->Result<(),(String,bool)>{
        let mut error=[0;512];
        if unsafe{moobee_win_writer_finish(self.handle,error.as_mut_ptr(),512)}==0{Ok(())}else{Err((message(&error),false))}
    }
}
impl Drop for WindowsMp4Sink{fn drop(&mut self){unsafe{moobee_win_writer_destroy(self.handle)}}}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn nv12_known_colors_and_geometry(){
        let mut out=[0;6];
        rgba_to_nv12(&[0,0,0,255].repeat(4),2,2,&mut out).unwrap();assert_eq!(out,[16,16,16,16,128,128]);
        rgba_to_nv12(&[255,255,255,255].repeat(4),2,2,&mut out).unwrap();assert_eq!(out,[235,235,235,235,128,128]);
        rgba_to_nv12(&[255,0,0,255].repeat(4),2,2,&mut out).unwrap();assert_eq!(out,[82,82,82,82,90,240]);
        assert!(rgba_to_nv12(&[0;12],3,1,&mut out).is_err());
        assert!(validate_profile(1920,1080,60,6000).is_err());
    }
    #[test]fn pcm_conversion_is_finite_and_saturating(){
        assert_eq!(pcm16(&[-2.,2.,0.,0.5]).unwrap(),vec![-32768,32767,0,16384]);
        assert!(pcm16(&[f32::NAN,0.]).is_err());
        assert!(pcm16(&[0.]).is_err());
    }
    #[test]
    #[ignore="requires Windows Media Foundation AAC and H.264 encoders"]
    fn synthetic_windows_av_finalizes(){
        let dir=std::env::temp_dir().join(format!("moobee-mf-av-{}",uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path=dir.join("synthetic.mp4");
        let mut sink=WindowsMp4Sink::new_with_audio(&path,640,360,10,1500,true).unwrap();
        let clock=crate::media_clock::MediaClock::new(10).unwrap();
        for tick in 0..20{
            let frame=image::RgbaImage::from_pixel(640,360,image::Rgba([20,40,100,255]));
            sink.append(frame.as_raw()).unwrap();
            let pcm=clock.audio_range(tick,48000).unwrap().flat_map(|i|{
                let v=0.15*(i as f32*440.*std::f32::consts::TAU/48000.).sin();[v,v]
            }).collect::<Vec<_>>();
            sink.append_audio(&pcm).unwrap();
        }
        sink.finish().unwrap();drop(sink);
        let(_,duration)=crate::program_mp4_storage::validate_finished(&path).unwrap();
        assert!((duration-2.0).abs()<0.2);
        // Check the actual MP4 stream descriptors; independent decode remains hardware QA.
        let bytes=std::fs::read(&path).unwrap();
        assert!(bytes.windows(4).any(|w|w==b"mp4a"));
        assert!(bytes.windows(4).any(|w|w==b"avc1"));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    #[ignore="requires Windows Media Foundation H.264 encoder; execute separately on Windows"]
    fn synthetic_windows_mp4_finalizes(){
        let dir=std::env::temp_dir().join(format!("moobee-mf-{}",uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path=dir.join("synthetic.mp4");
        let mut sink=WindowsMp4Sink::new(&path,640,360,10,1500).unwrap();
        for tick in 0..20{
            let frame=image::RgbaImage::from_fn(640,360,|x,y|image::Rgba([((x+tick*7)%256)as u8,(y%256)as u8,60,255]));
            sink.append(frame.as_raw()).unwrap();
        }
        sink.finish().unwrap();drop(sink);
        let(_,duration)=crate::program_mp4_storage::validate_finished(&path).unwrap();
        assert!((duration-2.0).abs()<0.2);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
