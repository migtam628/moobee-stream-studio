//! Pure recovery policy. Never opens a connection and never retries a denied
//! publish, invalid configuration, TLS certificate error or local media fault.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum Failure{TransientTransport,AuthorizationDenied,Certificate,InvalidMedia,InvalidConfiguration,Unknown}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum Decision{WaitUntil(u64),StartFreshEpoch{attempt:u8},Stop}
const DELAYS:[u64;3]=[1000,3000,9000];
const MAX_WINDOW_MS:u64=30000;
#[derive(Default)]
pub(crate) struct Policy{attempts:u8,started:Option<u64>,due:Option<u64>,last_clock:u64,stopped:bool}
impl Policy{
    pub(crate) fn failed(&mut self,now:u64,cause:Failure,authorized:bool,cancelled:bool)->Decision{
        if !self.check(now,authorized,cancelled)||cause!=Failure::TransientTransport||self.due.is_some(){
            self.stopped=true;self.due=None;return Decision::Stop
        }
        let start=*self.started.get_or_insert(now);
        let Some(delay)=DELAYS.get(self.attempts as usize)else{self.stopped=true;return Decision::Stop};
        let Some(due)=now.checked_add(*delay)else{self.stopped=true;return Decision::Stop};
        if due.saturating_sub(start)>MAX_WINDOW_MS{self.stopped=true;return Decision::Stop}
        self.due=Some(due);Decision::WaitUntil(due)
    }
    fn check(&mut self,now:u64,authorized:bool,cancelled:bool)->bool{
        if self.stopped||!authorized||cancelled||now<self.last_clock||self.started.is_some_and(|s|now.saturating_sub(s)>MAX_WINDOW_MS){
            self.stopped=true;self.due=None;return false
        }
        self.last_clock=now;true
    }
    pub(crate) fn poll(&mut self,now:u64,authorized:bool,cancelled:bool)->Decision{
        if !self.check(now,authorized,cancelled){return Decision::Stop}
        let Some(due)=self.due else{return Decision::Stop};
        if now<due{return Decision::WaitUntil(due)}
        self.due=None;self.attempts+=1;
        // Caller must establish a new authorized publisher AND reset the
        // encoder, timestamps, codec config and IDR; old queued media is invalid.
        Decision::StartFreshEpoch{attempt:self.attempts}
    }
}
/// Drive bounded recovery on the capture worker, not the UI. Each attempt
/// owns a fresh connection and encoder. Failure classification comes from the
/// caller's typed transport boundary, never from error message matching.
pub(crate) fn recover<T>(
    deadline_ms:u64,now:impl Fn()->u64,allowed:impl Fn()->bool,
    mut wait:impl FnMut(u64),mut attempt:impl FnMut(u8,u64)->Result<T,(Failure,String)>,
)->Result<T,String>{
    let mut policy=Policy::default();let mut index=0;
    loop{
        let remaining=deadline_ms.saturating_sub(now());
        if !allowed()||remaining<2000{return Err("Live recovery cancelled or deadline exceeded".into())}
        let (failure,error)=match attempt(index,remaining){Ok(value)=>return Ok(value),Err(e)=>e};
        let authorized=allowed();
        let mut decision=policy.failed(now(),failure,authorized,!authorized);
        loop{
            match decision{
                Decision::Stop=>return Err(error),
                Decision::StartFreshEpoch{attempt}=>{index=attempt;break},
                Decision::WaitUntil(due)=>{
                    let t=now();
                    if !allowed()||t>=deadline_ms||due>=deadline_ms{return Err("Live recovery cancelled or deadline exceeded".into())}
                    wait(due.saturating_sub(t).min(25));
                    let authorized=allowed();
                    decision=policy.poll(now(),authorized,!authorized);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn bounded_retries_never_replay_or_retry_denied_destinations(){
        let mut p=Policy::default();
        assert_eq!(p.failed(0,Failure::TransientTransport,true,false),Decision::WaitUntil(1000));
        assert_eq!(p.poll(999,true,false),Decision::WaitUntil(1000));
        assert_eq!(p.poll(1000,true,false),Decision::StartFreshEpoch{attempt:1});
        assert_eq!(p.failed(1000,Failure::TransientTransport,true,false),Decision::WaitUntil(4000));
        assert_eq!(p.poll(4000,true,false),Decision::StartFreshEpoch{attempt:2});
        assert_eq!(p.failed(4000,Failure::TransientTransport,true,false),Decision::WaitUntil(13000));
        assert_eq!(p.poll(13000,true,false),Decision::StartFreshEpoch{attempt:3});
        assert_eq!(p.failed(13000,Failure::TransientTransport,true,false),Decision::Stop);
        for cause in [Failure::AuthorizationDenied,Failure::Certificate,Failure::InvalidMedia,Failure::InvalidConfiguration,Failure::Unknown]{
            assert_eq!(Policy::default().failed(0,cause,true,false),Decision::Stop);
        }
    }
    #[test]fn orchestrator_restarts_only_fresh_classified_epochs(){
        use std::cell::Cell;
        let clock=Cell::new(0u64);let mut attempts=vec![];
        let value=recover(30000,||clock.get(),||true,|ms|clock.set(clock.get()+ms),|n,_|{
            attempts.push((n,clock.get()));
            if n<2{Err((Failure::TransientTransport,"fixture".into()))}else{Ok(42)}
        }).unwrap();
        assert_eq!(value,42);assert_eq!(attempts,vec![(0,0),(1,1000),(2,4000)]);
        let mut calls=0;
        assert!(recover::<()>(30000,||0,||true,|_|panic!("must not wait"),|_,_|{
            calls+=1;Err((Failure::Certificate,"denied".into()))
        }).is_err());assert_eq!(calls,1);
    }
    #[test]fn recovery_cancellation_prevents_new_epoch(){
        use std::cell::Cell;
        let clock=Cell::new(0u64);let active=Cell::new(true);let mut calls=0;
        assert!(recover::<()>(30000,||clock.get(),||active.get(),|ms|{
            clock.set(clock.get()+ms);active.set(false);
        },|_,_|{calls+=1;Err((Failure::TransientTransport,"fixture".into()))}).is_err());
        assert_eq!(calls,1);
    }
    #[test]fn cancellation_revocation_and_clock_errors_are_terminal(){
        for (authorized,cancelled,now) in [(false,false,1000),(true,true,1000),(true,false,99),(true,false,30101)]{
            let mut p=Policy::default();p.failed(100,Failure::TransientTransport,true,false);
            assert_eq!(p.poll(now,authorized,cancelled),Decision::Stop);
            assert_eq!(p.poll(1200,true,false),Decision::Stop);
        }
    }
}
