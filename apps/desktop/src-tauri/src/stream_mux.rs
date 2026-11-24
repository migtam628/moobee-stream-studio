//! FLV/RTMP media payload building blocks. Accept encoded access units;
//! this module neither opens a socket nor publishes a stream. A live encoder
//! packet source and RTMPS session still need to be integrated.
const MAX_TAG_BYTES: usize = 0xFF_FFFF;
fn be24(value: u32) -> [u8;3] { [(value>>16) as u8,(value>>8) as u8,value as u8] }

#[derive(Debug,PartialEq,Eq)]
pub(crate) struct Packet {
    pub kind: u8,
    pub timestamp_ms: u32,
    pub payload: Vec<u8>,
}
impl Packet {
    /// Serialize the same media message as an FLV tag for offline validation.
    pub(crate) fn flv_tag(&self) -> Result<Vec<u8>,String> {
        if ![8,9].contains(&self.kind) || self.payload.is_empty() || self.payload.len() > MAX_TAG_BYTES {
            return Err("Invalid FLV media packet".into());
        }
        let mut out = Vec::with_capacity(15+self.payload.len());
        out.push(self.kind);
        out.extend_from_slice(&be24(self.payload.len() as u32));
        out.extend_from_slice(&be24(self.timestamp_ms & 0xFF_FFFF));
        out.push((self.timestamp_ms>>24) as u8);
        out.extend_from_slice(&[0,0,0]);
        out.extend_from_slice(&self.payload);
        out.extend_from_slice(&(11+self.payload.len() as u32).to_be_bytes());
        Ok(out)
    }
}
pub(crate) fn avc_sequence(sps: &[u8], pps: &[u8]) -> Result<Packet,String> {
    if sps.len()<4 || sps.len()>u16::MAX as usize || pps.is_empty() || pps.len()>u16::MAX as usize ||
        sps[0]&0x1f != 7 || pps[0]&0x1f != 8 { return Err("Invalid H.264 parameter sets".into()); }
    let mut payload = vec![0x17,0,0,0,0,1,sps[1],sps[2],sps[3],0xff,0xe1];
    payload.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    payload.extend_from_slice(sps);
    payload.push(1);
    payload.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    payload.extend_from_slice(pps);
    Ok(Packet {kind:9,timestamp_ms:0,payload})
}
pub(crate) fn avc_access_unit(nals: &[&[u8]], dts_ms: u32, cts_ms: i32, keyframe: bool) -> Result<Packet,String> {
    if nals.is_empty() || nals.len()>128 || !(-8_388_608..=8_388_607).contains(&cts_ms) {
        return Err("Invalid H.264 access unit timing or NAL count".into());
    }
    let mut payload=vec![if keyframe {0x17}else{0x27},1];
    payload.extend_from_slice(&be24((cts_ms as u32)&0xFF_FFFF));
    for nal in nals {
        if nal.is_empty() || nal.len()>MAX_TAG_BYTES || nal[0]&0x80 != 0 ||
            payload.len()+4+nal.len()>MAX_TAG_BYTES { return Err("Invalid or oversized H.264 NAL".into()); }
        payload.extend_from_slice(&(nal.len() as u32).to_be_bytes());
        payload.extend_from_slice(nal);
    }
    Ok(Packet {kind:9,timestamp_ms:dts_ms,payload})
}
/// AAC-LC, 48 kHz stereo AudioSpecificConfig.
pub(crate) fn aac_sequence() -> Packet { Packet {kind:8,timestamp_ms:0,payload:vec![0xaf,0,0x11,0x90]} }
pub(crate) fn aac_access_unit(raw: &[u8], timestamp_ms: u32) -> Result<Packet,String> {
    if raw.is_empty() || raw.len()>MAX_TAG_BYTES-2 || (raw.len()>=2 && raw[0]==0xff && raw[1]&0xf6==0xf0) {
        return Err("AAC access unit must be raw AAC, not ADTS".into());
    }
    let mut payload=vec![0xaf,1];payload.extend_from_slice(raw);
    Ok(Packet {kind:8,timestamp_ms,payload})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encodes_avc_config_and_signed_composition_time() {
        let config=avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap();
        assert_eq!(&config.payload[..11],&[0x17,0,0,0,0,1,0x64,0,0x28,0xff,0xe1]);
        let nal=avc_access_unit(&[&[0x65,1,2]],0x12345678,-2,true).unwrap();
        assert_eq!(&nal.payload[..9],&[0x17,1,0xff,0xff,0xfe,0,0,0,3]);
        let tag=nal.flv_tag().unwrap();
        assert_eq!(&tag[..8],&[9,0,0,12,0x34,0x56,0x78,0x12]);
        assert_eq!(&tag[tag.len()-4..],&23u32.to_be_bytes());
        assert!(avc_access_unit(&[&[]],0,0,true).is_err());
    }
    #[test]
    fn aac_configuration_and_raw_units() {
        assert_eq!(aac_sequence().payload,vec![0xaf,0,0x11,0x90]);
        assert_eq!(aac_access_unit(&[1,2,3],20).unwrap().payload,vec![0xaf,1,1,2,3]);
        assert!(aac_access_unit(&[0xff,0xf1,0],0).is_err());
    }
}
