//! Bounded inbound RTMP chunks and AMF0 publish negotiation. Transport-agnostic;
//! no socket access, credentials persistence or external service calls.
use std::{collections::HashMap,io::Read};
const MAX_MESSAGE:usize=1024*1024;
const MAX_STREAMS:usize=32;
const MAX_TOTAL:usize=4*1024*1024;
const MAX_CHUNK:usize=65_536;
type Result<T>=std::result::Result<T,String>;
#[derive(Clone,Copy)]
struct Header{timestamp:u32,delta:u32,len:usize,kind:u8,stream:u32,extended:bool}
struct Partial{header:Header,data:Vec<u8>}
#[derive(Debug,PartialEq,Eq)]
pub(crate) struct Message{pub kind:u8,pub stream:u32,pub timestamp:u32,pub body:Vec<u8>}
pub(crate) struct Reader{size:usize,headers:HashMap<u32,Header>,pending:HashMap<u32,Partial>,bytes:u64}
impl Default for Reader{fn default()->Self{Self{size:128,headers:HashMap::new(),pending:HashMap::new(),bytes:0}}}
impl Reader{
    pub(crate) fn bytes(&self)->u64{self.bytes}
    fn read<const N:usize>(&mut self,r:&mut impl Read)->Result<[u8;N]>{
        let mut b=[0;N];r.read_exact(&mut b).map_err(|_|"RTMP input ended or timed out")?;self.bytes=self.bytes.saturating_add(N as u64);Ok(b)
    }
    fn u24(&mut self,r:&mut impl Read)->Result<u32>{let [a,b,c]=self.read::<3>(r)?;Ok(u32::from_be_bytes([0,a,b,c]))}
    fn u32(&mut self,r:&mut impl Read)->Result<u32>{Ok(u32::from_be_bytes(self.read(r)?))}
    pub(crate) fn next_chunk(&mut self,r:&mut impl Read)->Result<Option<Message>>{
        let first=self.read::<1>(r)?[0];let fmt=first>>6;
        let csid=match first&63{0=>64+u32::from(self.read::<1>(r)?[0]),1=>64+u32::from(u16::from_le_bytes(self.read(r)?)),id=>u32::from(id)};
        let continuing=self.pending.contains_key(&csid);
        if continuing&&fmt!=3{return Err("RTMP message interrupted without abort".into())}
        let old=self.headers.get(&csid).copied();
        let h=if fmt==3{
            let mut h=old.ok_or("RTMP compressed header without history")?;
            if h.extended{let _=self.u32(r)?;}
            if !continuing{h.timestamp=h.timestamp.wrapping_add(h.delta);}
            h
        }else{
            let ts=self.u24(r)?;
            let mut h=if fmt==0{
                let len=self.u24(r)? as usize;let kind=self.read::<1>(r)?[0];let stream=u32::from_le_bytes(self.read(r)?);
                Header{timestamp:0,delta:0,len,kind,stream,extended:false}
            }else{
                let mut h=old.ok_or("RTMP compressed header without history")?;
                if fmt==1{h.len=self.u24(r)? as usize;h.kind=self.read::<1>(r)?[0];}
                h
            };
            h.extended=ts==0xFF_FFFF;
            let value=if h.extended{self.u32(r)?}else{ts};
            if fmt==0{h.timestamp=value;h.delta=value;}else{h.delta=value;h.timestamp=h.timestamp.wrapping_add(value);}
            h
        };
        if h.len>MAX_MESSAGE{return Err("RTMP inbound message exceeds limit".into())}
        if !continuing{
            if !self.headers.contains_key(&csid)&&self.headers.len()>=MAX_STREAMS{return Err("Too many RTMP chunk streams".into())}
            if self.pending.values().map(|p|p.header.len).sum::<usize>()+h.len>MAX_TOTAL{return Err("RTMP reassembly exceeds limit".into())}
            self.headers.insert(csid,h);self.pending.insert(csid,Partial{header:h,data:Vec::with_capacity(h.len)});
        }
        let p=self.pending.get_mut(&csid).ok_or("RTMP reassembly missing")?;
        let take=(p.header.len-p.data.len()).min(self.size);
        let start=p.data.len();p.data.resize(start+take,0);
        r.read_exact(&mut p.data[start..]).map_err(|_|"RTMP payload ended or timed out")?;
        self.bytes=self.bytes.saturating_add(take as u64);
        if p.data.len()!=p.header.len{return Ok(None)}
        let p=self.pending.remove(&csid).unwrap();
        let msg=Message{kind:p.header.kind,stream:p.header.stream,timestamp:p.header.timestamp,body:p.data};
        match msg.kind{
            1|2 if msg.stream!=0=>return Err("Invalid RTMP control stream".into()),
            1=>{
                let n=u32::from_be_bytes(msg.body.as_slice().try_into().map_err(|_|"Invalid RTMP chunk size")?) as usize;
                if !(1..=MAX_CHUNK).contains(&n){return Err("Unsupported RTMP inbound chunk size".into())}self.size=n;
            }
            2=>{let id=u32::from_be_bytes(msg.body.as_slice().try_into().map_err(|_|"Invalid RTMP abort")?);self.pending.remove(&id);}
            _=>{}
        }
        Ok(Some(msg))
    }
}

#[derive(Debug,PartialEq)]
enum Value{Number(f64),Text(String),Object(Vec<(String,Value)>),Null,Other}
struct Amf<'a>{buf:&'a[u8],pos:usize,nodes:usize}
impl<'a> Amf<'a>{
    fn take(&mut self,n:usize)->Result<&'a[u8]>{let end=self.pos.checked_add(n).ok_or("AMF length overflow")?;let v=self.buf.get(self.pos..end).ok_or("Truncated AMF value")?;self.pos=end;Ok(v)}
    fn byte(&mut self)->Result<u8>{Ok(self.take(1)?[0])}
    fn string(&mut self)->Result<String>{
        let n=u16::from_be_bytes(self.take(2)?.try_into().unwrap()) as usize;
        if n>4096{return Err("AMF text exceeds limit".into())}
        Ok(std::str::from_utf8(self.take(n)?).map_err(|_|"Invalid AMF text")?.to_owned())
    }
    fn value(&mut self,depth:usize)->Result<Value>{
        self.nodes+=1;if depth>8||self.nodes>512{return Err("AMF structure exceeds limit".into())}
        match self.byte()?{
            0=>Ok(Value::Number(f64::from_be_bytes(self.take(8)?.try_into().unwrap()))),
            1=>{self.take(1)?;Ok(Value::Other)}
            2=>Ok(Value::Text(self.string()?)),
            3|8=>{
                if self.buf[self.pos-1]==8{self.take(4)?;}
                let mut fields=Vec::new();
                loop{
                    let key=self.string()?;
                    if key.is_empty()&&self.buf.get(self.pos)==Some(&9){self.pos+=1;break}
                    let val=self.value(depth+1)?;
                    if fields.iter().any(|(k,_)|k==&key){return Err("Duplicate AMF field".into())}
                    fields.push((key,val));
                }
                Ok(Value::Object(fields))
            }
            5|6=>Ok(Value::Null),
            10=>{let n=u32::from_be_bytes(self.take(4)?.try_into().unwrap());if n>512{return Err("AMF array exceeds limit".into())}for _ in 0..n{self.value(depth+1)?;}Ok(Value::Other)}
            11=>{self.take(10)?;Ok(Value::Other)}
            12=>{let n=u32::from_be_bytes(self.take(4)?.try_into().unwrap()) as usize;if n>4096{return Err("AMF long text exceeds limit".into())}self.take(n)?;Ok(Value::Other)}
            _=>Err("Unsupported AMF value".into())
        }
    }
}
fn decode_command(body:&[u8])->Result<Vec<Value>>{
    if body.len()>MAX_MESSAGE{return Err("AMF command exceeds limit".into())}
    let mut a=Amf{buf:body,pos:0,nodes:0};let mut values=Vec::new();
    while a.pos<body.len(){values.push(a.value(0)?);}
    Ok(values)
}
fn code(values:&[Value])->Option<&str>{
    values.iter().skip(2).find_map(|v|if let Value::Object(fields)=v{fields.iter().find_map(|(k,v)|if k=="code"{if let Value::Text(s)=v{Some(s.as_str())}else{None}}else{None})}else{None})
}
#[derive(Debug,PartialEq,Eq,Clone,Copy)]
pub(crate) enum Phase{Connect,CreateStream,Publish(u32),Live(u32),Failed}
pub(crate) struct Negotiation{phase:Phase,window:Option<u32>,ack_at:u64}
#[derive(Debug,PartialEq,Eq)]
pub(crate) enum Action{CreateStream,Publish(u32),Ready(u32),Ack(u32),Pong([u8;4])}
impl Default for Negotiation{fn default()->Self{Self{phase:Phase::Connect,window:None,ack_at:0}}}
impl Negotiation{
    pub(crate) fn phase(&self)->Phase{self.phase}
    pub(crate) fn acknowledge(&mut self,received:u64)->Option<Action>{
        if self.window.is_some_and(|w|received.saturating_sub(self.ack_at)>=u64::from(w)){
            self.ack_at=received;Some(Action::Ack(received as u32))
        }else{None}
    }
    pub(crate) fn receive(&mut self,msg:&Message)->Result<Option<Action>>{
        if self.phase==Phase::Failed{return Err("RTMP negotiation already failed".into())}
        let result=self.receive_inner(msg);
        if result.is_err(){self.phase=Phase::Failed;}
        result
    }
    fn receive_inner(&mut self,msg:&Message)->Result<Option<Action>>{
        if matches!(msg.kind,1..=6)&&msg.stream!=0{return Err("RTMP control used wrong stream".into())}
        match msg.kind{
            5=>{
                let w=u32::from_be_bytes(msg.body.as_slice().try_into().map_err(|_|"Invalid RTMP acknowledgement window")?);
                if w==0{return Err("Invalid RTMP acknowledgement window".into())}
                self.window=Some(w);Ok(None)
            }
            4=>{
                let event=msg.body.get(..2).ok_or("Invalid RTMP user control")?;
                if event==[0,6]{let t=msg.body.get(2..).ok_or("Invalid RTMP ping")?.try_into().map_err(|_|"Invalid RTMP ping")?;Ok(Some(Action::Pong(t)))}else{Ok(None)}
            }
            20|17=>{
                let body=if msg.kind==17{if msg.body.first()!=Some(&0){return Err("Unsupported AMF3 command".into())}&msg.body[1..]}else{&msg.body};
                let v=decode_command(body)?;
                let Some(Value::Text(name))=v.first()else{return Err("Invalid RTMP command".into())};
                let txn=match v.get(1){Some(Value::Number(n))=>*n,_=>return Err("Missing RTMP transaction".into())};
                match (self.phase,name.as_str(),txn){
                    (_, "_error",_)=>Err("RTMP server rejected a command".into()),
                    (Phase::Connect,"_result",1.0) if msg.stream==0=>{
                        if code(&v)!=Some("NetConnection.Connect.Success"){return Err("RTMP connection not authorized".into())}
                        self.phase=Phase::CreateStream;Ok(Some(Action::CreateStream))
                    }
                    (Phase::CreateStream,"_result",2.0) if msg.stream==0=>{
                        let Some(Value::Number(id))=v.get(3)else{return Err("RTMP stream ID missing".into())};
                        if !id.is_finite()||id.fract()!=0.||*id<1.||*id>u32::MAX as f64{return Err("Invalid RTMP stream ID".into())}
                        let id=*id as u32;self.phase=Phase::Publish(id);Ok(Some(Action::Publish(id)))
                    }
                    (Phase::Publish(id),"onStatus",_) if msg.stream==id=>{
                        if code(&v)!=Some("NetStream.Publish.Start"){return Err("RTMP publish was not authorized".into())}
                        self.phase=Phase::Live(id);Ok(Some(Action::Ready(id)))
                    }
                    (Phase::Live(id),"onStatus",_) if msg.stream==id=>{
                        if matches!(code(&v),Some("NetStream.Publish.Stop"|"NetStream.Publish.BadName"|"NetStream.Publish.Denied"|"NetStream.Failed"|"NetStream.Unpublish.Success")){return Err("RTMP publish stopped".into())}
                        Ok(None)
                    }
                    _=>Ok(None)
                }
            }
            _=>Ok(None)
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    use crate::stream_mux::rtmp::chunks;
    fn text(s:&str)->Vec<u8>{let mut v=vec![2];v.extend_from_slice(&(s.len() as u16).to_be_bytes());v.extend_from_slice(s.as_bytes());v}
    fn num(n:f64)->Vec<u8>{let mut v=vec![0];v.extend_from_slice(&n.to_be_bytes());v}
    fn cmd(name:&str,txn:f64,arg:Vec<u8>,stream:u32)->Message{
        let mut body=text(name);body.extend(num(txn));body.push(5);body.extend(arg);Message{kind:20,stream,timestamp:0,body}
    }
    fn status(s:&str)->Vec<u8>{let mut v=vec![3,0,4];v.extend(b"code");v.extend(text(s));v.extend([0,0,9]);v}
    #[test]fn reassembles_interleaved_messages_and_extended_timestamps(){
        let a=chunks(6,1,9,0x01020304,&[9;200],128).unwrap();
        let b=chunks(4,1,8,42,&[1,2,3],128).unwrap();
        let mut wire=a[..144].to_vec();wire.extend(b);wire.extend(&a[144..]);
        let mut r=Reader::default();let mut input=wire.as_slice();
        assert!(r.next_chunk(&mut input).unwrap().is_none());
        assert_eq!(r.next_chunk(&mut input).unwrap().unwrap().body,vec![1,2,3]);
        let m=r.next_chunk(&mut input).unwrap().unwrap();assert_eq!(m.timestamp,0x01020304);assert_eq!(m.body,vec![9;200]);
        assert_eq!(r.bytes(),wire.len() as u64);
    }
    #[test]fn compressed_headers_and_chunk_size(){
        let mut wire=chunks(2,0,1,0,&2u32.to_be_bytes(),128).unwrap();
        wire.extend(chunks(6,1,9,10,&[1,2,3],2).unwrap());
        wire.extend([0x86,0,0,5,4,5,0xc6,6]); // fmt2, delta 5, inherited length
        let mut r=Reader::default();let mut input=wire.as_slice();
        assert_eq!(r.next_chunk(&mut input).unwrap().unwrap().kind,1);
        assert!(r.next_chunk(&mut input).unwrap().is_none());
        assert_eq!(r.next_chunk(&mut input).unwrap().unwrap().body,[1,2,3]);
        assert!(r.next_chunk(&mut input).unwrap().is_none());
        let m=r.next_chunk(&mut input).unwrap().unwrap();assert_eq!(m.timestamp,15);assert_eq!(m.body,[4,5,6]);
        assert!(Reader::default().next_chunk(&mut &[0xc6][..]).is_err());
        assert!(Reader::default().next_chunk(&mut &[6,0,0,0,0x20,0,0,9,1,0,0,0][..]).is_err());
    }
    #[test]fn publish_requires_order_and_matching_stream(){
        let mut n=Negotiation::default();
        assert_eq!(n.receive(&cmd("onStatus",0.,status("NetStream.Publish.Start"),1)).unwrap(),None);
        assert_eq!(n.receive(&cmd("_result",1.,status("NetConnection.Connect.Success"),0)).unwrap(),Some(Action::CreateStream));
        assert_eq!(n.receive(&cmd("_result",2.,num(4.),0)).unwrap(),Some(Action::Publish(4)));
        assert_eq!(n.receive(&cmd("onStatus",0.,status("NetStream.Publish.Start"),3)).unwrap(),None);
        assert_eq!(n.receive(&cmd("onStatus",0.,status("NetStream.Publish.Start"),4)).unwrap(),Some(Action::Ready(4)));
        assert_eq!(n.phase(),Phase::Live(4));
        assert!(n.receive(&cmd("onStatus",0.,status("NetStream.Publish.Stop"),4)).is_err());
        assert_eq!(n.phase(),Phase::Failed);
    }
    #[test]fn handles_window_ping_and_rejection_without_exposing_text(){
        let mut n=Negotiation::default();
        assert_eq!(n.receive(&Message{kind:5,stream:0,timestamp:0,body:100u32.to_be_bytes().to_vec()}).unwrap(),None);
        assert_eq!(n.acknowledge(99),None);assert_eq!(n.acknowledge(100),Some(Action::Ack(100)));
        assert_eq!(n.receive(&Message{kind:4,stream:0,timestamp:0,body:vec![0,6,1,2,3,4]}).unwrap(),Some(Action::Pong([1,2,3,4])));
        let e=n.receive(&cmd("_error",1.,status("private-fixture"),0)).unwrap_err();
        assert!(!e.contains("private-fixture"));assert_eq!(n.phase(),Phase::Failed);
    }
}
