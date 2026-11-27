//! Sender-side diagnostics; not capture-device timestamps or receiver sync.
use serde::Serialize;
use crate::stream_mux::Packet;

#[derive(Clone,Copy,Debug,Default,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Timing{
    pub video_packets:u64,pub audio_packets:u64,
    pub video_end_ms:Option<u64>,pub audio_end_ms:Option<u64>,
    pub encoded_video_lead_ms:Option<i64>,pub max_abs_encoded_gap_ms:Option<u64>,
    pub pending_audio_frames:Option<u64>,pub pcm_frames_supplied:u64,
    pub windows_missing_source_frames:Option<u64>,
    pub max_encode_ms:u64,
}
impl Timing{
    pub(crate) fn observe(&mut self,batch:&[Packet],fps:u32,audio:bool,pending:u64,elapsed_ms:u64){
        for p in batch{
            match (p.kind,p.payload.get(1)){
                (9,Some(1))=>{self.video_packets+=1;self.video_end_ms=Some(u64::from(p.timestamp_ms)+u64::from(1000/fps));}
                (8,Some(1))=>{self.audio_packets+=1;self.audio_end_ms=Some(u64::from(p.timestamp_ms)+1024*1000/48000);}
                _=>{}
            }
        }
        if let (Some(v),Some(a))=(self.video_end_ms,self.audio_end_ms){
            let gap=v as i64-a as i64;
            self.encoded_video_lead_ms=Some(gap);
            self.max_abs_encoded_gap_ms=Some(self.max_abs_encoded_gap_ms.unwrap_or(0).max(gap.unsigned_abs()));
        }
        self.pending_audio_frames=audio.then_some(pending);
        self.max_encode_ms=self.max_encode_ms.max(elapsed_ms);
    }
}
#[derive(Clone,Copy,Debug,Default,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Resources{
    pub samples:u32,pub sample_elapsed_ms:u64,pub resident_bytes:Option<u64>,
    pub baseline_resident_bytes:Option<u64>,pub peak_resident_bytes:Option<u64>,
    pub cpu_tenths:Option<u32>,pub peak_cpu_tenths:Option<u32>,
}
impl Resources{
    pub(crate) fn observe(&mut self,elapsed_ms:u64,memory:u64,cpu:Option<f32>){
        self.samples=self.samples.saturating_add(1);self.sample_elapsed_ms=elapsed_ms;
        self.resident_bytes=Some(memory);
        self.baseline_resident_bytes.get_or_insert(memory);
        self.peak_resident_bytes=Some(self.peak_resident_bytes.unwrap_or(0).max(memory));
        self.cpu_tenths=cpu.filter(|v|v.is_finite()&&*v>=0.).map(|v|(v*10.).round() as u32);
        if let Some(v)=self.cpu_tenths{self.peak_cpu_tenths=Some(self.peak_cpu_tenths.unwrap_or(0).max(v));}
    }
}
#[cfg(any(target_os="macos",target_os="windows"))]
pub(crate) struct Sampler(std::sync::Arc<std::sync::atomic::AtomicBool>);
#[cfg(any(target_os="macos",target_os="windows"))]
impl Sampler{
    pub(crate) fn start(run:crate::live_lifecycle::Run)->Self{
        use std::{sync::{Arc,atomic::{AtomicBool,Ordering}},time::Duration};
        use sysinfo::{System,ProcessesToUpdate,ProcessRefreshKind,Pid};
        let stop=Arc::new(AtomicBool::new(false));let cancel=stop.clone();
        // Failure to start/read a sampler never fabricates values or stops media.
        let _=std::thread::Builder::new().name("moobee-soak-sampler".into()).spawn(move||{
            let pid=Pid::from_u32(std::process::id());
            let mut system=System::new();
            let mut have_previous=false;
            while !cancel.load(Ordering::Acquire)&&run.native_active(){
                system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]),true,ProcessRefreshKind::nothing().with_cpu().with_memory());
                if let Some(p)=system.process(pid){
                    if !cancel.load(Ordering::Acquire){run.resource_sample(p.memory(),have_previous.then(||p.cpu_usage()));}
                    have_previous=true;
                }else{have_previous=false;}
                for _ in 0..20{
                    if cancel.load(Ordering::Acquire)||!run.native_active(){return}
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        });
        Self(stop)
    }
}
#[cfg(any(target_os="macos",target_os="windows"))]
impl Drop for Sampler{fn drop(&mut self){self.0.store(true,std::sync::atomic::Ordering::Release);}}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn resource_summary_preserves_baseline_peak_and_missing_cpu(){
        let mut r=Resources::default();
        r.observe(0,100,None);assert_eq!(r.cpu_tenths,None);
        r.observe(1000,200,Some(123.4));r.observe(2000,150,Some(50.));
        assert_eq!(r.samples,3);assert_eq!(r.baseline_resident_bytes,Some(100));assert_eq!(r.peak_resident_bytes,Some(200));
        assert_eq!(r.peak_cpu_tenths,Some(1234));assert_eq!(r.cpu_tenths,Some(500));
    }
    #[test]fn packet_timing_does_not_invent_missing_audio(){
        use crate::stream_mux::{avc_access_unit,aac_access_unit};
        let mut t=Timing::default();
        t.observe(&[avc_access_unit(&[&[0x65,1]],100,0,true).unwrap()],10,true,1024,7);
        assert_eq!(t.encoded_video_lead_ms,None);
        t.observe(&[aac_access_unit(&[1,2],100).unwrap()],10,true,512,4);
        assert_eq!(t.encoded_video_lead_ms,Some(79));assert_eq!(t.max_abs_encoded_gap_ms,Some(79));
        assert_eq!(t.max_encode_ms,7);assert_eq!(t.pending_audio_frames,Some(512));
        let mut silent=Timing::default();silent.observe(&[],30,false,0,0);
        assert_eq!(silent.pending_audio_frames,None);
    }
}
