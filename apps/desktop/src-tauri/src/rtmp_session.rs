//! Publishing session over an already-connected, authenticated byte transport.
//! No media can be sent until the explicit server publish confirmation.
use std::io::{Read,Write};
use crate::{rtmp_receive::{Action,Negotiation,Phase,Reader},stream_mux::{Packet,rtmp}};
type Result<T>=std::result::Result<T,String>;
const MAX_NEGOTIATION_CHUNKS:usize=8192;
const MAX_NEGOTIATION_BYTES:u64=8*1024*1024;
const OUT_CHUNK:usize=128;
pub(crate) struct Publisher<T>{io:T,reader:Reader,state:Negotiation,failed:bool}
impl<T:Read+Write> Publisher<T>{
    fn send_raw(&mut self,bytes:&[u8])->Result<()>{
        if self.failed{return Err("RTMP session is closed".into())}
        if self.io.write_all(bytes).and_then(|_|self.io.flush()).is_err(){self.failed=true;return Err("RTMP transport write failed".into())}
        Ok(())
    }
    fn command(&mut self,stream:u32,payload:&[u8])->Result<()>{self.send_raw(&rtmp::chunks(3,stream,20,0,payload,OUT_CHUNK)?)}
    fn control(&mut self,kind:u8,payload:&[u8])->Result<()>{self.send_raw(&rtmp::chunks(2,0,kind,0,payload,OUT_CHUNK)?)}
    fn respond(&mut self,action:Action)->Result<()>{
        match action{
            Action::Ack(n)=>self.control(3,&n.to_be_bytes()),
            Action::Pong(t)=>{let mut payload=vec![0,7];payload.extend(t);self.control(4,&payload)}
            _=>Err("Unexpected RTMP state transition".into()),
        }
    }
    /// The caller supplies unpredictable handshake bytes and a clock value;
    /// the transport must already enforce finite I/O timeouts.
    pub(crate) fn establish(io:T,app:&str,tc_url:&str,stream_name:&str,time:u32,random:&[u8])->Result<Self>{
        // Validate and build credential-bearing commands before any write.
        let connect=rtmp::connect(app,tc_url,1)?;
        let publish=rtmp::publish(stream_name)?;
        let hello=rtmp::client_hello(time,random)?;
        let mut s=Self{io,reader:Reader::default(),state:Negotiation::default(),failed:false};
        s.send_raw(&hello)?;
        let mut response=[0u8;3073];
        s.io.read_exact(&mut response).map_err(|_|"RTMP handshake read failed")?;
        s.send_raw(&rtmp::server_hello(&hello,&response,time)?)?;
        s.command(0,&connect)?;
        for _ in 0..MAX_NEGOTIATION_CHUNKS{
            let msg=s.reader.next_chunk(&mut s.io)?;
            if s.reader.bytes()>MAX_NEGOTIATION_BYTES{return Err("RTMP negotiation exceeds byte limit".into())}
            let action=match msg{Some(ref m)=>s.state.receive(m)?,None=>None};
            if let Some(ack)=s.state.acknowledge(s.reader.bytes()){s.respond(ack)?;}
            match action{
                Some(Action::CreateStream)=>s.command(0,&rtmp::create_stream(2)?)?,
                Some(Action::Publish(id))=>s.command(id,&publish)?,
                Some(Action::Ready(_))=>return Ok(s),
                Some(other)=>s.respond(other)?,
                None=>{}
            }
        }
        Err("RTMP negotiation exceeded chunk limit".into())
    }
    pub(crate) fn authorized(&self)->bool{!self.failed&&matches!(self.state.phase(),Phase::Live(_))}
    pub(crate) fn send_media(&mut self,packet:&Packet)->Result<()>{
        let Phase::Live(id)=self.state.phase() else{return Err("RTMP publish is not authorized".into())};
        self.send_raw(&rtmp::media(packet,id,OUT_CHUNK)?)
    }
    /// Send at most one validated queued packet. The future scheduler must
    /// service inbound traffic between bounded batches.
    pub(crate) fn send_queued(&mut self,queue:&mut crate::live_media_queue::MediaQueue)->Result<bool>{
        if queue.stats().restart_required{return Err("Media queue requires a fresh publishing session".into())}
        let Some(packet)=queue.pop() else{return Ok(false)};
        if let Err(e)=self.send_media(&packet){queue.fail();return Err(e)}
        Ok(true)
    }
    /// Pump due queued packets after publish authorization. The caller owns
    /// clock scheduling and transport readiness.
    pub(crate) fn pump(&mut self,pacer:&mut crate::live_packet_pacer::Pacer,queue:&mut crate::live_media_queue::MediaQueue,elapsed_ms:u64)->Result<crate::live_packet_pacer::Tick>{
        pacer.tick(queue,elapsed_ms,|packet|self.send_media(packet))
    }
    /// Read one inbound chunk while publishing. The owner must continuously
    /// service inbound traffic independently of media pacing; this synchronous
    /// method is intentionally not exposed as a UI command.
    pub(crate) fn service_inbound(&mut self)->Result<()>{
        if self.failed{return Err("RTMP session is closed".into())}
        let result=(||{
            if let Some(msg)=self.reader.next_chunk(&mut self.io)?{
                if let Some(action)=self.state.receive(&msg)?{self.respond(action)?;}
            }
            if let Some(ack)=self.state.acknowledge(self.reader.bytes()){self.respond(ack)?;}
            Ok(())
        })();
        if result.is_err(){self.failed=true;}
        result
    }
}
// The production adapter uses the OS trust store, certificate and hostname
// validation. It never downgrades to unencrypted RTMP. No UI invokes it yet.
#[cfg(any(target_os="macos",target_os="windows"))]
pub(crate) mod tls {
    use super::*;
    use std::{net::{TcpStream,ToSocketAddrs},time::{Duration,Instant,SystemTime,UNIX_EPOCH},io};
    const CONNECT_TIMEOUT:Duration=Duration::from_secs(5);
    const IO_TIMEOUT:Duration=Duration::from_secs(3);
    const NEGOTIATION_TIMEOUT:Duration=Duration::from_secs(20);
    struct Endpoint{host:String,port:u16,app:String,tc_url:String}
    fn endpoint(value:&str)->Result<Endpoint>{
        if value.len()>2048||value.chars().any(char::is_control){return Err("Invalid RTMPS endpoint".into())}
        let u=url::Url::parse(value).map_err(|_|"Invalid RTMPS endpoint")?;
        if u.scheme()!="rtmps"||!u.username().is_empty()||u.password().is_some()||u.query().is_some()||u.fragment().is_some(){
            return Err("Use a secure RTMPS server URL without credentials, query or fragment".into())
        }
        let host=match u.host(){Some(url::Host::Domain(v)) if !v.is_empty()=>v.to_owned(),_=>return Err("RTMPS requires a certificate-verifiable DNS hostname".into())};
        // RTMPS is a non-special URL scheme: URL may expose IP-looking
        // literals as opaque Domain values. Validate DNS labels ourselves.
        if host.len()>253||host.parse::<std::net::IpAddr>().is_ok()||host.split('.').any(|label|label.is_empty()||label.len()>63||label.starts_with('-')||label.ends_with('-')||!label.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-'))||!host.split('.').last().is_some_and(|label|label.bytes().any(|c|c.is_ascii_alphabetic())){
            return Err("RTMPS requires a certificate-verifiable DNS hostname".into())
        }
        let app=u.path().trim_matches('/');
        if app.is_empty()||app.len()>512||app.split('/').any(|p|p.is_empty()||!p.bytes().all(|c|c.is_ascii_alphanumeric()||b"-_.".contains(&c))){
            return Err("Invalid RTMPS application path".into())
        }
        let port=u.port().unwrap_or(443);
        if port==0{return Err("Invalid RTMPS port".into())}
        let tc_url=format!("rtmps://{}:{}/{}",host,port,app);
        Ok(Endpoint{host,port,app:app.to_owned(),tc_url})
    }
    pub(crate) struct DeadlineSocket{socket:TcpStream,deadline:Option<Instant>}
    impl DeadlineSocket{
        fn timeout(&self)->io::Result<Duration>{
            match self.deadline{
                Some(d)=>{let remaining=d.saturating_duration_since(Instant::now());if remaining.is_zero(){Err(io::Error::new(io::ErrorKind::TimedOut,"RTMPS negotiation deadline"))}else{Ok(remaining.min(IO_TIMEOUT))}},
                None=>Ok(IO_TIMEOUT)
            }
        }
    }
    impl Read for DeadlineSocket{
        fn read(&mut self,out:&mut[u8])->io::Result<usize>{self.socket.set_read_timeout(Some(self.timeout()?))?;self.socket.read(out)}
    }
    impl Write for DeadlineSocket{
        fn write(&mut self,data:&[u8])->io::Result<usize>{self.socket.set_write_timeout(Some(self.timeout()?))?;self.socket.write(data)}
        fn flush(&mut self)->io::Result<()>{self.timeout()?;self.socket.flush()}
    }
    /// Explicit future caller only. DNS resolution is OS-blocking and must
    /// be isolated from the UI; socket I/O and negotiation are bounded.
    pub(crate) fn connect_tls(server_url:&str,stream_key:&str)->Result<Publisher<native_tls::TlsStream<DeadlineSocket>>>{
        let target=endpoint(server_url)?;
        rtmp::publish(stream_key)?;
        let addresses=(target.host.as_str(),target.port).to_socket_addrs().map_err(|_|"RTMPS DNS resolution failed")?;
        let deadline=Instant::now()+CONNECT_TIMEOUT;
        let mut socket=None;
        for address in addresses.take(8){
            let remaining=deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero(){break}
            if let Ok(s)=TcpStream::connect_timeout(&address,remaining){socket=Some(s);break}
        }
        let socket=socket.ok_or("RTMPS TCP connection failed")?;
        socket.set_nodelay(true).map_err(|_|"RTMPS socket configuration failed")?;
        let connector=native_tls::TlsConnector::builder().min_protocol_version(Some(native_tls::Protocol::Tlsv12)).build().map_err(|_|"RTMPS TLS initialization failed")?;
        let tls=connector.connect(&target.host,DeadlineSocket{socket,deadline:Some(Instant::now()+NEGOTIATION_TIMEOUT)})
            .map_err(|_|"RTMPS TLS certificate validation or handshake failed")?;
        let mut random=[0u8;1528];getrandom::fill(&mut random).map_err(|_|"RTMP handshake randomness unavailable")?;
        let now=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_|"System clock invalid")?.as_millis() as u32;
        let mut publisher=Publisher::establish(tls,&target.app,&target.tc_url,stream_key,now,&random)?;
        publisher.io.get_mut().deadline=None;
        Ok(publisher)
    }
    #[cfg(test)]
    mod tests{
        use super::*;
        #[test]fn validates_secure_endpoint_without_leaking_credentials(){
            let a=endpoint("rtmps://ingest.example.invalid:443/live").unwrap();
            assert_eq!((&*a.host,a.port,&*a.app),("ingest.example.invalid",443,"live"));
            for value in ["rtmp://example.invalid/live","rtmps://user:secret@example.invalid/live","rtmps://example.invalid/live?key=secret","rtmps://example.invalid/live#secret","rtmps://127.0.0.1/live","rtmps://2130706433/live","rtmps://bad%20host/live","rtmps://example.invalid/","rtmps://example.invalid/live/%20"]{
                let Err(e)=endpoint(value) else{panic!("Invalid endpoint accepted")};assert!(!e.contains("secret"));
            }
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    use std::io::{self,Cursor};
    struct Fixture{input:Cursor<Vec<u8>>,output:Vec<u8>}
    impl Read for Fixture{fn read(&mut self,b:&mut[u8])->io::Result<usize>{self.input.read(b)}}
    impl Write for Fixture{fn write(&mut self,b:&[u8])->io::Result<usize>{self.output.extend(b);Ok(b.len())}fn flush(&mut self)->io::Result<()>{Ok(())}}
    fn text(s:&str)->Vec<u8>{let mut b=vec![2];b.extend((s.len() as u16).to_be_bytes());b.extend(s.as_bytes());b}
    fn num(n:f64)->Vec<u8>{let mut b=vec![0];b.extend(n.to_be_bytes());b}
    fn status(s:&str)->Vec<u8>{let mut b=vec![3,0,4];b.extend(b"code");b.extend(text(s));b.extend([0,0,9]);b}
    fn reply(name:&str,txn:f64,arg:Vec<u8>,stream:u32)->Vec<u8>{
        let mut b=text(name);b.extend(num(txn));b.push(5);b.extend(arg);rtmp::chunks(3,stream,20,0,&b,128).unwrap()
    }
    fn fixture(accepted:bool)->Fixture{
        let client=rtmp::client_hello(7,&[9;1528]).unwrap();
        let mut server=rtmp::client_hello(8,&[1;1528]).unwrap();server.extend(&client[1..]);
        server.extend(rtmp::chunks(2,0,5,0,&64u32.to_be_bytes(),128).unwrap());
        server.extend(reply("_result",1.,status("NetConnection.Connect.Success"),0));
        server.extend(reply("_result",2.,num(5.),0));
        server.extend(rtmp::chunks(2,0,4,0,&[0,6,1,2,3,4],128).unwrap());
        server.extend(reply("onStatus",0.,status(if accepted{"NetStream.Publish.Start"}else{"NetStream.Publish.BadName"}),5));
        server.extend(reply("onStatus",0.,status("NetStream.Publish.Stop"),5));
        Fixture{input:Cursor::new(server),output:vec![]}
    }
    #[test]fn simulated_end_to_end_publish_and_stop(){
        let mut p=Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
        let mut queue=crate::live_media_queue::MediaQueue::default();
        queue.push(crate::stream_mux::avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap()).unwrap();
        queue.push(crate::stream_mux::aac_sequence()).unwrap();
        queue.push(crate::stream_mux::avc_access_unit(&[&[0x65,1]],0,0,true).unwrap()).unwrap();
        while p.send_queued(&mut queue).unwrap(){}
        assert_eq!(queue.stats().dequeued_packets,3);
        let output=&p.io.output[3073..];
        let mut decoder=Reader::default();let mut input=output;let mut commands=Vec::new();let mut media=false;let mut pong=false;
        while !input.is_empty(){if let Some(m)=decoder.next_chunk(&mut input).unwrap(){
            if m.kind==20{commands.push(m.stream)}
            if m.kind==4{pong=m.body==[0,7,1,2,3,4]}
            if m.kind==8{assert_eq!(m.stream,5);media=true;}
        }}
        assert_eq!(commands,vec![0,0,5]);assert!(pong&&media);
        assert!(p.service_inbound().is_err());assert!(p.send_media(&crate::stream_mux::aac_sequence()).is_err());
    }
    #[test]fn rejects_publish_before_media(){
        let result=Publisher::establish(fixture(false),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]);
        let Err(error)=result else{panic!("Rejected publish was accepted")};
        assert!(!error.contains("fixture-key"));
    }
}
