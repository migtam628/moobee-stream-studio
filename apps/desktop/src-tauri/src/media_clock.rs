//! Rational Program timeline. A single zero-based frame index maps to video
//! presentation time and audio sample ranges without accumulating rounding.
use std::time::Duration;
#[derive(Clone, Copy, Debug)]
pub(crate) struct MediaClock { fps: u32 }
impl MediaClock {
    pub(crate) fn new(fps: u32) -> Result<Self, String> {
        if !(1..=120).contains(&fps) { return Err("Invalid video clock rate".into()); }
        Ok(Self { fps })
    }
    pub(crate) fn time(&self, frame: u32) -> Duration {
        Duration::from_nanos(u64::from(frame) * 1_000_000_000 / u64::from(self.fps))
    }
    pub(crate) fn audio_range(&self, frame: u32, sample_rate: u32) -> Result<std::ops::Range<u64>, String> {
        if !(8_000..=192_000).contains(&sample_rate) { return Err("Invalid audio sample rate".into()); }
        let start = u64::from(frame) * u64::from(sample_rate) / u64::from(self.fps);
        let end = (u64::from(frame) + 1) * u64::from(sample_rate) / u64::from(self.fps);
        Ok(start..end)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audio_and_video_share_exact_rational_boundaries() {
        for fps in [10, 15, 24, 30, 60] {
            let clock = MediaClock::new(fps).unwrap();
            assert_eq!(clock.time(fps * 8), Duration::from_secs(8));
            let mut next = 0;
            for tick in 0..fps * 8 {
                let range = clock.audio_range(tick, 48_000).unwrap();
                assert_eq!(range.start, next);
                next = range.end;
            }
            assert_eq!(next, 384_000);
        }
    }
    #[test]
    fn two_hour_clock_boundaries_remain_exact(){
        for fps in [10,15,30]{
            let c=MediaClock::new(fps).unwrap();
            let ticks=fps*7200;
            assert_eq!(c.time(ticks),Duration::from_secs(7200));
            assert_eq!(c.audio_range(ticks-1,48000).unwrap().end,345600000);
            assert_eq!(c.audio_range(ticks,48000).unwrap().start,345600000);
        }
    }
    #[test]
    fn invalid_rates_are_rejected() {
        assert!(MediaClock::new(0).is_err());
        assert!(MediaClock::new(121).is_err());
        assert!(MediaClock::new(30).unwrap().audio_range(0, 0).is_err());
    }
}
