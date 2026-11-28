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
pub(crate) type ConnectionPermit=std::sync::Arc<dyn Fn()->bool+Send+Sync>;
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
        let publish=zeroize::Zeroizing::new(rtmp::publish(stream_name)?);
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
                Some(Action::Publish(id))=>{
                    let wire=zeroize::Zeroizing::new(rtmp::chunks(3,id,20,0,&publish,OUT_CHUNK)?);
                    s.send_raw(&wire)?;
                },
                Some(Action::Ready(_))=>return Ok(s),
                Some(other)=>s.respond(other)?,
                None=>{}
            }
        }
        Err("RTMP negotiation exceeded chunk limit".into())
    }
    pub(crate) fn transport_failure(&self)->Option<Failure>{self.transport_failure}
    fn clean_inbound_boundary(&self)->bool{self.inbound.is_empty()&&!self.reader.has_pending_message()}
    pub(crate) fn authorized(&self)->bool{!self.failed&&matches!(self.state.phase(),Phase::Live(_))}
    /// Send a protocol-level end command after draining all admitted media.
    /// Successful local write is not a server acknowledgement.
    pub(crate) fn close_stream(&mut self)->Result<()>{
        if !self.authorized(){return Err("RTMP publish is not authorized".into())}
        let Phase::Live(id)=self.state.phase()else{return Err("No RTMP stream".into())};
        let result=self.command(0,&rtmp::delete_stream(id)?);
        self.failed=true;
        result
    }
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
    use std::{net::{SocketAddr,TcpStream,ToSocketAddrs},time::{Duration,Instant,SystemTime,UNIX_EPOCH},io,sync::{atomic::{AtomicUsize,Ordering},mpsc::{self,Receiver,RecvTimeoutError}}};
    const CONNECT_TIMEOUT:Duration=Duration::from_secs(5);
    const DNS_TIMEOUT:Duration=Duration::from_secs(5);
    const DNS_POLL:Duration=Duration::from_millis(25);
    // An OS resolver cannot be forcibly interrupted. Bound outstanding workers,
    // including timed-out calls, until they actually return. No credentials or
    // socket ownership enter these detached resolver threads.
    const MAX_DNS_WORKERS:usize=4;
    static DNS_WORKERS:AtomicUsize=AtomicUsize::new(0);
    struct DnsSlot;
    impl DnsSlot{
        fn acquire()->Result<Self>{
            DNS_WORKERS.fetch_update(Ordering::AcqRel,Ordering::Acquire,|n|(n<MAX_DNS_WORKERS).then_some(n+1))
                .map(|_|Self).map_err(|_|"RTMPS DNS workers busy".into())
        }
    }
    impl Drop for DnsSlot{fn drop(&mut self){DNS_WORKERS.fetch_sub(1,Ordering::AcqRel);}}
    fn check_permit(permit:Option<&ConnectionPermit>)->Result<()>{
        if permit.is_some_and(|p|!p()){Err("RTMPS negotiation cancelled".into())}else{Ok(())}
    }
    fn wait_dns(rx:Receiver<Result<Vec<SocketAddr>>>,deadline:Instant,permit:Option<&ConnectionPermit>)->Result<Vec<SocketAddr>>{
        loop{
            check_permit(permit)?;
            let remaining=deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero(){return Err("RTMPS DNS resolution deadline".into())}
            match rx.recv_timeout(remaining.min(DNS_POLL)){
                Ok(result)=>{
                    check_permit(permit)?;
                    if Instant::now()>=deadline{return Err("RTMPS DNS resolution deadline".into())}
                    return result
                }
                Err(RecvTimeoutError::Timeout)=>{},
                Err(RecvTimeoutError::Disconnected)=>return Err("RTMPS DNS resolver unavailable".into()),
            }
        }
    }
    fn resolve_bounded(host:String,port:u16,permit:Option<&ConnectionPermit>)->Result<Vec<SocketAddr>>{
        check_permit(permit)?;
        let slot=DnsSlot::acquire()?;
        let deadline=Instant::now()+DNS_TIMEOUT;
        let (tx,rx)=mpsc::sync_channel(1);
        std::thread::Builder::new().name("moobee-rtmps-dns".into()).spawn(move||{
            let _slot=slot;
            let result=(host.as_str(),port).to_socket_addrs()
                .map(|a|a.take(8).collect::<Vec<_>>())
                .map_err(|_|"RTMPS DNS resolution failed".to_owned())
                .and_then(|a|if a.is_empty(){Err("RTMPS DNS returned no addresses".into())}else{Ok(a)});
            let _=tx.send(result);
        }).map_err(|_|"RTMPS DNS resolver unavailable")?;
        wait_dns(rx,deadline,permit)
    }
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
    pub(crate) struct DeadlineSocket{socket:TcpStream,deadline:Option<Instant>,permit:Option<ConnectionPermit>,_lease:Option<std::sync::Arc<crate::stream_destinations::Lease>>}
    impl DeadlineSocket{
        fn timeout(&self)->io::Result<Duration>{
            if self.permit.as_ref().is_some_and(|p|!p()){return Err(io::Error::new(io::ErrorKind::PermissionDenied,"RTMPS negotiation cancelled"))}
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
    // Canonicalize the actual protocol routing fields without DNS or key material.
    pub(crate) fn same_publish_endpoint(a:&str,b:&str)->Result<bool>{
        let a=endpoint(a)?;let b=endpoint(b)?;
        Ok(a.host.eq_ignore_ascii_case(&b.host)&&a.port==b.port&&a.app==b.app)
    }
    pub(crate) fn validate_destination(server_url:&str,stream_key:&str)->Result<()>{
        endpoint(server_url)?;
        let _validated=zeroize::Zeroizing::new(rtmp::publish(stream_key)?);
        Ok(())
    }
    /// Explicit authorized caller only. OS DNS is isolated with bounded wait
    /// and worker count; connection and negotiation have separate deadlines.
    pub(crate) fn connect_tls(server_url:&str,stream_key:&str)->Result<Publisher<native_tls::TlsStream<DeadlineSocket>>>{
        connect_tls_classified(server_url,stream_key,&mut Failure::Unknown)
    }
    pub(crate) fn connect_tls_classified(server_url:&str,stream_key:&str,failure:&mut Failure)->Result<Publisher<native_tls::TlsStream<DeadlineSocket>>>{
        connect_tls_permitted(server_url,stream_key,failure,None,None)
    }
    pub(crate) fn connect_tls_permitted(server_url:&str,stream_key:&str,failure:&mut Failure,permit:Option<ConnectionPermit>,lease:Option<std::sync::Arc<crate::stream_destinations::Lease>>)->Result<Publisher<native_tls::TlsStream<DeadlineSocket>>>{
        *failure=Failure::Unknown;
        let allowed=||if permit.as_ref().is_some_and(|p|!p()){Err("RTMPS negotiation cancelled".to_owned())}else{Ok(())};
        allowed()?;
        let target=endpoint(server_url)?;
        let _validated=zeroize::Zeroizing::new(rtmp::publish(stream_key)?);
        let addresses=resolve_bounded(target.host.clone(),target.port,permit.as_ref())?;
        allowed()?;
        let deadline=Instant::now()+CONNECT_TIMEOUT;
        let mut socket=None;
        let mut retryable=true;let mut attempted=false;
        for address in addresses{
            allowed()?;
            let remaining=deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero(){break}
            attempted=true;
            match TcpStream::connect_timeout(&address,remaining){Ok(s)=>{socket=Some(s);break},Err(e)=>{retryable&=transient_io(e.kind());}}
        }
        let socket=match socket{Some(s)=>s,None=>{if attempted&&retryable{*failure=Failure::TransientTransport;}return Err("RTMPS TCP connection failed".into())}};
        allowed()?;
        socket.set_nodelay(true).map_err(|_|"RTMPS socket configuration failed")?;
        let connector=native_tls::TlsConnector::builder().min_protocol_version(Some(native_tls::Protocol::Tlsv12)).build().map_err(|_|"RTMPS TLS initialization failed")?;
        let tls=connector.connect(&target.host,DeadlineSocket{socket,deadline:Some(Instant::now()+NEGOTIATION_TIMEOUT),permit:permit.clone(),_lease:lease})
            .map_err(|_|"RTMPS TLS certificate validation or handshake failed")?;
        allowed()?;
        let mut random=[0u8;1528];getrandom::fill(&mut random).map_err(|_|"RTMP handshake randomness unavailable")?;
        let now=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_|"System clock invalid")?.as_millis() as u32;
        let mut publisher=Publisher::establish(tls,&target.app,&target.tc_url,stream_key,now,&random)?;
        allowed()?;
        publisher.io.get_mut().deadline=None;
        publisher.io.get_mut().permit=None; // graceful media drain is independent of start cancellation
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
                Ok(0)=>{
                    // EOF after an authorized publish, without a parsed terminal
                    // protocol command, is a transport interruption. A partial
                    // inbound chunk is ambiguous and remains fail-closed.
                    self.transport_failure=if self.clean_inbound_boundary(){Some(Failure::TransientTransport)}else{None};
                    self.failed=true;
                    Err("RTMPS server closed connection".into())
                },
                Ok(n)=>{self.ingest_inbound(&bytes[..n])?;Ok(())},
                Err(e) if matches!(e.kind(),io::ErrorKind::WouldBlock|io::ErrorKind::TimedOut|io::ErrorKind::Interrupted)=>Ok(()),
                Err(e)=>{self.transport_failure=if self.clean_inbound_boundary()&&(transient_io(e.kind())||e.kind()==io::ErrorKind::UnexpectedEof){Some(Failure::TransientTransport)}else{None};self.failed=true;Err("RTMPS inbound transport failed".into())},
            }
        }
    }
    #[cfg(test)]
    mod tests{
        use super::*;
        // Test-only trust anchor and loopback socket. Never installed into
        // system trust, never passed into a production connector.
        fn certificates()->(std::sync::Arc<rustls::ServerConfig>,native_tls::Certificate){
            use rcgen::{CertificateParams,KeyPair,IsCa,BasicConstraints,ExtendedKeyUsagePurpose,KeyUsagePurpose,DnType,date_time_ymd};
            let mut ca=CertificateParams::new(Vec::<String>::new()).unwrap();
            ca.is_ca=IsCa::Ca(BasicConstraints::Unconstrained);
            ca.distinguished_name.push(DnType::CommonName,"MooBee loopback test CA");
            ca.key_usages=vec![KeyUsagePurpose::KeyCertSign,KeyUsagePurpose::CrlSign];
            // Apple trust policy rejects overly long server certificate lifetimes.
            // Derive a short test-only validity window from the runner clock.
            let now=SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let epoch=date_time_ymd(1970,1,1);
            let not_before=epoch+now.saturating_sub(Duration::from_secs(86400));
            let not_after=epoch+now+Duration::from_secs(7*86400);
            ca.not_before=not_before;ca.not_after=not_after;
            let ca_key=KeyPair::generate().unwrap();let ca=ca.self_signed(&ca_key).unwrap();
            let mut leaf=CertificateParams::new(vec!["localhost".to_owned()]).unwrap();
            leaf.distinguished_name.push(DnType::CommonName,"localhost");
            leaf.key_usages=vec![KeyUsagePurpose::DigitalSignature];
            leaf.extended_key_usages=vec![ExtendedKeyUsagePurpose::ServerAuth];
            leaf.not_before=not_before;leaf.not_after=not_after;
            let key=KeyPair::generate().unwrap();let leaf=leaf.signed_by(&key,&ca,&ca_key).unwrap();
            let config=rustls::ServerConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS12]).unwrap().with_no_client_auth()
                .with_single_cert(vec![leaf.der().clone(),ca.der().clone()],rustls::pki_types::PrivatePkcs8KeyDer::from(key.serialize_der()).into()).unwrap();
            (std::sync::Arc::new(config),native_tls::Certificate::from_der(ca.der().as_ref()).unwrap())
        }
        fn loopback(capture_path:Option<std::path::PathBuf>,f:impl FnOnce(&mut crate::live_publish_worker::Worker)->(u64,u64,u64,u64)){
            use std::net::{TcpListener,Ipv4Addr};
            use crate::{live_session::Session,live_publish_worker::Worker};
            let (config,root)=certificates();
            let listener=TcpListener::bind((Ipv4Addr::LOCALHOST,0)).unwrap();
            let addr=listener.local_addr().unwrap();
            let key="loopback-fixture-only";
            let server=std::thread::spawn(move||{
                let (socket,peer)=listener.accept().unwrap();assert!(peer.ip().is_loopback());
                socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut stream=rustls::StreamOwned::new(rustls::ServerConnection::new(config).unwrap(),socket);
                crate::rtmps_test_receiver::serve(&mut stream,key,capture_path.as_deref())
            });
            let socket=TcpStream::connect_timeout(&addr,Duration::from_secs(3)).unwrap();
            socket.set_nodelay(true).unwrap();
            let mut builder=native_tls::TlsConnector::builder();
            builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12)).max_protocol_version(Some(native_tls::Protocol::Tlsv12)).add_root_certificate(root);
            let tls=builder.build().unwrap().connect("localhost",DeadlineSocket{socket,deadline:Some(Instant::now()+NEGOTIATION_TIMEOUT),permit:None,_lease:None}).unwrap_or_else(|e|match e{
                native_tls::HandshakeError::Failure(err)=>panic!("Loopback TLS handshake failed: {err}"),
                native_tls::HandshakeError::WouldBlock(_)=>panic!("Loopback TLS handshake unexpectedly blocked"),
            });
            let mut random=[0u8;1528];getrandom::fill(&mut random).unwrap();
            let mut publisher=Publisher::establish(tls,"live","rtmps://localhost/live",key,0,&random).unwrap();
            publisher.io.get_mut().deadline=None;
            let mut worker=Worker::start_with_poll(Session::new(publisher).unwrap(),|s|s.poll_tls_inbound()).unwrap();
            let (packets,bytes,video,audio)=f(&mut worker);
            worker.request_drain();
            let deadline=Instant::now()+Duration::from_secs(5);
            while !worker.reap().unwrap(){assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(10));}
            let stats=worker.stats().unwrap();assert_eq!(stats.session.phase,crate::live_session::Phase::Stopped);
            let report=server.join().unwrap().unwrap();
            assert_eq!(report.media_packets,packets);assert_eq!(report.payload_bytes,bytes);
            assert_eq!(report.video_frames,video);assert_eq!(report.audio_blocks,audio);
            assert_eq!(stats.session.sent_packets,packets);assert_eq!(stats.session.sent_payload_bytes,bytes);
            assert!(report.orderly_close&&report.pong_received);
            println!("LOOPBACK_RECEIVER_REPORT {}",serde_json::to_string(&report).unwrap());
        }
        #[test]fn authorized_tls_eof_is_retryable_but_explicit_publish_stop_is_not(){
            use std::net::{TcpListener,Ipv4Addr};
            use crate::live_session::{Session,Phase};
            for explicit_stop in [false,true]{
                let (config,root)=certificates();
                let listener=TcpListener::bind((Ipv4Addr::LOCALHOST,0)).unwrap();
                let addr=listener.local_addr().unwrap();
                let (tx,rx)=mpsc::sync_channel(1);
                let server=std::thread::spawn(move||{
                    let (socket,_)=listener.accept().unwrap();
                    socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                    let mut stream=rustls::StreamOwned::new(rustls::ServerConnection::new(config).unwrap(),socket);
                    crate::rtmps_test_receiver::authorize_fixture(&mut stream,"loopback-fixture-only").unwrap();
                    rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    if explicit_stop{crate::rtmps_test_receiver::stop_fixture(&mut stream).unwrap();}
                    stream.conn.send_close_notify();
                    stream.flush().unwrap();
                });
                let socket=TcpStream::connect_timeout(&addr,Duration::from_secs(3)).unwrap();
                let mut builder=native_tls::TlsConnector::builder();
                builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12)).max_protocol_version(Some(native_tls::Protocol::Tlsv12)).add_root_certificate(root);
                let tls=builder.build().unwrap().connect("localhost",DeadlineSocket{socket,deadline:Some(Instant::now()+NEGOTIATION_TIMEOUT),permit:None,_lease:None}).unwrap_or_else(|_|panic!("Fixture TLS handshake failed"));
                let mut publisher=Publisher::establish(tls,"live","rtmps://localhost/live","loopback-fixture-only",0,&[9;1528]).unwrap();
                publisher.io.get_mut().deadline=None;
                let mut session=Session::new(publisher).unwrap();
                tx.send(()).unwrap();
                let deadline=Instant::now()+Duration::from_secs(5);
                loop{
                    if session.poll_tls_inbound().is_err(){break}
                    assert!(Instant::now()<deadline,"Receiver disconnect was not observed");
                    std::thread::sleep(Duration::from_millis(10));
                }
                assert_eq!(session.stats().phase,Phase::Failed);
                assert_eq!(session.stats().failure,if explicit_stop{None}else{Some(Failure::TransientTransport)});
                server.join().unwrap();
            }
        }
        #[test]fn encrypted_fanout_recovers_only_disconnected_receiver_at_fresh_idr(){
            use std::{net::{TcpListener,Ipv4Addr},sync::Arc,thread::JoinHandle};
            use crate::{live_session::Session,live_publish_worker::Worker,live_fanout::{Fanout,Connector},rtmps_test_receiver::Report,stream_mux::{avc_sequence,avc_access_unit}};
            fn client(addr:SocketAddr,root:native_tls::Certificate)->Worker{
                let socket=TcpStream::connect_timeout(&addr,Duration::from_secs(3)).unwrap();
                socket.set_nodelay(true).unwrap();
                let mut builder=native_tls::TlsConnector::builder();
                builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12)).max_protocol_version(Some(native_tls::Protocol::Tlsv12)).add_root_certificate(root);
                let tls=builder.build().unwrap().connect("localhost",DeadlineSocket{socket,deadline:Some(Instant::now()+NEGOTIATION_TIMEOUT),permit:None,_lease:None}).unwrap_or_else(|_|panic!("Fixture TLS handshake failed"));
                let mut publisher=Publisher::establish(tls,"live","rtmps://localhost/live","loopback-fixture-only",0,&[9;1528]).unwrap();
                publisher.io.get_mut().deadline=None;
                Worker::start_with_poll(Session::new(publisher).unwrap(),|s|s.poll_tls_inbound()).unwrap()
            }
            fn open()->(Worker,JoinHandle<Result<Report>>){
                let (config,root)=certificates();
                let listener=TcpListener::bind((Ipv4Addr::LOCALHOST,0)).unwrap();
                let addr=listener.local_addr().unwrap();
                let server=std::thread::spawn(move||{
                    let (socket,_)=listener.accept().unwrap();
                    socket.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
                    socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                    let mut stream=rustls::StreamOwned::new(rustls::ServerConnection::new(config).unwrap(),socket);
                    crate::rtmps_test_receiver::serve(&mut stream,"loopback-fixture-only",None)
                });
                (client(addr,root),server)
            }
            let (healthy,healthy_server)=open();
            let (config,root)=certificates();
            let listener=TcpListener::bind((Ipv4Addr::LOCALHOST,0)).unwrap();
            let addr=listener.local_addr().unwrap();
            let broken_server=std::thread::spawn(move||{
                let (socket,_)=listener.accept().unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut stream=rustls::StreamOwned::new(rustls::ServerConnection::new(config).unwrap(),socket);
                crate::rtmps_test_receiver::authorize_fixture(&mut stream,"loopback-fixture-only").unwrap();
                crate::rtmps_test_receiver::consume_video_start_fixture(&mut stream).unwrap();
                stream.conn.send_close_notify();
                stream.flush().unwrap();
            });
            let broken=client(addr,root);
            let (tx,rx)=mpsc::sync_channel(1);
            let connector:Connector=Arc::new(move|permit|{
                if !permit(){return Err(Failure::Unknown)}
                let (worker,server)=open();
                if !permit(){worker.request_stop();return Err(Failure::Unknown)}
                tx.try_send(server).map_err(|_|Failure::Unknown)?;
                Ok(worker)
            });
            let mut fanout=Fanout::with_recovery(vec![(uuid::Uuid::new_v4().to_string(),healthy,None),(uuid::Uuid::new_v4().to_string(),broken,Some(connector))],None).unwrap();
            let video=|t,key|avc_access_unit(&[&[if key{0x65}else{0x41},1]],t,0,key).unwrap();
            fanout.submit(vec![avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap(),video(0,true)]).unwrap();
            let origin=Instant::now();let until=origin+Duration::from_secs(5);
            while fanout.destinations()[1].phase!="failed"{
                assert!(Instant::now()<until,"Receiver disconnect not observed");
                std::thread::sleep(Duration::from_millis(10));
            }
            broken_server.join().unwrap();
            let mut expected=1u64;
            let mut ts=0;
            while !fanout.test_awaiting_key(1){
                assert!(Instant::now()<until,"Receiver reconnect not observed");
                std::thread::sleep(Duration::from_millis(50));
                ts=(origin.elapsed().as_millis() as u32).max(ts+1);
                fanout.submit(vec![video(ts,false)]).unwrap();
                expected+=1;
            }
            let rejoined=rx.recv_timeout(Duration::from_secs(1)).unwrap();
            assert!(fanout.destinations()[1].interrupted);
            ts+=50;fanout.submit(vec![video(ts,true)]).unwrap();expected+=1;
            ts+=50;fanout.submit(vec![video(ts,false)]).unwrap();expected+=1;
            // The healthy receiver finishes normally; rejoining is not an uninterrupted soak.
            assert!(fanout.finish().is_err());
            let rows=fanout.destinations();
            let healthy_report=healthy_server.join().unwrap().unwrap();
            let rejoined_report=rejoined.join().unwrap().unwrap();
            assert_eq!(healthy_report.video_frames,expected);
            assert_eq!(rejoined_report.video_frames,2);
            assert_eq!(rejoined_report.idr_frames,1);
            assert_eq!(rejoined_report.last_video_ms,Some(50));
            assert!(healthy_report.orderly_close&&rejoined_report.orderly_close);
            assert!(!rows[0].interrupted&&rows[1].interrupted);
            assert_eq!(rows[1].reconnect_attempt,1);
            assert_eq!(rows[0].phase,"stopped");assert_eq!(rows[1].phase,"stopped");
        }
        #[test]fn loopback_tls_rejects_untrusted_root_and_wrong_hostname(){
            use std::net::{TcpListener,Ipv4Addr};
            for (trust,host) in [(false,"localhost"),(true,"wrong.invalid")]{
                let (config,root)=certificates();
                let listener=TcpListener::bind((Ipv4Addr::LOCALHOST,0)).unwrap();
                let addr=listener.local_addr().unwrap();
                let socket=TcpStream::connect_timeout(&addr,Duration::from_secs(3)).unwrap();
                let server=std::thread::spawn(move||{
                    let (mut socket,_)=listener.accept().unwrap();
                    socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                    let mut conn=rustls::ServerConnection::new(config).unwrap();
                    while conn.is_handshaking(){if conn.complete_io(&mut socket).is_err(){break}}
                });
                let mut builder=native_tls::TlsConnector::builder();
                builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12)).max_protocol_version(Some(native_tls::Protocol::Tlsv12));
                if trust{builder.add_root_certificate(root);}
                let result=builder.build().unwrap().connect(host,DeadlineSocket{socket,deadline:Some(Instant::now()+Duration::from_secs(5)),permit:None,_lease:None});
                assert!(result.is_err());drop(result);server.join().unwrap();
            }
        }
        #[test]fn loopback_tls_independent_receiver_accepts_drained_synthetic_packets(){
            use crate::stream_mux::{avc_sequence,avc_access_unit,aac_sequence,aac_access_unit};
            loopback(None,|worker|{
                let mut nal=vec![0x65];nal.extend([0x42;300]);
                let batch=vec![avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap(),aac_sequence(),
                    avc_access_unit(&[&nal],0,0,true).unwrap(),aac_access_unit(&[1,2,3],0).unwrap(),
                    avc_access_unit(&[&[0x41,1]],100,0,false).unwrap(),aac_access_unit(&[4,5],21).unwrap()];
                let bytes=batch.iter().map(|p|p.payload.len()as u64).sum();
                worker.submit_batch(batch).unwrap();
                std::thread::sleep(Duration::from_millis(250));
                (6,bytes,2,2)
            });
        }
        #[test]
        #[ignore="requires working native AVC/AAC encoders; local receiver only"]
        fn loopback_tls_native_encoder_acceptance(){
            let capture=std::env::var_os("MOOBEE_RECEIVER_FLV").map(std::path::PathBuf::from);
            loopback(capture,|worker|{
                let mut encoder=crate::live_program_encoder::Encoder::new(640,360,10,1500,true).unwrap();
                let mut totals=(0u64,0u64,0u64,0u64);
                let start=Instant::now();
                for tick in 0..20u32{
                    let frame=image::RgbaImage::from_pixel(640,360,image::Rgba([tick as u8,50,100,255]));
                    let pcm=(0..4800).flat_map(|i|{let sample=0.2*((tick*4800+i) as f32*440.0*std::f32::consts::TAU/48000.).sin();[sample,sample]}).collect::<Vec<_>>();
                    let batch=encoder.tick(frame.as_raw(),Some(&pcm)).unwrap();
                    for p in &batch{
                        totals.0+=1;totals.1+=p.payload.len()as u64;
                        if p.payload[1]==1{if p.kind==9{totals.2+=1;}else{totals.3+=1;}}
                    }
                    worker.submit_batch(batch).unwrap();
                    std::thread::sleep((start+Duration::from_millis(u64::from(tick+1)*100)).saturating_duration_since(Instant::now()));
                }
                assert_eq!(totals.2,20);assert!(totals.3>80);totals
            });
        }
        #[test]fn dns_wait_deadline_and_revocation_do_not_open_sockets(){
            use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
            let (_tx,rx)=mpsc::sync_channel(1);
            assert_eq!(wait_dns(rx,Instant::now()+Duration::from_millis(30),None).unwrap_err(),"RTMPS DNS resolution deadline");
            let (tx,rx)=mpsc::sync_channel(1);
            let enabled=Arc::new(AtomicBool::new(false));
            let flag=enabled.clone();
            let permit:ConnectionPermit=Arc::new(move||flag.load(Ordering::Acquire));
            tx.send(Ok(vec!["127.0.0.1:443".parse().unwrap()])).unwrap();
            assert_eq!(wait_dns(rx,Instant::now()+Duration::from_secs(1),Some(&permit)).unwrap_err(),"RTMPS negotiation cancelled");
            let (_tx,rx)=mpsc::sync_channel(1);
            enabled.store(true,Ordering::Release);
            let revoke=std::thread::spawn(move||{std::thread::sleep(Duration::from_millis(30));enabled.store(false,Ordering::Release);});
            assert_eq!(wait_dns(rx,Instant::now()+Duration::from_secs(2),Some(&permit)).unwrap_err(),"RTMPS negotiation cancelled");
            revoke.join().unwrap();
        }
        #[test]fn dns_wait_returns_bounded_result_and_channel_failure(){
            let (tx,rx)=mpsc::sync_channel(1);
            let address:SocketAddr="127.0.0.1:443".parse().unwrap();
            tx.send(Ok(vec![address])).unwrap();
            assert_eq!(wait_dns(rx,Instant::now()+Duration::from_secs(1),None).unwrap(),vec![address]);
            let (tx,rx)=mpsc::sync_channel(1);
            drop(tx);
            assert_eq!(wait_dns(rx,Instant::now()+Duration::from_secs(1),None).unwrap_err(),"RTMPS DNS resolver unavailable");
        }
        #[test]fn cancelled_negotiation_never_resolves_or_connects(){
            let permit:ConnectionPermit=std::sync::Arc::new(||false);
            let mut cause=Failure::TransientTransport;
            let err=connect_tls_permitted("rtmps://example.invalid/live","fixture-key",&mut cause,Some(permit),None).err().unwrap();
            assert_eq!(err,"RTMPS negotiation cancelled");assert_eq!(cause,Failure::Unknown);
        }
        #[test]fn revoked_socket_permit_blocks_io(){
            use std::net::{TcpListener,Ipv4Addr};
            use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
            let listener=TcpListener::bind((Ipv4Addr::LOCALHOST,0)).unwrap();
            let socket=TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (_peer,_)=listener.accept().unwrap();
            let enabled=Arc::new(AtomicBool::new(true));let flag=enabled.clone();
            let mut socket=DeadlineSocket{socket,deadline:Some(Instant::now()+Duration::from_secs(5)),permit:Some(Arc::new(move||flag.load(Ordering::Acquire))),_lease:None};
            assert!(socket.timeout().is_ok());enabled.store(false,Ordering::Release);
            assert_eq!(socket.write(&[1]).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
            assert_eq!(socket.read(&mut [0]).unwrap_err().kind(),io::ErrorKind::PermissionDenied);
            assert!(socket.flush().is_err());
        }
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
    #[test]fn orderly_close_sends_delete_stream_and_disables_media(){
        let mut p=Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
        let before=p.io.output.len();
        p.close_stream().unwrap();
        assert_eq!(&p.io.output[before..],&rtmp::chunks(3,0,20,0,&rtmp::delete_stream(5).unwrap(),128).unwrap());
        assert!(!p.authorized());assert!(p.close_stream().is_err());
        assert!(p.send_media(&crate::stream_mux::aac_sequence()).is_err());
    }
    #[test]fn orderly_session_end_requires_empty_media_queue(){
        use crate::live_session::{Session,Phase};
        let p=Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
        let mut s=Session::new(p).unwrap();
        s.submit(crate::stream_mux::avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap()).unwrap();
        s.submit(crate::stream_mux::avc_access_unit(&[&[0x65,1]],0,0,true).unwrap()).unwrap();
        assert!(s.finish_drained().is_err());
        assert_eq!(s.stats().queued_packets,2);
        assert_eq!(s.tick(0).unwrap().sent,2);
        s.finish_drained().unwrap();
        assert_eq!(s.stats().phase,Phase::Stopped);
        assert_eq!(s.stats().sent_packets,2);
        assert!(s.finish_drained().is_err());
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
    #[test]fn incomplete_message_is_not_a_clean_disconnect_boundary(){
        let mut p=Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap();
        assert!(p.clean_inbound_boundary());
        let body=vec![0u8;200];
        let wire=rtmp::chunks(6,0,18,0,&body,128).unwrap();
        assert_eq!(p.ingest_inbound(&wire[..140]).unwrap(),1);
        assert!(p.inbound.is_empty());
        assert!(p.reader.has_pending_message());
        assert!(!p.clean_inbound_boundary());
        assert_eq!(p.ingest_inbound(&wire[140..]).unwrap(),1);
        assert!(p.clean_inbound_boundary());
        assert_eq!(p.ingest_inbound(&wire[..5]).unwrap(),0);
        assert!(!p.clean_inbound_boundary());
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
    #[test]fn fanout_isolates_server_stop_and_drains_surviving_destination(){
        use crate::{live_session::Session,live_publish_worker::Worker,live_fanout::Fanout};
        use std::time::{Duration,Instant};
        let open=||Worker::start(Session::new(Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap()).unwrap()).unwrap();
        let survivor=open();let failing=open();
        let first=uuid::Uuid::new_v4().to_string();let second=uuid::Uuid::new_v4().to_string();
        let mut fanout=Fanout::new(vec![(first.clone(),survivor),(second.clone(),failing)]).unwrap();
        fanout.submit(vec![crate::stream_mux::avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap(),crate::stream_mux::avc_access_unit(&[&[0x65,1]],0,0,true).unwrap()]).unwrap();
        let deadline=Instant::now()+Duration::from_secs(2);
        while fanout.destinations().iter().any(|d|d.sent_packets<2){assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(5));}
        fanout.test_signal_inbound(&second).unwrap();
        while fanout.destinations().iter().find(|d|d.id==second).unwrap().phase!="failed"{assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(5));}
        fanout.submit(vec![crate::stream_mux::avc_access_unit(&[&[0x41,2]],100,0,false).unwrap()]).unwrap();
        assert!(fanout.finish().is_err()); // never report partial success as complete
        let rows=fanout.destinations();
        assert_eq!((rows[0].phase,rows[0].sent_packets),("stopped",3));
        assert_eq!((rows[1].phase,rows[1].sent_packets),("failed",2));
        assert_eq!(fanout.retry_failure(),None);
    }
    #[test]fn fanout_rejoins_only_failed_destination_at_fresh_idr(){
        use crate::{live_session::Session,live_publish_worker::Worker,live_fanout::{Fanout,Connector}};
        use std::{sync::{Arc,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
        struct Fault{inner:Fixture,fail:Arc<AtomicBool>}
        impl Read for Fault{fn read(&mut self,b:&mut[u8])->io::Result<usize>{self.inner.read(b)}}
        impl Write for Fault{
            fn write(&mut self,b:&[u8])->io::Result<usize>{
                if self.fail.load(Ordering::Acquire){Err(io::Error::from(ErrorKind::ConnectionReset))}else{self.inner.write(b)}
            }
            fn flush(&mut self)->io::Result<()>{Ok(())}
        }
        let open=||Worker::start(Session::new(Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap()).unwrap()).unwrap();
        let flag=Arc::new(AtomicBool::new(false));
        let fault=Worker::start(Session::new(Publisher::establish(Fault{inner:fixture(true),fail:flag.clone()},"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap()).unwrap()).unwrap();
        let connector:Connector=Arc::new(move|permit|if permit(){Ok(open())}else{Err(Failure::Unknown)});
        let mut fanout=Fanout::with_recovery(vec![(uuid::Uuid::new_v4().to_string(),open(),None),(uuid::Uuid::new_v4().to_string(),fault,Some(connector))],None).unwrap();
        let video=|t,key|crate::stream_mux::avc_access_unit(&[&[if key{0x65}else{0x41},1]],t,0,key).unwrap();
        fanout.submit(vec![crate::stream_mux::avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap(),video(0,true)]).unwrap();
        let until=Instant::now()+Duration::from_secs(2);
        while fanout.destinations().iter().any(|d|d.sent_packets<2){assert!(Instant::now()<until);std::thread::sleep(Duration::from_millis(5));}
        flag.store(true,Ordering::Release);
        fanout.submit(vec![video(10,false)]).unwrap();
        while fanout.destinations()[1].phase!="failed"{assert!(Instant::now()<until);std::thread::sleep(Duration::from_millis(5));}
        fanout.submit(vec![video(20,false)]).unwrap();
        assert_eq!(fanout.destinations()[1].phase,"reconnecting");
        std::thread::sleep(Duration::from_millis(1100));
        fanout.submit(vec![video(1100,false)]).unwrap();
        let until=Instant::now()+Duration::from_secs(2);
        while !fanout.test_awaiting_key(1){
            assert!(Instant::now()<until);std::thread::sleep(Duration::from_millis(30));
            fanout.submit(vec![video(1100,false)]).unwrap();
        }
        assert_eq!(fanout.destinations()[1].admitted_packets,3);
        fanout.submit(vec![video(1200,true)]).unwrap();
        fanout.submit(vec![video(1300,false)]).unwrap();
        assert!(fanout.finish().is_err()); // recovered gaps are not a clean soak
        let rows=fanout.destinations();
        assert_eq!(rows[0].phase,"stopped");assert!(!rows[0].interrupted);
        assert_eq!(rows[1].phase,"stopped");assert!(rows[1].interrupted);
        assert_eq!(rows[1].reconnect_attempt,1);
        assert_eq!(rows[1].sent_packets,5);assert_eq!(rows[1].admitted_packets,6);
    }
    #[test]fn retiring_disconnected_output_cancels_pending_recovery(){
        use crate::{live_session::Session,live_publish_worker::Worker,live_fanout::{Fanout,Connector},stream_mux::{avc_sequence,avc_access_unit}};
        use std::{sync::{Arc,atomic::{AtomicBool,AtomicUsize,Ordering}},time::{Duration,Instant}};
        struct Fault{inner:Fixture,fail:Arc<AtomicBool>}
        impl Read for Fault{fn read(&mut self,b:&mut[u8])->io::Result<usize>{self.inner.read(b)}}
        impl Write for Fault{
            fn write(&mut self,b:&[u8])->io::Result<usize>{
                if self.fail.load(Ordering::Acquire){Err(io::Error::from(ErrorKind::ConnectionReset))}else{self.inner.write(b)}
            }
            fn flush(&mut self)->io::Result<()>{Ok(())}
        }
        let open=||Worker::start(Session::new(Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap()).unwrap()).unwrap();
        let fault=Arc::new(AtomicBool::new(false));
        let broken=Worker::start(Session::new(Publisher::establish(Fault{inner:fixture(true),fail:fault.clone()},"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap()).unwrap()).unwrap();
        let calls=Arc::new(AtomicUsize::new(0));let count=calls.clone();
        let connector:Connector=Arc::new(move|_|{count.fetch_add(1,Ordering::AcqRel);Err(Failure::Unknown)});
        let a=uuid::Uuid::new_v4().to_string();let b=uuid::Uuid::new_v4().to_string();let stop=Arc::new(AtomicBool::new(false));
        let flags=std::collections::HashMap::from([(a.clone(),Arc::new(AtomicBool::new(false))),(b.clone(),stop.clone())]);
        let mut fanout=Fanout::with_recovery(vec![(a,open(),None),(b,broken,Some(connector))],None).unwrap().bind_destination_stops(flags).unwrap();
        let video=|t,key|avc_access_unit(&[&[if key{0x65}else{0x41},1]],t,0,key).unwrap();
        fanout.submit(vec![avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap(),video(0,true)]).unwrap();
        let until=Instant::now()+Duration::from_secs(2);
        while fanout.destinations().iter().any(|d|d.sent_packets<2){assert!(Instant::now()<until);std::thread::sleep(Duration::from_millis(5));}
        fault.store(true,Ordering::Release);
        fanout.submit(vec![video(10,false)]).unwrap();
        while fanout.destinations()[1].phase!="failed"{assert!(Instant::now()<until);std::thread::sleep(Duration::from_millis(5));}
        fanout.submit(vec![video(20,false)]).unwrap();
        assert_eq!(fanout.destinations()[1].phase,"reconnecting");
        stop.store(true,Ordering::Release);
        fanout.submit(vec![video(30,false)]).unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        fanout.submit(vec![video(1200,true)]).unwrap();
        assert!(fanout.finish().is_err());
        let rows=fanout.destinations();
        assert_eq!(calls.load(Ordering::Acquire),0);
        assert_eq!(rows[0].phase,"stopped");
        assert!(!rows[0].interrupted&&rows[1].interrupted);
        assert_eq!(rows[1].reconnect_attempt,0);
    }
    #[test]fn fanout_user_stop_drains_one_output_and_keeps_survivor(){
        use crate::{live_session::Session,live_publish_worker::Worker,live_fanout::Fanout,stream_mux::{avc_sequence,avc_access_unit}};
        use std::{sync::{Arc,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
        let open=||Worker::start(Session::new(Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap()).unwrap()).unwrap();
        let a=uuid::Uuid::new_v4().to_string();let b=uuid::Uuid::new_v4().to_string();
        let stop=Arc::new(AtomicBool::new(false));
        let flags=std::collections::HashMap::from([(a.clone(),stop.clone()),(b.clone(),Arc::new(AtomicBool::new(false)))]);
        let mut fanout=Fanout::new(vec![(a,open()),(b,open())]).unwrap().bind_destination_stops(flags).unwrap();
        let video=|t,key|avc_access_unit(&[&[if key{0x65}else{0x41},1]],t,0,key).unwrap();
        fanout.submit(vec![avc_sequence(&[0x67,0x64,0,0x28],&[0x68,0xee]).unwrap(),video(0,true)]).unwrap();
        let until=Instant::now()+Duration::from_secs(2);
        while fanout.destinations().iter().any(|d|d.sent_packets<2){assert!(Instant::now()<until);std::thread::sleep(Duration::from_millis(5));}
        stop.store(true,Ordering::Release);
        fanout.submit(vec![video(100,false)]).unwrap();
        fanout.submit(vec![video(200,false)]).unwrap();
        assert!(fanout.finish().is_err(),"retiring one output cannot be an uninterrupted pass");
        let rows=fanout.destinations();
        assert_eq!((rows[0].phase,rows[0].admitted_packets,rows[0].sent_packets),("stopped",2,2));
        assert!(rows[0].interrupted);assert_eq!(rows[0].reconnect_attempt,0);
        assert_eq!((rows[1].phase,rows[1].sent_packets),("stopped",4));assert!(!rows[1].interrupted);
    }
    #[test]fn fanout_rejects_duplicate_destination(){
        use crate::{live_session::Session,live_publish_worker::Worker,live_fanout::Fanout};
        let open=||Worker::start(Session::new(Publisher::establish(fixture(true),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]).unwrap()).unwrap()).unwrap();
        let id=uuid::Uuid::new_v4().to_string();
        assert!(Fanout::new(vec![(id.clone(),open()),(id,open())]).is_err());
    }
    #[test]fn rejects_publish_before_media(){
        let result=Publisher::establish(fixture(false),"live","rtmps://example.invalid/live","fixture-key",7,&[9;1528]);
        let Err(error)=result else{panic!("Rejected publish was accepted")};
        assert!(!error.contains("fixture-key"));
    }
}
