//! Test-only receiver for the sender's fmt0/fmt3 subset. No production listener.
use std::io::{Read,Write};
use serde::Serialize;
const STREAM:u32=5;
const MAX_MESSAGE:usize=2*1024*1024;
const MAX_PACKETS:u64=600_000;
type Result<T>=std::result::Result<T,String>;
fn read<const N:usize>(io:&mut impl Read)->Result<[u8;N]>{let mut b=[0;N];io.read_exact(&mut b).map_err(|_|"Receiver read failed")?;Ok(b)}
fn write(io:&mut impl Write,b:&[u8])->Result<()>{io.write_all(b).and_then(|_|io.flush()).map_err(|_|"Receiver write failed".into())}
fn u24(b:&[u8])->u32{(u32::from(b[0])<<16)|(u32::from(b[1])<<8)|u32::from(b[2])}
struct Message{kind:u8,stream:u32,time:u32,body:Vec<u8>}
fn receive(io:&mut impl Read)->Result<Message>{
    let cs=read::<1>(io)?[0];
    if !(2..=63).contains(&cs){return Err("Receiver expects fmt0 header".into())}
    let h=read::<11>(io)?;
    let len=u24(&h[3..6]) as usize;
    if len==0||len>MAX_MESSAGE{return Err("Receiver message size exceeded".into())}
    let short=u24(&h[..3]);let ext=short==0xff_ffff;
    let time=if ext{u32::from_be_bytes(read(io)?)}else{short};
    let mut body=vec![0;len];
    for (i,b) in body.chunks_mut(128).enumerate(){
        if i>0{
            if read::<1>(io)?[0]!=(0xc0|cs){return Err("Invalid continuation".into())}
            if ext&&u32::from_be_bytes(read(io)?)!=time{return Err("Invalid extended timestamp".into())}
        }
        io.read_exact(b).map_err(|_|"Truncated receiver message")?;
    }
    Ok(Message{kind:h[6],stream:u32::from_le_bytes(h[7..11].try_into().unwrap()),time,body})
}
fn text(s:&str)->Vec<u8>{let mut b=vec![2];b.extend((s.len()as u16).to_be_bytes());b.extend(s.as_bytes());b}
fn number(n:f64)->Vec<u8>{let mut b=vec![0];b.extend(n.to_be_bytes());b}
fn command(name:&str,txn:f64)->Vec<u8>{let mut b=text(name);b.extend(number(txn));b}
fn status(code:&str)->Vec<u8>{let mut b=vec![3,0,4];b.extend(b"code");b.extend(text(code));b.extend([0,0,9]);b}
fn reply(io:&mut impl Write,name:&str,txn:f64,stream:u32,arg:Vec<u8>)->Result<()>{
    let mut b=command(name,txn);b.push(5);b.extend(arg);
    // Responses in this subset fit in a single default-size chunk.
    if b.len()>128{return Err("Test reply too large".into())}
    let n=b.len()as u8;let mut out=vec![3,0,0,0,0,0,n,20];out.extend(stream.to_le_bytes());out.extend(b);write(io,&out)
}
fn expect(m:&Message,stream:u32,body:&[u8],prefix:bool)->Result<()>{
    if m.kind!=20||m.stream!=stream||m.time!=0||!(if prefix{m.body.starts_with(body)}else{m.body==body}){
        return Err("Unexpected receiver command".into())
    }Ok(())
}
#[derive(Debug,Default,Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Report{
    pub video_config:bool,pub audio_config:bool,pub video_frames:u64,pub audio_blocks:u64,
    pub idr_frames:u64,pub payload_bytes:u64,pub media_packets:u64,pub last_video_ms:Option<u32>,
    pub last_audio_ms:Option<u32>,pub orderly_close:bool,pub pong_received:bool,
}
impl Report{
    fn media(&mut self,m:Message)->Result<()>{
        if m.stream!=STREAM||self.media_packets>=MAX_PACKETS{return Err("Unexpected stream or packet budget exceeded".into())}
        let b=&m.body;
        match (m.kind,b.get(..2)){
            (9,Some([0x17,0]))=>{
                if self.video_config||m.time!=0||b.len()<16||b[5]!=1||b[9]!=0xff||b[10]!=0xe1{return Err("Invalid AVC config".into())}
                let n=u16::from_be_bytes(b[11..13].try_into().unwrap())as usize;
                let end=13+n;
                if n<4||b.get(13).map(|v|v&31)!=Some(7)||b.get(end)!=Some(&1){return Err("Invalid SPS".into())}
                let p=b.get(end+1..end+3).ok_or("Missing PPS size")?;
                let n=u16::from_be_bytes(p.try_into().unwrap())as usize;
                if n==0||end+3+n!=b.len()||b[end+3]&31!=8{return Err("Invalid PPS".into())}
                self.video_config=true;
            }
            (8,Some([0xaf,0]))=>{
                if self.audio_config||m.time!=0||b.as_slice()!=[0xaf,0,0x11,0x90]{return Err("Invalid AAC config".into())}self.audio_config=true;
            }
            (9,Some([0x17|0x27,1]))=>{
                if !self.video_config||b.len()<10||self.last_video_ms.is_some_and(|t|m.time<=t)||b[2..5]!=[0,0,0]{return Err("Invalid video timing or format".into())}
                let mut pos=5;let mut idr=false;let mut picture=false;let mut count=0;
                while pos<b.len(){
                    let size=b.get(pos..pos+4).ok_or("Truncated NAL size")?;
                    let size=u32::from_be_bytes(size.try_into().unwrap())as usize;pos+=4;
                    let end=pos.checked_add(size).ok_or("NAL overflow")?;
                    let nal=b.get(pos..end).filter(|n|!n.is_empty()).ok_or("Invalid NAL")?;
                    if nal[0]&0x80!=0{return Err("Invalid NAL header".into())}
                    idr|=nal[0]&31==5;picture|=matches!(nal[0]&31,1|5);pos=end;count+=1;
                    if count>128{return Err("Too many NALs".into())}
                }
                if !picture||idr!=(b[0]==0x17)||(self.video_frames==0&&(!idr||m.time!=0)){return Err("Missing or invalid IDR".into())}
                self.video_frames+=1;self.idr_frames+=if idr{1}else{0};self.last_video_ms=Some(m.time);
            }
            (8,Some([0xaf,1]))=>{
                if !self.audio_config||self.video_frames==0||b.len()<3||self.last_audio_ms.is_some_and(|t|m.time<=t){return Err("Invalid AAC timeline".into())}
                if b.len()>=4&&b[2]==0xff&&b[3]&0xf6==0xf0{return Err("ADTS is not raw AAC".into())}
                self.audio_blocks+=1;self.last_audio_ms=Some(m.time);
            }
            _=>return Err("Unsupported media".into())
        }
        self.media_packets+=1;self.payload_bytes+=b.len()as u64;Ok(())
    }
}
// Optional bounded receiver-side FLV evidence; synthetic codec QA only.
const MAX_CAPTURE:usize=16*1024*1024;
struct Capture{bytes:Vec<u8>}
impl Capture{
    fn new()->Self{Self{bytes:b"FLV\x01\x05\x00\x00\x00\x09\x00\x00\x00\x00".to_vec()}}
    fn append(&mut self,m:&Message)->Result<()>{
        let n=m.body.len();if self.bytes.len()+15+n>MAX_CAPTURE{return Err("Receiver capture budget exceeded".into())}
        let n=n as u32;
        self.bytes.push(m.kind);
        self.bytes.extend(&n.to_be_bytes()[1..]);
        self.bytes.extend(&m.time.to_be_bytes()[1..]);self.bytes.push((m.time>>24)as u8);
        self.bytes.extend([0,0,0]);self.bytes.extend(&m.body);self.bytes.extend((11+n).to_be_bytes());Ok(())
    }
    fn save(mut self,path:&std::path::Path,audio:bool)->Result<()>{
        self.bytes[4]=if audio{5}else{1};
        let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(path).map_err(|_|"Could not create receiver evidence file")?;
        if file.write_all(&self.bytes).and_then(|_|file.sync_all()).is_err(){
            drop(file);let _=std::fs::remove_file(path);return Err("Could not write receiver evidence file".into())
        }Ok(())
    }
}
pub(crate) fn serve(io:&mut (impl Read+Write),fixture_key:&str,capture_path:Option<&std::path::Path>)->Result<Report>{
    let c=read::<1537>(io)?;
    if c[0]!=3{return Err("Invalid RTMP client version".into())}
    let mut s=vec![3];let mut s1=[0u8;1536];getrandom::fill(&mut s1[8..]).map_err(|_|"Randomness unavailable")?;
    s1[..4].copy_from_slice(&1u32.to_be_bytes());s.extend(s1);s.extend(&c[1..]);write(io,&s)?;
    let c2=read::<1536>(io)?;
    if c2[..4]!=s1[..4]||c2[8..]!=s1[8..]{return Err("Handshake echo failed".into())}
    let mut connect=command("connect",1.);connect.push(3);
    expect(&receive(io)?,0,&connect,true)?;
    reply(io,"_result",1.,0,status("NetConnection.Connect.Success"))?;
    let mut create=command("createStream",2.);create.push(5);expect(&receive(io)?,0,&create,false)?;
    reply(io,"_result",2.,0,number(f64::from(STREAM)))?;
    let mut publish=command("publish",0.);publish.push(5);publish.extend(text(fixture_key));publish.extend(text("live"));
    expect(&receive(io)?,STREAM,&publish,false)?;
    reply(io,"onStatus",0.,STREAM,status("NetStream.Publish.Start"))?;
    // Exercise the actual sender's inbound TLS polling path.
    write(io,&[2,0,0,0,0,0,6,4,0,0,0,0,0,6,1,2,3,4])?;
    let mut close=command("deleteStream",0.);close.push(5);close.extend(number(f64::from(STREAM)));
    let mut r=Report::default();let mut capture=capture_path.map(|_|Capture::new());
    loop{
        let m=receive(io)?;
        match m.kind{
            8|9=>{if let Some(c)=capture.as_mut(){c.append(&m)?;}r.media(m)?;},
            4 if m.stream==0&&m.body==[0,7,1,2,3,4]&&!r.pong_received=>r.pong_received=true,
            20=>{expect(&m,0,&close,false)?;r.orderly_close=true;break}
            _=>return Err("Unexpected receiver message".into())
        }
    }
    if r.video_frames<2||!r.pong_received||(r.audio_config&&r.audio_blocks==0){return Err("Incomplete receiver acceptance".into())}
    if let (Some(c),Some(path))=(capture,capture_path){c.save(path,r.audio_config)?;}
    Ok(r)
}
#[test]fn independent_capture_flv_tag_framing_and_budget(){
    let mut c=Capture::new();
    let m=Message{kind:9,stream:STREAM,time:0x01020304,body:vec![0x17,1,0,0,0,0,0,0,1,0x65]};
    c.append(&m).unwrap();
    assert_eq!(&c.bytes[..13],b"FLV\x01\x05\x00\x00\x00\x09\x00\x00\x00\x00");
    assert_eq!(&c.bytes[13..24],&[9,0,0,10,2,3,4,1,0,0,0]);
    assert_eq!(&c.bytes[24..34],m.body.as_slice());
    assert_eq!(&c.bytes[34..],&21u32.to_be_bytes());
    c.bytes.resize(MAX_CAPTURE,0);assert!(c.append(&m).is_err());
}
#[test]fn receiver_rejects_missing_configuration_and_invalid_continuations(){
    let mut r=Report::default();assert!(r.media(Message{kind:9,stream:STREAM,time:0,body:vec![0x17,1,0,0,0,0,0,0,1,0x65]}).is_err());
    let mut wire=vec![6,0,0,0,0,0,129,9,5,0,0,0];wire.extend([1;128]);wire.extend([0xc4,1]);
    assert!(receive(&mut wire.as_slice()).is_err());
}
