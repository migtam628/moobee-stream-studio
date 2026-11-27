//! Publishing session over an already-connected, authenticated byte transport.
//! No media can be sent until the explicit server publish confirmation.
use std::io::{Read,Write,ErrorKind};
use crate::live_reconnect::Failure;
use crate::{rtmp_receive::{Action,Negotiation,Phase,Reader},stream_mux::{Packet,rtmp}};
type Result<T>=std::result::Result<T,String>;
const MAX_NEGOTIATION_CHUNKS:usize=8192;
const MAX_NEGOTIATION_BYTES:u64=8*1024*1024;
const OUT_CHUNK:usize=128;
const MAX_INBOUND_BUFFER:usize=2*65_536+64;
const INBOUND_BATCH:usize=16;
pub(crate) struct Publisher<T>{io:T,reader:Reader,state:Negotiation,failed:bool,inbound:Vec<u8>,transport_failure:Option<Failure>}
fn transient_io(kind:ErrorKind)->bool{matches!(kind,ErrorKind::ConnectionReset|ErrorKind::ConnectionAborted|ErrorKind::BrokenPipe|ErrorKind::TimedOut|ErrorKind::ConnectionRefused)}
impl<T:Read+Write> Publisher<T>{
    fn send_raw(&mut self,bytes:&[u8])->Result<()>{
        if self.failed{return Err("RTMP session is closed".into())}
        if let Err(e)=self.io.write_all(bytes).and_then(|_|self.io.flush()){
            self.failed=true;
            if transient_io(e.kind()){self.transport_failure=Some(Failure::TransientTransport);}
            return Err("RTMP transport write failed".into())
        }
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
        let mut s=Self{io,reader:Reader::default(),state:Negotiation::default(),failed:false,inbound:Vec::new(),transport_failure:None};
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
    pub(crate) fn transport_failure(&self)->Option<Failure>{self.transport_failure}
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
    /// Feed decrypted bytes obtained by a nonblocking transport poll. No
    /// parser state is consumed until an entire chunk is buffered.
    pub(crate) fn ingest_inbound(&mut self,bytes:&[u8])->Result<usize>{
        if !self.authorized(){return Err("RTMP session is closed".into())}
        let result=(||{
            if bytes.len()>MAX_INBOUND_BUFFER-self.inbound.len(){return Err("RTMP inbound buffer exceeds limit".into())}
            self.inbound.extend_from_slice(bytes);
            let mut count=0;
            while count<INBOUND_BATCH{
                let Some(n)=self.reader.complete_chunk_len(&self.inbound)?else{break};
                let mut input=&self.inbound[..n];
                let msg=self.reader.next_chunk(&mut input)?;
                if !input.is_empty(){return Err("RTMP chunk framing mismatch".into())}
                self.inbound.drain(..n);
                if let Some(msg)=msg{if let Some(action)=self.state.receive(&msg)?{self.respond(action)?;}}
                if let Some(ack)=self.state.acknowledge(self.reader.bytes()){self.respond(ack)?;}
                count+=1;
            }
            Ok(count)
        })();
        if result.is_err(){self.failed=true;self.inbound.clear();}
        result
    }
    /// Read one inbound chunk while publishing. The owner must continuously
    /// service inbound traffic independently of media pacing; this synchronous
    /// method is intentionally not exposed as a UI command.
    pub(crate) fn service_inbound(&mut self)->Result<()>{
        if self.failed||!self.inbound.is_empty(){return Err("RTMP session cannot mix buffered and direct reads".into())}
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
    pub(crate) fn validate_destination(server_url:&str,stream_key:&str)->Result<()>{
        endpoint(server_url)?;
        rtmp::publish(stream_key)?;
        Ok(())
    }
    /// Explicit future caller only. DNS resolution is OS-blocking and must
    /// be isolated from the UI; socket I/O and negotiation are bounded.
    pub(crate) fn connect_tls(server_url:&str,stream_key:&str)->Result<Publisher<native_tls::TlsStream<DeadlineSocket>>>{
        connect_tls_classified(server_url,stream_key,&mut Failure::Unknown)
    }
    pub(crate) fn connect_tls_classified(server_url:&str,stream_key:&str,failure:&mut Failure)->Result<Publisher<native_tls::TlsStream<DeadlineSocket>>>{
        *failure=Failure::Unknown;
        let target=endpoint(server_url)?;
        rtmp::publish(stream_key)?;
        let addresses=(target.host.as_str(),target.port).to_socket_addrs().map_err(|_|"RTMPS DNS resolution failed")?;
        let deadline=Instant::now()+CONNECT_TIMEOUT;
        let mut socket=None;
        let mut retryable=true;let mut attempted=false;
        for address in addresses.take(8){
            let remaining=deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero(){break}
            attempted=true;
            match TcpStream::connect_timeout(&address,remaining){Ok(s)=>{socket=Some(s);break},Err(e)=>{retryable&=transient_io(e.kind());}}
        }
        let socket=match socket{Some(s)=>s,None=>{if attempted&&retryable{*failure=Failure::TransientTransport;}return Err("RTMPS TCP connection failed".into())}};
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
    impl Publisher<native_tls::TlsStream<DeadlineSocket>>{
        /// Service buffered controls and one small nonblocking TLS read.
        /// A WouldBlock is normal; EOF and other errors terminate the epoch.
        pub(crate) fn poll_tls_inbound(&mut self)->Result<()>{
            if self.ingest_inbound(&[])?==INBOUND_BATCH{return Ok(())}
            let mut bytes=[0u8;8192];
            let room=(MAX_INBOUND_BUFFER-self.inbound.len()).min(bytes.len());
            if room==0{return Err("RTMP inbound buffer exhausted".into())}
            self.io.get_ref().socket.set_nonblocking(true).map_err(|_|"RTMPS poll setup failed")?;
            let read=self.io.read(&mut bytes[..room]);
            self.io.get_ref().socket.set_nonblocking(false).map_err(|_|"RTMPS poll restore failed")?;
            match read{
                Ok(0)=>Err("RTMPS server closed connection".into()),
                Ok(n)=>{self.ingest_inbound(&bytes[..n])?;Ok(())},
                Err(e) if matches!(e.kind(),io::ErrorKind::WouldBlock|io::ErrorKind::TimedOut|io::ErrorKind::Interrupted)=>Ok(()),
                Err(e)=>{if transient_io(e.kind()){self.transport_failure=Some(Failure::TransientTransport);}Err("RTMPS inbound transport failed".into())},
            }
        }
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
    #[test]fn session_owns_authorization_queue_stats_and_stop(){
        use crate::live_session::{Session,Phase};
        let publisher=Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
        let mut s=Session::new(publisher).unwrap();
        assert_eq!(s.stats().phase,Phase::Authorized);
        s.submit(crate::stream_mux::avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap()).unwrap();
        s.submit(crate::stream_mux::avc_access_unit(&[&[0x65,1]],0,0,true).unwrap()).unwrap();
        assert_eq!(s.tick(0).unwrap().sent,2);
        assert_eq!(s.stats().phase,Phase::Sending);
        assert_eq!(s.stats().sent_packets,2);
        s.submit(crate::stream_mux::avc_access_unit(&[&[0x41,2]],100,0,false).unwrap()).unwrap();
        assert_eq!(s.tick(50).unwrap().sent,0);
        s.stop();s.stop();
        assert_eq!(s.stats().phase,Phase::Stopped);
        assert_eq!(s.stats().queued_packets,0);
        assert!(s.tick(100).is_err());
    }
    #[test]fn only_classified_socket_write_failure_is_retryable(){
        use crate::live_session::{Session,Phase};
        use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
        struct Failing{inner:Fixture,fail:Arc<AtomicBool>,kind:ErrorKind}
        impl Read for Failing{fn read(&mut self,b:&mut[u8])->io::Result<usize>{self.inner.read(b)}}
        impl Write for Failing{
            fn write(&mut self,b:&[u8])->io::Result<usize>{
                if self.fail.load(Ordering::Acquire){Err(io::Error::from(self.kind))}else{self.inner.write(b)}
            }
            fn flush(&mut self)->io::Result<()>{Ok(())}
        }
        for (kind,expected) in [(ErrorKind::ConnectionReset,Some(Failure::TransientTransport)),(ErrorKind::InvalidData,None)]{
            let fail=Arc::new(AtomicBool::new(false));
            let p=Publisher::establish(Failing{inner:fixture(true),fail:fail.clone(),kind},"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
            let mut s=Session::new(p).unwrap();
            s.submit(crate::stream_mux::avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap()).unwrap();
            fail.store(true,Ordering::Release);
            assert!(s.tick(0).is_err());
            assert_eq!(s.stats().phase,Phase::Failed);assert_eq!(s.stats().failure,expected);
        }
    }
    #[test]fn session_server_stop_is_terminal(){
        use crate::live_session::{Session,Phase};
        let publisher=Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
        let mut s=Session::new(publisher).unwrap();
        assert!(s.service_inbound().is_err());
        assert_eq!(s.stats().phase,Phase::Failed);
        assert!(s.stats().restart_required);
        assert_eq!(s.stats().failure,None);
        assert!(s.submit(crate::stream_mux::aac_sequence()).is_err());
    }
    #[test]fn worker_delivers_and_stops_without_producer_socket_io(){
        use crate::{live_session::{Session,Phase},live_publish_worker::Worker};
        use std::time::{Instant,Duration};
        let p=Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
        let mut worker=Worker::start(Session::new(p).unwrap()).unwrap();
        worker.submit(crate::stream_mux::avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap()).unwrap();
        worker.submit(crate::stream_mux::avc_access_unit(&[&[0x65,1]],0,0,true).unwrap()).unwrap();
        let deadline=Instant::now()+Duration::from_secs(2);
        while worker.stats().unwrap().session.sent_packets<2{
            assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(5));
        }
        worker.request_stop();
        while !worker.reap().unwrap(){assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(5));}
        assert_eq!(worker.stats().unwrap().session.phase,Phase::Stopped);
        assert!(worker.submit(crate::stream_mux::aac_sequence()).is_err());
    }
    #[test]fn worker_server_stop_and_invalid_mailbox_fail_closed(){
        use crate::{live_session::{Session,Phase},live_publish_worker::Worker};
        use std::time::{Instant,Duration};
        for inbound in [true,false]{
            let p=Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
            let mut worker=Worker::start(Session::new(p).unwrap()).unwrap();
            if inbound{worker.signal_inbound().unwrap();}
            else{let mut packet=crate::stream_mux::aac_sequence();packet.payload.clear();assert!(worker.submit(packet).is_err());}
            let deadline=Instant::now()+Duration::from_secs(2);
            while !worker.reap().unwrap(){assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(5));}
            let s=worker.stats().unwrap();
            assert_eq!(s.session.phase,Phase::Failed);assert_eq!(s.pending_bytes,0);assert!(s.session.restart_required);
        }
    }
    #[test]fn buffered_inbound_survives_fragmentation_and_services_ping(){
        let mut p=Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
        let ping=rtmp::chunks(2,0,4,0,&[0,6,9,8,7,6],128).unwrap();
        let initial=p.io.output.len();
        for b in &ping[..ping.len()-1]{assert_eq!(p.ingest_inbound(&[*b]).unwrap(),0);}
        assert_eq!(p.io.output.len(),initial);
        assert_eq!(p.ingest_inbound(&ping[ping.len()-1..]).unwrap(),1);
        assert!(p.io.output.len()>initial);
        let stop=reply("onStatus",0.,status("NetStream.Publish.Stop"),5);
        assert!(p.ingest_inbound(&stop).is_err());assert!(!p.authorized());
    }
    #[test]fn worker_automatically_polls_and_drains_admitted_media(){
        use crate::{live_session::{Session,Phase},live_publish_worker::Worker};
        use std::{sync::{Arc,atomic::{AtomicUsize,Ordering}},time::{Duration,Instant}};
        let polls=Arc::new(AtomicUsize::new(0));let observed=Arc::clone(&polls);
        let p=Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
        let mut worker=Worker::start_with_poll(Session::new(p).unwrap(),move |_|{
            observed.fetch_add(1,Ordering::Relaxed);Ok(())
        }).unwrap();
        worker.submit_batch(vec![crate::stream_mux::avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap(),crate::stream_mux::avc_access_unit(&[&[0x65,1]],0,0,true).unwrap(),crate::stream_mux::avc_access_unit(&[&[0x41,2]],100,0,false).unwrap()]).unwrap();
        worker.request_drain();
        assert!(worker.submit(crate::stream_mux::aac_sequence()).is_err());
        let deadline=Instant::now()+Duration::from_secs(3);
        while !worker.reap().unwrap(){assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(5));}
        let stats=worker.stats().unwrap();
        assert_eq!(stats.session.phase,Phase::Stopped);assert_eq!(stats.session.sent_packets,3);
        assert_eq!(stats.pending_bytes,0);assert!(polls.load(Ordering::Relaxed)>0);
    }
    #[test]fn automatic_poll_failure_terminates_worker(){
        use crate::{live_session::{Session,Phase},live_publish_worker::Worker};
        use std::time::{Instant,Duration};
        let p=Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
        let mut w=Worker::start_with_poll(Session::new(p).unwrap(),|_|Err("fixture disconnect".into())).unwrap();
        let deadline=Instant::now()+Duration::from_secs(2);
        while !w.reap().unwrap(){assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(5));}
        assert_eq!(w.stats().unwrap().session.phase,Phase::Failed);
    }
    #[cfg(any(target_os="macos",target_os="windows"))]
    #[test]
    #[ignore="requires functioning native AVC/AAC encoders"]
    fn native_program_sink_encodes_and_drains_to_simulated_server(){
        use crate::{live_session::Session,live_publish_worker::Worker,live_program_sink::Sink};
        let p=Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
        let mut sink=Sink::open(640,360,10,1500,true,||Worker::start(Session::new(p).unwrap())).unwrap();
        for i in 0..3{
            sink.append(image::RgbaImage::from_pixel(640,360,image::Rgba([i*20,60,80,255]))).unwrap();
            sink.append_audio(&vec![0.1;9600]).unwrap();
        }
        sink.finish().unwrap();
        assert!(sink.sent_packets()>3);assert!(sink.sent_bytes()>0);
        assert!(sink.append(image::RgbaImage::new(640,360)).is_err());
    }
    #[test]fn rejects_publish_before_media(){
        let result=Publisher::establish(fixture(false),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]);
        let Err(error)=result else{panic!("Rejected publish was accepted")};
        assert!(!error.contains("fixture-key"));
    }
}
