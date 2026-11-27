//! Rejoin a shared encoder at a future IDR, never replay old media.
use crate::stream_mux::Packet;
fn copy(p:&Packet,time:u32)->Packet{Packet{kind:p.kind,timestamp_ms:time,payload:p.payload.clone()}}
#[derive(Default)]
pub(crate) struct Config{video:Option<Packet>,audio:Option<Packet>}
impl Config{
    pub(crate) fn observe(&mut self,batch:&[Packet]){
        for p in batch{
            match (p.kind,p.payload.get(..2)){
                (9,Some([0x17,0]))=>self.video=Some(copy(p,0)),
                (8,Some([0xaf,0]))=>self.audio=Some(copy(p,0)),
                _=>{}
            }
        }
    }
    pub(crate) fn start(&self,batch:&[Packet])->Option<(u32,Vec<Packet>)>{
        let index=batch.iter().position(|p|p.kind==9&&p.payload.get(..2)==Some(&[0x17,1]))?;
        let base=batch[index].timestamp_ms;
        let mut output=vec![copy(self.video.as_ref()?,0)];
        if let Some(a)=&self.audio{output.push(copy(a,0));}
        output.extend(Self::rebase(&batch[index..],base));
        Some((base,output))
    }
    pub(crate) fn rebase(batch:&[Packet],base:u32)->Vec<Packet>{
        batch.iter().filter(|p|p.payload.get(1)==Some(&1)&&p.timestamp_ms>=base)
            .map(|p|copy(p,p.timestamp_ms-base)).collect()
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    use crate::{stream_mux::{avc_sequence,avc_access_unit,aac_sequence,aac_access_unit},live_media_queue::MediaQueue};
    #[test]fn rejoin_waits_for_idr_and_drops_older_audio(){
        let mut c=Config::default();
        c.observe(&[avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap(),aac_sequence()]);
        assert!(c.start(&[avc_access_unit(&[&[0x41,2]],1900,0,false).unwrap()]).is_none());
        let (base,p)=c.start(&[avc_access_unit(&[&[0x65,1]],2000,0,true).unwrap(),aac_access_unit(&[1],1980).unwrap(),aac_access_unit(&[2],2010).unwrap()]).unwrap();
        assert_eq!(base,2000);assert_eq!(p.len(),4);
        assert_eq!(p.iter().map(|p|p.timestamp_ms).collect::<Vec<_>>(),vec![0,0,0,10]);
        let mut q=MediaQueue::default();for p in p{q.push(p).unwrap();}
        let later=Config::rebase(&[avc_access_unit(&[&[0x41,2]],2100,0,false).unwrap()],base);
        assert_eq!(later[0].timestamp_ms,100);q.push(later.into_iter().next().unwrap()).unwrap();
    }
}
