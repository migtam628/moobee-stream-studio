//! Sender-side diagnostics only. These counters do not establish reception,
//! decoder health, wire throughput or network packet loss.
use serde::Serialize;
use std::collections::VecDeque;
const WINDOW_MS:u64=3000;
const MIN_WINDOW_MS:u64=1000;
const STALE_MS:u64=2000;
const MAX_POINTS:usize=32;
const SAMPLE_INTERVAL_MS:u64=250;
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) enum Health{Unavailable,WarmingUp,Normal,Backpressure,Stalled,Stale,Ended}
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Reading{
    pub health:Health,pub payload_kbps:Option<u64>,pub output_fps_tenths:Option<u64>,
    pub queue_bytes:usize,pub queue_packets:usize,pub late_frames_in_window:Option<u32>,
    pub sample_age_ms:Option<u64>,
}
impl Default for Reading{
    fn default()->Self{Self{health:Health::Unavailable,payload_kbps:None,output_fps_tenths:None,
        queue_bytes:0,queue_packets:0,late_frames_in_window:None,sample_age_ms:None}}
}
#[derive(Clone,Copy,Debug)]
pub(crate) struct Sample{
    pub elapsed_ms:u64,pub frames:u32,pub late_frames:u32,pub sent_bytes:u64,pub sent_packets:u64,
    pub queue_bytes:usize,pub queue_packets:usize,
}
#[derive(Default)]
pub(crate) struct Monitor{points:VecDeque<Sample>,latest:Option<Sample>,last_progress:Option<u64>}
impl Monitor{
    pub(crate) fn observe(&mut self,s:Sample){
        if let Some(p)=self.latest{
            if s.elapsed_ms<p.elapsed_ms||s.frames<p.frames||s.late_frames<p.late_frames||s.sent_bytes<p.sent_bytes||s.sent_packets<p.sent_packets{
                // A new encoding epoch must not produce negative rates.
                *self=Self::default();
            }else if s.sent_packets>p.sent_packets{self.last_progress=Some(s.elapsed_ms);}
        }
        if self.last_progress.is_none(){self.last_progress=Some(s.elapsed_ms);}
        self.latest=Some(s);
        if self.points.back().map_or(true,|p|s.elapsed_ms.saturating_sub(p.elapsed_ms)>=SAMPLE_INTERVAL_MS){self.points.push_back(s);}
        while self.points.len()>MAX_POINTS||self.points.len()>1&&self.points.front().is_some_and(|p|s.elapsed_ms.saturating_sub(p.elapsed_ms)>WINDOW_MS){
            self.points.pop_front();
        }
    }
    pub(crate) fn reading(&self,now:u64,ended:bool)->Reading{
        let Some(s)=self.latest else{return Reading::default()};
        let age=now.saturating_sub(s.elapsed_ms);
        let mut r=Reading{queue_bytes:s.queue_bytes,queue_packets:s.queue_packets,sample_age_ms:Some(age),..Reading::default()};
        if ended{r.health=Health::Ended;return r}
        if age>STALE_MS{r.health=Health::Stale;return r}
        if let Some(p)=self.points.front(){
            let dt=s.elapsed_ms.saturating_sub(p.elapsed_ms);
            if dt>=MIN_WINDOW_MS{
                r.payload_kbps=Some(s.sent_bytes.saturating_sub(p.sent_bytes).saturating_mul(8)/dt);
                r.output_fps_tenths=Some(u64::from(s.frames.saturating_sub(p.frames))*10000/dt);
                r.late_frames_in_window=Some(s.late_frames.saturating_sub(p.late_frames));
            }
        }
        r.health=if s.queue_packets>0&&self.last_progress.is_some_and(|p|now.saturating_sub(p)>=STALE_MS){Health::Stalled}
        else if s.queue_bytes>=4*1024*1024||s.queue_packets>=96{Health::Backpressure}
        else if r.payload_kbps.is_none()||s.sent_packets==0{Health::WarmingUp}
        else{Health::Normal};
        r
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    fn sample(ms:u64,frames:u32,bytes:u64,packets:u64,queue:usize)->Sample{
        Sample{elapsed_ms:ms,frames,late_frames:0,sent_bytes:bytes,sent_packets:packets,queue_bytes:queue,queue_packets:usize::from(queue>0)}
    }
    #[test]fn window_rates_and_freshness_are_measured_not_invented(){
        let mut m=Monitor::default();assert_eq!(m.reading(0,false).health,Health::Unavailable);
        m.observe(sample(0,0,0,0,0));assert_eq!(m.reading(0,false).health,Health::WarmingUp);
        m.observe(sample(1000,30,375000,80,0));
        let r=m.reading(1000,false);assert_eq!(r.payload_kbps,Some(3000));assert_eq!(r.output_fps_tenths,Some(300));assert_eq!(r.health,Health::Normal);
        assert_eq!(m.reading(3100,false).health,Health::Stale);
        assert_eq!(m.reading(3100,true).health,Health::Ended);
    }
    #[test]fn backpressure_stall_and_epoch_reset(){
        let mut m=Monitor::default();m.observe(sample(0,0,0,0,0));m.observe(sample(1000,10,100,1,5*1024*1024));
        assert_eq!(m.reading(1000,false).health,Health::Backpressure);
        m.observe(sample(3000,30,100,1,500));assert_eq!(m.reading(3000,false).health,Health::Stalled);
        m.observe(sample(3100,0,0,0,0));assert_eq!(m.reading(3100,false).health,Health::WarmingUp);
        assert_eq!(m.reading(3100,false).payload_kbps,None);
    }
}
