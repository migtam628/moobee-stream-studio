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


// RTMP wire framing and publish commands. No socket, credentials storage, or
// external endpoint access occurs in this module. Server responses must be
// parsed and authorization confirmed before any future publisher sends media.
pub(crate) mod rtmp {
    const MAX_MESSAGE:usize=0xFF_FFFF;
    const MAX_CHUNK:usize=65_536;
    const HANDSHAKE:usize=1536;
    fn u24(value:u32)->[u8;3]{[(value>>16) as u8,(value>>8) as u8,value as u8]}

    /// Stateless fmt-0 messages with fmt-3 continuation chunks. The chosen
    /// chunk stream ID is bounded to one-byte basic headers (2..63).
    /// Chunk size must match the negotiated outbound size; 128 is the default.
    pub(crate) fn chunks(csid:u8,stream_id:u32,type_id:u8,timestamp:u32,payload:&[u8],chunk_size:usize)->Result<Vec<u8>,String>{
        if !(2..=63).contains(&csid)||!(1..=MAX_CHUNK).contains(&chunk_size)||payload.is_empty()||payload.len()>MAX_MESSAGE{
            return Err("Invalid RTMP message or chunk geometry".into())
        }
        let extended=timestamp>=0xFF_FFFF;
        let count=payload.len().div_ceil(chunk_size);
        let mut out=Vec::with_capacity(payload.len()+12+count*(if extended{5}else{1}));
        for (index,part) in payload.chunks(chunk_size).enumerate(){
            if index==0{
                out.push(csid);
                out.extend_from_slice(&u24(timestamp.min(0xFF_FFFF)));
                out.extend_from_slice(&u24(payload.len() as u32));
                out.push(type_id);
                out.extend_from_slice(&stream_id.to_le_bytes());
            }else{out.push(0xC0|csid);}
            if extended{out.extend_from_slice(&timestamp.to_be_bytes());}
            out.extend_from_slice(part);
        }
        Ok(out)
    }

    /// Basic RTMP v3 handshake only. Random bytes must come from a system
    /// CSPRNG supplied by the eventual transport. No downgrade is attempted.
    pub(crate) fn client_hello(time:u32,random:&[u8])->Result<Vec<u8>,String>{
        if random.len()!=HANDSHAKE-8{return Err("RTMP handshake random block must be 1528 bytes".into())}
        let mut out=Vec::with_capacity(HANDSHAKE+1);
        out.push(3);out.extend_from_slice(&time.to_be_bytes());out.extend_from_slice(&[0;4]);out.extend_from_slice(random);Ok(out)
    }
    /// Validate S0/S1/S2, then echo S1 time and random bytes in C2.
    /// Simple handshake only; signed/complex handshakes are not supported.
    pub(crate) fn server_hello(client:&[u8],server:&[u8],received_time:u32)->Result<Vec<u8>,String>{
        if client.len()!=1+HANDSHAKE||client[0]!=3||server.len()!=1+2*HANDSHAKE||server[0]!=3{
            return Err("Invalid RTMP v3 handshake length or version".into())
        }
        let s1=&server[1..1+HANDSHAKE];let s2=&server[1+HANDSHAKE..];
        if s2[..4]!=client[1..5]||s2[8..]!=client[9..]{
            return Err("RTMP server handshake did not echo client challenge".into())
        }
        let mut c2=s1.to_vec();c2[4..8].copy_from_slice(&received_time.to_be_bytes());Ok(c2)
    }

    // Bounded AMF0 command values used by the eventual publish state machine.
    // Errors never interpolate the values, especially the stream name/key.
    fn short(out:&mut Vec<u8>,value:&str)->Result<(),String>{
        if value.len()>4096||value.chars().any(char::is_control){return Err("Invalid RTMP command text".into())}
        out.extend_from_slice(&(value.len() as u16).to_be_bytes());out.extend_from_slice(value.as_bytes());Ok(())
    }
    fn text(out:&mut Vec<u8>,value:&str)->Result<(),String>{out.push(2);short(out,value)}
    fn number(out:&mut Vec<u8>,value:f64){out.push(0);out.extend_from_slice(&value.to_be_bytes());}
    fn property(out:&mut Vec<u8>,name:&str,value:&str)->Result<(),String>{short(out,name)?;text(out,value)}
    fn command(name:&str,txn:u32)->Result<Vec<u8>,String>{
        let mut out=Vec::new();text(&mut out,name)?;number(&mut out,f64::from(txn));Ok(out)
    }
    pub(crate) fn connect(app:&str,tc_url:&str,txn:u32)->Result<Vec<u8>,String>{
        if app.is_empty()||tc_url.is_empty()||txn==0{return Err("Invalid RTMP connect parameters".into())}
        let mut out=command("connect",txn)?;out.push(3);
        property(&mut out,"app",app)?;property(&mut out,"tcUrl",tc_url)?;
        property(&mut out,"flashVer","MooBee Stream Studio")?;
        short(&mut out,"fpad")?;out.extend_from_slice(&[1,0]);
        short(&mut out,"objectEncoding")?;number(&mut out,0.);
        out.extend_from_slice(&[0,0,9]);Ok(out)
    }
    pub(crate) fn create_stream(txn:u32)->Result<Vec<u8>,String>{
        if txn==0{return Err("Invalid createStream transaction".into())}
        let mut out=command("createStream",txn)?;out.push(5);Ok(out)
    }
    pub(crate) fn publish(stream_name:&str)->Result<Vec<u8>,String>{
        if stream_name.is_empty(){return Err("Missing RTMP stream name".into())}
        let mut out=command("publish",0)?;out.push(5);text(&mut out,stream_name)?;text(&mut out,"live")?;Ok(out)
    }
    // RTMP NetConnection deleteStream releases the assigned message stream.
    // No stream name/key is repeated during shutdown.
    pub(crate) fn delete_stream(id:u32)->Result<Vec<u8>,String>{
        if id==0{return Err("Missing assigned RTMP stream ID".into())}
        let mut out=command("deleteStream",0)?;out.push(5);number(&mut out,f64::from(id));Ok(out)
    }
    pub(crate) fn media(packet:&super::Packet,stream_id:u32,chunk_size:usize)->Result<Vec<u8>,String>{
        if stream_id==0{return Err("RTMP media requires an assigned message stream ID".into())}
        let csid=match packet.kind{8=>4,9=>6,_=>return Err("Invalid RTMP media type".into())};
        chunks(csid,stream_id,packet.kind,packet.timestamp_ms,&packet.payload,chunk_size)
    }
}
#[cfg(test)]
mod tests_rtmp {
    use super::{rtmp::*,aac_sequence,avc_access_unit};
    #[test]fn chunk_headers_and_continuations(){
        let data=chunks(6,0x01020304,9,0x010203,&[1,2,3,4,5],2).unwrap();
        assert_eq!(data,vec![6,1,2,3,0,0,5,9,4,3,2,1,1,2,0xc6,3,4,0xc6,5]);
        let ext=chunks(4,1,8,0x01020304,&[10,20,30],2).unwrap();
        assert_eq!(ext,vec![4,255,255,255,0,0,3,8,1,0,0,0,1,2,3,4,10,20,0xc4,1,2,3,4,30]);
        assert!(chunks(64,1,9,0,&[1],128).is_err());
        assert!(chunks(6,1,9,0,&[1],0).is_err());
        assert!(chunks(6,1,9,0,&[],128).is_err());
    }
    #[test]fn handshake_checks_echo_and_version(){
        let client=client_hello(123,&[0x7f;1528]).unwrap();
        let mut server=client_hello(42,&[0x13;1528]).unwrap();
        server.extend_from_slice(&client[1..]);
        let c2=server_hello(&client,&server,99).unwrap();
        assert_eq!(&c2[..8],&[0,0,0,42,0,0,0,99]);
        assert_eq!(&c2[8..],&[0x13;1528]);
        server[1537+8]^=1;
        assert!(server_hello(&client,&server,0).is_err());
        assert!(client_hello(0,&[]).is_err());
    }
    #[test]fn commands_are_amf0_and_validate_secret_without_echo(){
        let c=connect("live","rtmps://example.invalid/live",1).unwrap();
        assert_eq!(&c[..10],b"\x02\x00\x07connect");
        assert!(c.ends_with(&[0,0,9]));
        let create=create_stream(2).unwrap();assert_eq!(&create[..15],b"\x02\x00\x0ccreateStream");
        assert_eq!(*create.last().unwrap(),5);
        let p=publish("fixture-only").unwrap();assert_eq!(&p[..10],b"\x02\x00\x07publish");assert!(p.ends_with(b"\x02\x00\x04live"));
        let invalid="private-fixture\n";
        assert!(!publish(invalid).unwrap_err().contains("private-fixture"));
        assert!(publish("").is_err());
        assert!(create_stream(0).is_err());
        let close=delete_stream(5).unwrap();assert_eq!(&close[..15],b"\x02\x00\x0cdeleteStream");assert!(close.ends_with(&5f64.to_be_bytes()));assert!(delete_stream(0).is_err());
    }
    #[test]fn existing_avc_aac_packets_route_to_distinct_chunk_streams(){
        let a=media(&aac_sequence(),1,128).unwrap();
        assert_eq!((a[0],a[7]),(4,8));assert_eq!(&a[12..],&[0xaf,0,0x11,0x90]);
        let video=avc_access_unit(&[&[0x65,1]],20,0,true).unwrap();
        let v=media(&video,1,128).unwrap();
        assert_eq!((v[0],v[7]),(6,9));assert_eq!(&v[1..4],&[0,0,20]);
        assert!(media(&video,0,128).is_err());
    }
}
