//! The longest continuous missing-PCM interval, separate from the sum of unrelated shortages.
use std::sync::atomic::{AtomicU64,Ordering};
#[derive(Default)]
pub(super) struct Gaps {
    played: AtomicU64,
    run: AtomicU64,
    start: AtomicU64,
    startup: AtomicU64,
    startup_last: AtomicU64,
    steady: AtomicU64,
}
impl Gaps {
    pub fn observe(&self, frames:usize, available:usize) {
        let before=self.played.fetch_add(frames as u64,Ordering::Relaxed);
        let missing=frames.saturating_sub(available) as u64;
        if missing==0 {self.break_gap();return;}
        let old=self.run.load(Ordering::Relaxed);
        let run=if available>0 || old==0 {
            self.start.store(before+available as u64,Ordering::Relaxed);
            self.run.store(missing,Ordering::Relaxed);missing
        } else {self.run.fetch_add(missing,Ordering::Relaxed)+missing};
        // Classify a burst by where it began, even if it crosses the 20-second boundary.
        let max=if self.start.load(Ordering::Relaxed)<48_000*20 {&self.startup} else {&self.steady};
        max.fetch_max(run,Ordering::Relaxed);
        if self.start.load(Ordering::Relaxed)<48_000*20 {self.startup_last.store(before+frames as u64,Ordering::Relaxed);}
    }
    pub fn break_gap(&self) {self.run.store(0,Ordering::Relaxed);}
    pub fn played(&self)->u64 {self.played.load(Ordering::Relaxed)}
    pub fn startup(&self)->u64 {self.startup.load(Ordering::Relaxed)}
    pub fn startup_last(&self)->u64 {self.startup_last.load(Ordering::Relaxed)}
    pub fn steady(&self)->u64 {self.steady.load(Ordering::Relaxed)}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separate_shortages_are_not_mistaken_for_one_long_mute() {
        let g=Gaps::default();
        g.observe(480,0);g.observe(480,200);g.observe(480,0);
        assert_eq!(g.startup(),760,"a valid prefix ends the preceding shortage");
        g.observe(480,480);g.observe(480,0);
        assert_eq!(g.startup(),760,"unrelated silent intervals must not be added");
        g.break_gap();g.observe(480,0);
        assert_eq!(g.startup(),760,"an intentional pause ends a shortage");
    }
    #[test]
    fn a_startup_shortage_can_end_after_twenty_seconds_without_becoming_a_play_shortage() {
        let g=Gaps::default();
        g.observe(48_000*20-100,48_000*20-100);
        g.observe(480,0);g.observe(480,0);
        assert_eq!((g.startup(),g.steady()),(960,0));
        assert_eq!(g.startup_last(),48_000*20+860);
        g.observe(480,480);g.observe(144,0);g.observe(144,0);
        assert_eq!((g.startup(),g.steady()),(960,288));
    }
}
