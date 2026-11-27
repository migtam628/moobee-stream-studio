//! Arrival-based startup barrier. Does not claim device-native timestamps.
use std::{sync::{atomic::{AtomicBool,Ordering},mpsc::{Receiver,TryRecvError,RecvTimeoutError}},time::{Duration,Instant}};
pub(crate) const MAX_DISCARDED:usize=256;
pub(crate) const STARTUP_REFRESH:Duration=Duration::from_millis(1500);
pub(crate) fn check(stop:&AtomicBool,deadline:Instant)->Result<(),String>{
    if stop.load(Ordering::Acquire){return Err("Live Program cancelled during startup".into())}
    if Instant::now()>=deadline{return Err("Live Program startup refresh exceeded its deadline".into())}
    Ok(())
}
/// Discard pre-barrier queue entries without converting their pixel buffers.
/// Require an observed empty queue before accepting the next arrival.
pub(crate) fn purge<T>(rx:&Receiver<T>,stop:&AtomicBool,deadline:Instant)->Result<usize,String>{
    for n in 0..=MAX_DISCARDED{
        check(stop,deadline)?;
        match rx.try_recv(){
            Ok(_) if n==MAX_DISCARDED=>return Err("Live display startup backlog exceeded its frame budget".into()),
            Ok(_)=>{},
            Err(TryRecvError::Empty)=>return Ok(n),
            Err(TryRecvError::Disconnected)=>return Err("Live display disconnected during startup".into()),
        }
    }
    unreachable!()
}
pub(crate) fn fresh<T>(rx:&Receiver<T>,stop:&AtomicBool,deadline:Instant)->Result<T,String>{
    loop{
        check(stop,deadline)?;
        let wait=deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(25));
        match rx.recv_timeout(wait){
            Ok(frame)=>{check(stop,deadline)?;return Ok(frame)}
            Err(RecvTimeoutError::Timeout)=>{},
            Err(RecvTimeoutError::Disconnected)=>return Err("Live display disconnected during startup".into()),
        }
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    use std::sync::mpsc::channel;
    #[test]fn purge_discards_old_entries_and_fresh_accepts_next(){
        let (tx,rx)=channel();let stop=AtomicBool::new(false);let deadline=Instant::now()+Duration::from_secs(1);
        for n in 0..4{tx.send(n).unwrap();}
        assert_eq!(purge(&rx,&stop,deadline).unwrap(),4);
        tx.send(99).unwrap();assert_eq!(fresh(&rx,&stop,deadline).unwrap(),99);
    }
    #[test]fn overflow_disconnect_timeout_and_cancel_fail_closed(){
        let (tx,rx)=channel();let stop=AtomicBool::new(false);let deadline=Instant::now()+Duration::from_secs(1);
        for _ in 0..=MAX_DISCARDED{tx.send(0).unwrap();}
        assert!(purge(&rx,&stop,deadline).is_err());
        assert!(fresh(&rx,&stop,Instant::now()).is_err());
        stop.store(true,Ordering::Release);tx.send(1).unwrap();
        assert!(fresh(&rx,&stop,deadline).is_err());
        stop.store(false,Ordering::Release);drop(tx);
        assert!(purge(&rx,&stop,deadline).is_err());
    }
}
