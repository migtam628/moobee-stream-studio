//! Explicitly opted-in local test alert compositor. No platform connector.
use crate::AlertEvent;
use font8x8::{UnicodeFonts,BASIC_FONTS};
use image::{Rgba,RgbaImage};
use tokio::sync::broadcast;
const MAX_AGE_MS:u64=5000;
const MAX_DRAIN:usize=16;
struct Active{image:RgbaImage,until:u32}
pub(crate) struct Layer{
    receiver:broadcast::Receiver<AlertEvent>,active:Option<Active>,
    width:u32,height:u32,fps:u32,events:u32,frames:u32,
}
fn rect(image:&mut RgbaImage,x:u32,y:u32,w:u32,h:u32,color:Rgba<u8>){
    for yy in y..y.saturating_add(h).min(image.height()){
        for xx in x..x.saturating_add(w).min(image.width()){image.put_pixel(xx,yy,color);}
    }
}
fn text(image:&mut RgbaImage,line:&str,x:u32,y:u32,scale:u32,max:usize,color:Rgba<u8>){
    for (i,ch) in line.chars().take(max).enumerate(){
        let glyph=BASIC_FONTS.get(ch).or_else(||BASIC_FONTS.get('?')).unwrap_or([0;8]);
        for (row,bits) in glyph.iter().enumerate(){for col in 0..8u32{
            if *bits&(1u8<<col)!=0{rect(image,x+(i as u32*8+col)*scale,y+row as u32*scale,scale,scale,color);}
        }}
    }
}
fn card(event:&AlertEvent,width:u32)->RgbaImage{
    let s=(width/640).clamp(1,3);
    let mut image=RgbaImage::from_pixel(368*s,64*s,Rgba([16,22,36,235]));
    let accent=match event.kind.as_str(){"follow"=>[65,210,177,255],"subscription"=>[179,125,255,255],"tip"=>[255,185,75,255],"raid"=>[104,175,255,255],_=>[155,165,185,255]};
    rect(&mut image,0,0,4*s,64*s,Rgba(accent));
    text(&mut image,&format!("TEST {}",event.kind.to_uppercase()),12*s,8*s,s,42,Rgba(accent));
    text(&mut image,&event.name,12*s,24*s,s,42,Rgba([255,255,255,255]));
    text(&mut image,&event.message,12*s,40*s,s,42,Rgba([207,217,232,255]));
    image
}
impl Layer{
    pub(crate) fn new(receiver:broadcast::Receiver<AlertEvent>,width:u32,height:u32,fps:u32)->Self{
        Self{receiver,active:None,width,height,fps,events:0,frames:0}
    }
    pub(crate) fn apply(&mut self,canvas:&mut RgbaImage,tick:u32,now_ms:u64){
        let mut newest=None;
        for _ in 0..MAX_DRAIN{
            match self.receiver.try_recv(){
                Ok(event) if event.time<=now_ms&&now_ms-event.time<=MAX_AGE_MS=>newest=Some(event),
                Ok(_)|Err(broadcast::error::TryRecvError::Lagged(_))=>continue,
                Err(_)=>break,
            }
        }
        if let Some(event)=newest{
            self.active=Some(Active{image:card(&event,self.width),until:tick.saturating_add(self.fps*3)});
            self.events=self.events.saturating_add(1);
        }
        if self.active.as_ref().is_some_and(|a|tick>=a.until){self.active=None}
        if let Some(a)=&self.active{
            if canvas.dimensions()!=(self.width,self.height)||a.image.width()>self.width||a.image.height()>self.height{return}
            let x=(self.width-a.image.width())/2;
            let y=self.height-a.image.height()-self.height/20;
            image::imageops::overlay(canvas,&a.image,i64::from(x),i64::from(y));
            self.frames=self.frames.saturating_add(1);
        }
    }
    pub(crate) fn counts(&self)->(u32,u32){(self.events,self.frames)}
}
#[cfg(test)]
mod tests{
    use super::*;
    fn event(time:u64)->AlertEvent{AlertEvent{id:"fixture".into(),kind:"follow".into(),name:"Sample".into(),message:"Local test only".into(),time}}
    #[test]fn fresh_event_changes_canvas_and_expires_on_media_clock(){
        let (tx,rx)=broadcast::channel(16);
        let mut layer=Layer::new(rx,640,360,10);
        let blank=RgbaImage::from_pixel(640,360,Rgba([0,0,0,255]));
        let mut frame=blank.clone();tx.send(event(1000)).ok().unwrap();
        layer.apply(&mut frame,0,1000);assert_ne!(frame,blank);assert_eq!(layer.counts(),(1,1));
        let mut frame=blank.clone();layer.apply(&mut frame,29,3900);assert_ne!(frame,blank);
        let mut frame=blank.clone();layer.apply(&mut frame,30,4000);assert_eq!(frame,blank);
        tx.send(event(0)).ok().unwrap();layer.apply(&mut frame,31,10000);assert_eq!(frame,blank);assert_eq!(layer.counts(),(1,2));
    }
    #[test]fn burst_replaces_instead_of_unbounded_layer_accumulation(){
        let (tx,rx)=broadcast::channel(16);let mut l=Layer::new(rx,1280,720,15);
        for i in 0..20{tx.send(event(1000+i)).ok().unwrap();}
        let mut frame=RgbaImage::new(1280,720);l.apply(&mut frame,0,1100);
        assert_eq!(l.counts(),(1,1));
    }
}
