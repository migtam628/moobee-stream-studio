//! Bitrate recovery uses existing native presets at a fresh encoder epoch.
//! Queue observations are sender-side indicators, not available bandwidth.
use serde::Serialize;
const PRESSURE_MS:u64=3000;
const MAX_SAMPLE_GAP_MS:u64=1500;
pub(crate) fn epoch_bitrate(initial:u32,attempt:u8,enabled:bool)->u32{
    if !enabled{return initial}
    match (initial,attempt){(6000,1)=>3000,(6000,2..)|(3000,1..)=>1500,_=>initial}
}
#[derive(Clone,Copy,Debug,Default,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Reading{
    pub initial_kbps:Option<u32>,pub current_kbps:Option<u32>,pub fallback_enabled:bool,
    pub suggested_kbps:Option<u32>,pub pressure_ms:u64,
}
#[derive(Default)]
pub(crate) struct Advisor{reading:Reading,since:std::collections::HashMap<String,u64>,last:Option<u64>}
impl Advisor{
    pub(crate) fn configure(&mut self,initial:u32,current:u32,enabled:bool){
        *self=Self{reading:Reading{initial_kbps:Some(initial),current_kbps:Some(current),fallback_enabled:enabled,..Reading::default()},..Self::default()};
    }
    pub(crate) fn observe(&mut self,now:u64,destinations:&[crate::live_fanout::DestinationStats]){
        let gap=self.last.is_some_and(|t|now<t||now-t>MAX_SAMPLE_GAP_MS);
        self.last=Some(now);
        if gap{self.since.clear();}
        let pressured=|d:&crate::live_fanout::DestinationStats|d.phase=="sending"&&(d.queued_bytes>=4*1024*1024||d.queued_packets>=96);
        self.since.retain(|id,_|destinations.iter().any(|d|d.id==*id&&pressured(d)));
        for d in destinations.iter().filter(|d|pressured(d)).take(3){self.since.entry(d.id.clone()).or_insert(now);}
        self.reading.pressure_ms=self.since.values().map(|s|now.saturating_sub(*s)).max().unwrap_or(0);
        self.reading.suggested_kbps=if self.reading.pressure_ms>=PRESSURE_MS{
            self.reading.current_kbps.map(|v|epoch_bitrate(v,1,true)).filter(|v|Some(*v)!=self.reading.current_kbps)
        }else{None};
    }
    pub(crate) fn reading(&self)->Reading{self.reading}
}
#[cfg(test)]
mod tests{
    use super::*;
    use crate::live_fanout::DestinationStats;
    fn row(phase:&'static str,bytes:usize)->DestinationStats{
        DestinationStats{id:"fixture".into(),phase,sent_packets:0,sent_payload_bytes:0,queued_packets:0,queued_bytes:bytes,admitted_packets:0,admitted_bytes:0,reconnect_attempt:0,interrupted:false}
    }
    #[test]fn fallback_is_opt_in_bounded_and_never_increases(){
        assert_eq!(epoch_bitrate(6000,0,true),6000);
        assert_eq!(epoch_bitrate(6000,1,true),3000);
        assert_eq!(epoch_bitrate(6000,2,true),1500);
        assert_eq!(epoch_bitrate(1500,3,true),1500);
        assert_eq!(epoch_bitrate(6000,3,false),6000);
    }
    #[test]fn pressure_needs_continuous_samples_on_one_active_output(){
        let mut a=Advisor::default();a.configure(6000,6000,true);
        for n in 0..=3{a.observe(n*1000,&[row("sending",4*1024*1024)]);}
        assert_eq!(a.reading().suggested_kbps,Some(3000));
        a.observe(5000,&[row("sending",4*1024*1024)]);assert_eq!(a.reading().suggested_kbps,None);
        // Aggregate pressure across healthy workers does not trigger advice.
        for n in 6..=10{a.observe(n*1000,&[row("sending",3*1024*1024),row("sending",3*1024*1024)]);}
        assert_eq!(a.reading().suggested_kbps,None);
        for n in 11..=14{a.observe(n*1000,&[row("failed",8*1024*1024)]);}
        assert_eq!(a.reading().pressure_ms,0);
        for n in 15..=20{let mut d=row("sending",5*1024*1024);d.id=format!("fixture-{}",n%2);a.observe(n*1000,&[d]);}
        assert_eq!(a.reading().suggested_kbps,None);
    }
}
