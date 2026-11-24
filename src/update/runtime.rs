//! In-memory observations around the existing installer effects. No phase performs an effect or checks cancellation.

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkerPhase {
    #[default]
    Starting,
    Layout,
    Connecting,
    Fallback,
    Downloading,
    CheckingDownload,
    Preparing,
    CheckingAssets,
    Unpacking,
    Validating,
    MovingAssets,
    Flushing,
    Activating,
    Pruning,
    StagingSystem,
    SavingCheats,
}

impl WorkerPhase {
    pub fn code(self) -> &'static str {
        match self {
            Self::Starting => "starting", Self::Layout => "preparing-layout",
            Self::Connecting => "connecting", Self::Fallback => "fallback", Self::Downloading => "downloading",
            Self::CheckingDownload => "checking-download", Self::Preparing => "preparing",
            Self::CheckingAssets => "checking-assets", Self::Unpacking => "unpacking", Self::Validating => "validating",
            Self::MovingAssets => "moving-assets", Self::Flushing => "flushing", Self::Activating => "activating",
            Self::Pruning => "pruning", Self::StagingSystem => "staging-system",
            Self::SavingCheats => "saving-cheats",
        }
    }

    /// Preserve the page's existing labels, including its generic label before the first connection.
    pub fn label(self) -> Option<&'static str> {
        match self {
            Self::Starting | Self::Layout => None,
            Self::Connecting => Some("Connecting to the update server…"),
            Self::Fallback => Some("Trying the next download server…"),
            Self::Downloading => Some("Downloading the update…"),
            Self::CheckingDownload => Some("Checking the download…"),
            Self::CheckingAssets => Some("Checking installed cores and data…"),
            Self::SavingCheats => Some("Saving cheats…"),
            _ => Some("Preparing the update…"),
        }
    }
}

/// The four steps the Updates page names (Downloading → Unpacking → Installing → Restarting). Restarting comes after the worker,
/// when the page has its result. A phase never moves the step back: the Brick's checks of installed parts still count as the download.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step { Downloading, Unpacking, Installing, Restarting }

const STEPS: u32 = 4;

impl Step {
    pub fn code(self) -> &'static str {
        match self { Self::Downloading => "downloading", Self::Unpacking => "unpacking", Self::Installing => "installing", Self::Restarting => "restarting" }
    }
    /// 1 to 4, for "Step 2 of 4".
    pub fn number(self) -> u32 { self as u32 + 1 }

    /// The Updates page's line for an app install in this step, `percent` done (assets/i18n has each line whole).
    pub fn line(self, percent: u32) -> String {
        match self {
            Self::Downloading => format!("Step {} of {}: Downloading the update… {}%", self.number(), STEPS, percent),
            Self::Unpacking => format!("Step {} of {}: Unpacking the update… {}%", self.number(), STEPS, percent),
            Self::Installing => format!("Step {} of {}: Installing the update… {}%", self.number(), STEPS, percent),
            Self::Restarting => format!("Step {} of {}: Restarting…", self.number(), STEPS),
        }
    }
}

impl WorkerPhase {
    pub fn step(self) -> Step {
        match self {
            Self::Unpacking | Self::Validating => Step::Unpacking,
            Self::MovingAssets | Self::Flushing | Self::Activating | Self::Pruning | Self::StagingSystem => Step::Installing,
            _ => Step::Downloading,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CancelBoundary {
    /// App/OS downloads: a valid completed download has no later cancellation check.
    #[default]
    Download,
    /// Brick retains its existing checks through asset staging, before the first reused-asset move.
    Activation,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BytesScope { #[default] None, CurrentFile }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkerSnapshot {
    pub phase: WorkerPhase,
    pub bytes: u64,
    pub total: u64,
    pub scope: BytesScope,
    pub cancel_effective: bool,
    /// Work done in the current step after the download (bytes unpacked, parts moved, files written), out of `work_total`;
    /// both 0 until the step names its total. Never goes back within a step.
    pub work_done: u64,
    pub work_total: u64,
}

#[derive(Default)]
struct Progress {
    phase: WorkerPhase,
    boundary: CancelBoundary,
    bytes: u64,
    scope: BytesScope,
    work: (u64, u64),
    meter: Meter,
}

/// Parts per ten thousand of the bar.
const WHOLE: u64 = 10_000;

/// Every file of one install's download counted as one bar (Alex, 20.11., the Brick's update to 0.6.0: "Connecting to update
/// server" for every small part and a bar from 0 again, "schaut buggy aus"). The bar only moves forward: when more files become
/// known (the Brick learns its changed parts from inside the app archive), the rest of the bar is shared out anew from where it is.
#[derive(Default)]
struct Meter {
    /// Bytes of the files done.
    finished: u64,
    /// Bytes of every file known so far, done or not.
    planned: u64,
    /// The current file: its size and its bytes so far.
    size: u64,
    current: u64,
    /// The first report of a file is where it starts (a part kept from an earlier try), not bytes that came in now.
    fresh: bool,
    /// More files may follow that aren't counted yet: the known ones fill only up to half of the bar until they are.
    open: bool,
    /// The bar at `from.1` bytes done was at `from.0`; what is left of the bar is shared out over the bytes still to come.
    from: (u64, u64),
    shown: u64,
    /// Bytes that came in over the network, and when the first did: the speed for the time left.
    received: u64,
    since: Option<Instant>,
    /// Retrying after a lost connection: try `n` of `m`.
    retry: Option<(u32, u32)>,
}

impl Meter {
    fn done(&self) -> u64 { self.finished.saturating_add(self.current.min(self.size)) }
    fn cap(&self) -> u64 { if self.open { WHOLE / 2 } else { WHOLE } }

    /// Shares the rest of the bar out anew from here.
    fn anchor(&mut self) { self.from = (self.shown, self.done()); }

    fn advance(&mut self) {
        let (done, total) = (self.done(), self.planned.max(self.done()));
        if done < self.from.1 { self.anchor(); }
        let (f0, d0) = self.from;
        let cap = self.cap().max(f0);
        let f = if total <= d0 { cap } else { f0 + (cap - f0) * (done - d0) / (total - d0) };
        self.shown = self.shown.max(f.min(WHOLE));
    }

    fn grow(&mut self, planned: u64) {
        if planned > self.planned { self.planned = planned; self.anchor(); }
        self.advance();
    }

    fn begin(&mut self, size: u64) {
        self.size = size;
        self.current = 0;
        self.fresh = true;
        self.grow(self.finished.saturating_add(size));
    }

    fn bytes(&mut self, n: u64, now: Instant) {
        if self.fresh {
            self.fresh = false;
        } else if n > self.current {
            self.received += n - self.current;
            self.since.get_or_insert(now);
        }
        if n > self.current { self.retry = None; }
        // The file again from nothing (another server, a bad kept part): the rest of the bar from where it is.
        let back = n < self.current;
        self.current = n;
        if back { self.anchor(); }
        self.advance();
    }

    fn file_done(&mut self) {
        self.finished = self.finished.saturating_add(self.size);
        self.size = 0;
        self.current = 0;
        self.advance();
    }

    fn view(&self, now: Instant) -> Download {
        let left = self.planned.saturating_sub(self.done());
        let secs_left = self.since.map(|t| now.duration_since(t).as_secs_f64())
            .filter(|s| *s >= 3.0 && self.received > 0 && !self.open)
            .map(|s| (left as f64 * s / self.received as f64).ceil() as u64);
        Download { part: self.shown, started: self.received > 0 || self.done() > 0, secs_left, retry: self.retry }
    }
}

/// What the Updates page says about a download: one bar for all of its files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Download {
    /// Parts per ten thousand; never less than before.
    pub part: u64,
    /// A byte arrived: from here on the page never says "Connecting" again.
    pub started: bool,
    pub secs_left: Option<u64>,
    /// After a lost connection: try `n` of `m`.
    pub retry: Option<(u32, u32)>,
}

impl Download {
    pub fn percent(&self) -> u32 { (self.part * 100 / WHOLE) as u32 }
    pub fn fraction(&self) -> f32 { self.part as f32 / WHOLE as f32 }

    /// The page's line for this download: one sentence with the share of all files and the time left, or the try after a
    /// lost connection.
    /// The short checks say their words only once everything is downloaded; before, they would flash between two parts.
    pub fn line(&self, phase: WorkerPhase) -> String {
        match (phase, self.retry) {
            (WorkerPhase::CheckingDownload | WorkerPhase::CheckingAssets, _) if self.part >= WHOLE => phase.label().unwrap_or_default().to_string(),
            (WorkerPhase::Fallback, Some((n, m))) => format!("Connection lost, trying again ({n}/{m})"),
            (WorkerPhase::Connecting | WorkerPhase::Fallback, _) if !self.started => WorkerPhase::Connecting.label().unwrap_or_default().to_string(),
            _ => {
                let line = Step::Downloading.line(self.percent());
                match self.secs_left {
                    Some(s) => format!("{line} · {}", time_left(s)),
                    None => line,
                }
            }
        }
    }
}

/// "About 3 min left", rounded up to whole minutes: a download that says one minute is not over in ten seconds.
pub fn time_left(secs: u64) -> String {
    let m = secs.div_ceil(60);
    match m {
        0 | 1 if secs < 60 => "Less than a minute left".to_string(),
        m if m < 60 => format!("About {m} min left"),
        m => format!("About {} h {:02} min left", m / 60, m % 60),
    }
}

impl Progress {
    fn snapshot(&self, total: u64) -> WorkerSnapshot {
        let cancel_effective = match self.boundary {
            CancelBoundary::Download => matches!(self.phase, WorkerPhase::Starting | WorkerPhase::Layout | WorkerPhase::Connecting | WorkerPhase::Fallback | WorkerPhase::Downloading),
            CancelBoundary::Activation => !matches!(self.phase, WorkerPhase::MovingAssets | WorkerPhase::Flushing | WorkerPhase::Activating | WorkerPhase::Pruning | WorkerPhase::StagingSystem),
        };
        let (bytes, total) = if self.scope == BytesScope::CurrentFile { (self.bytes, total) } else { (0, 0) };
        WorkerSnapshot { phase: self.phase, bytes, total, scope: self.scope, cancel_effective, work_done: self.work.0, work_total: self.work.1 }
    }
}

type Observer = Arc<dyn Fn(WorkerSnapshot) + Send + Sync>;

#[derive(Default)]
struct Shared {
    requested: AtomicBool,
    progress: Mutex<Progress>,
    // Preserve the existing nonblocking page getter independently of status notifications.
    total: AtomicU64,
    observer: Mutex<Option<Observer>>,
}

/// The existing cancellation flag, with typed worker observations. It never adds a cancellation check.
#[derive(Clone, Default)]
pub struct Cancel(Arc<Shared>);

impl Cancel {
    pub fn new() -> Self { Self::default() }
    pub fn cancel(&self) { self.0.requested.store(true, Ordering::SeqCst); }
    pub fn is_cancelled(&self) -> bool { self.0.requested.load(Ordering::SeqCst) }
    pub fn progress_total(&self) -> u64 { self.0.total.load(Ordering::Relaxed) }

    pub fn status(&self) -> Option<&'static str> {
        self.phase().and_then(WorkerPhase::label)
    }

    pub fn phase(&self) -> Option<WorkerPhase> { self.0.progress.try_lock().ok().map(|p| p.phase) }

    /// Called before spawning a worker, or by the worker itself. The UI projection reads its event-owned copy.
    pub fn snapshot(&self) -> WorkerSnapshot {
        self.0.progress.lock().unwrap_or_else(|e| e.into_inner()).snapshot(self.progress_total())
    }

    pub fn observe(&self, observer: impl Fn(WorkerSnapshot) + Send + Sync + 'static) {
        *self.0.observer.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(observer));
    }

    fn notify(&self, snapshot: WorkerSnapshot) {
        let observer = self.0.observer.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(observer) = observer { observer(snapshot); }
    }

    fn change(&self, change: impl FnOnce(&mut Progress)) {
        let snapshot = {
            let mut p = self.0.progress.lock().unwrap_or_else(|e| e.into_inner());
            let before = p.snapshot(self.progress_total());
            change(&mut p);
            let after = p.snapshot(self.progress_total());
            (before != after).then_some(after)
        };
        if let Some(snapshot) = snapshot { self.notify(snapshot); }
    }

    pub fn set_phase(&self, phase: WorkerPhase) {
        self.change(|p| {
            if phase.step() != p.phase.step() { p.work = (0, 0); }
            p.phase = phase;
            if phase == WorkerPhase::CheckingAssets { p.scope = BytesScope::None; p.bytes = 0; }
        });
    }

    pub(super) fn boundary(&self, boundary: CancelBoundary) { self.change(|p| p.boundary = boundary); }

    pub(crate) fn begin_file(&self, total: u64) {
        self.change(|p| { self.0.total.store(total, Ordering::Relaxed); p.scope = BytesScope::CurrentFile; p.bytes = 0; p.meter.begin(total); });
    }

    /// The current file is complete and checked: its bytes count as done for the whole download.
    pub(crate) fn file_done(&self) { self.lock().meter.file_done(); }

    /// More files may follow that this download doesn't know yet (the Brick's parts are named inside its app archive).
    pub(crate) fn more_files_may_follow(&self) { let mut p = self.lock(); p.meter.open = true; p.meter.advance(); }

    /// Every file is known now: `bytes` more are still to come after the ones done.
    pub(crate) fn files_known(&self, bytes: u64) {
        let mut p = self.lock();
        let m = &mut p.meter;
        if m.open { m.open = false; m.anchor(); }
        let planned = m.done().saturating_add(m.size.saturating_sub(m.current)).saturating_add(bytes);
        m.grow(planned);
    }

    /// After a lost connection the next try is `n` of `m`.
    pub(crate) fn retrying(&self, n: u32, m: u32) { self.lock().meter.retry = Some((n, m)); }

    /// The whole download so far, for the page.
    pub fn download(&self) -> Download { self.download_at(Instant::now()) }
    fn download_at(&self, now: Instant) -> Download { self.lock().meter.view(now) }

    /// Snapshots: a download `done` of `total` bytes whose bytes have been coming for `secs`, perhaps retrying.
    pub fn pretend_download(&self, done: u64, total: u64, secs: u64, retry: Option<(u32, u32)>) {
        let now = Instant::now();
        self.begin_file(total);
        self.bytes_at(0, now);
        self.bytes_at(done, now);
        let mut p = self.lock();
        p.meter.since = now.checked_sub(std::time::Duration::from_secs(secs));
        p.meter.retry = retry;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Progress> { self.0.progress.lock().unwrap_or_else(|e| e.into_inner()) }

    /// The current step's amount of work (bytes, parts, files); its progress starts at 0.
    pub(crate) fn work_begin(&self, total: u64) { self.change(|p| p.work = (0, total)); }

    /// `n` more done in the current step.
    pub(crate) fn work_add(&self, n: u64) { self.work_with(|done, _| done.saturating_add(n)); }

    /// `done` in all of the current step, never less than before.
    pub(crate) fn work_at(&self, done: u64) { self.work_with(|_, _| done); }

    /// The page hears of the step's progress once per tenth of a percent, not for every block.
    fn work_with(&self, next: impl FnOnce(u64, u64) -> u64) {
        let snapshot = {
            let mut p = self.0.progress.lock().unwrap_or_else(|e| e.into_inner());
            let (done, total) = p.work;
            let now = next(done, total).clamp(done, total);
            p.work.0 = now;
            let permille = |d: u64| if total == 0 { 0 } else { d.saturating_mul(1000) / total };
            (permille(now) != permille(done)).then(|| p.snapshot(self.progress_total()))
        };
        if let Some(snapshot) = snapshot { self.notify(snapshot); }
    }

    /// Runs immediately before the existing progress callback, without changing its count or order.
    pub(super) fn bytes(&self, bytes: u64) { self.bytes_at(bytes, Instant::now()); }

    fn bytes_at(&self, bytes: u64, now: Instant) {
        let mut p = self.lock();
        p.bytes = bytes;
        p.meter.bytes(bytes, now);
    }
}

/// Tests: what the worker told the page in `step` shows real progress: a total, more than nothing done while `phase` ran, never
/// less than before, and all of it at the end.
#[cfg(test)]
pub(crate) fn assert_step_progress(seen: &[WorkerSnapshot], step: Step, phase: WorkerPhase) {
    let in_step: Vec<&WorkerSnapshot> = seen.iter().filter(|s| s.phase.step() == step && s.work_total > 0).collect();
    assert!(!in_step.is_empty(), "{step:?} names no amount of work: {seen:?}");
    assert!(seen.iter().any(|s| s.phase == phase && s.work_done > 0 && s.work_total > 0), "nothing done while {phase:?} ran: {seen:?}");
    for pair in in_step.windows(2) {
        assert!(pair[1].work_done >= pair[0].work_done && pair[1].work_total == pair[0].work_total, "{step:?} went back: {pair:?}");
    }
    let last = in_step.last().unwrap();
    assert_eq!(last.work_done, last.work_total, "{step:?} ends with all of its work done");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_line_is_translated_whole() {
        use crate::i18n::Lang;
        let de = Lang::De.table().unwrap();
        assert_eq!(de.translate(&Step::Unpacking.line(42)).as_deref(), Some("Schritt 2 von 4: Update wird entpackt… 42 %"));
        assert_eq!(de.translate(&Step::Restarting.line(100)).as_deref(), Some("Schritt 4 von 4: Startet neu …"));
        for lang in [Lang::Es, Lang::Pt, Lang::Fr, Lang::It, Lang::Ko] {
            for step in [Step::Downloading, Step::Unpacking, Step::Installing, Step::Restarting] {
                let line = step.line(7);
                let got = lang.table().unwrap().translate(&line);
                assert!(got.as_deref().is_some_and(|t| t != line && t.contains(&step.number().to_string())), "{lang:?} {line:?}: {got:?}");
            }
        }
    }

    /// Alex, 20.11. (the Brick's update to 0.6.0): "Connecting to update server" flashed for every small part and the bar
    /// started again for each. Many small parts as the worker reports them: one sentence, a bar that only rises, no "Connecting".
    #[test]
    fn many_small_parts_give_one_rising_bar_and_one_sentence() {
        let cancel = Cancel::new();
        let t0 = Instant::now();
        let at = |ms: u64| t0 + std::time::Duration::from_millis(ms);
        let mut clock = 0;
        let mut seen: Vec<(WorkerPhase, Download)> = Vec::new();
        let look = |cancel: &Cancel, seen: &mut Vec<(WorkerPhase, Download)>, now: Instant| seen.push((cancel.phase().unwrap(), cancel.download_at(now)));
        // The app archive first; it names 40 changed parts inside.
        cancel.more_files_may_follow();
        cancel.begin_file(4_000);
        cancel.set_phase(WorkerPhase::Connecting);
        look(&cancel, &mut seen, at(clock));
        for n in (0..=4_000).step_by(500) {
            clock += 100;
            cancel.bytes_at(n, at(clock));
            cancel.set_phase(WorkerPhase::Downloading);
            look(&cancel, &mut seen, at(clock));
        }
        cancel.set_phase(WorkerPhase::CheckingDownload);
        look(&cancel, &mut seen, at(clock));
        cancel.file_done();
        cancel.set_phase(WorkerPhase::CheckingAssets);
        look(&cancel, &mut seen, at(clock));
        cancel.files_known(40 * 300);
        for _ in 0..40 {
            cancel.begin_file(300);
            for phase in [WorkerPhase::Preparing, WorkerPhase::Connecting] { cancel.set_phase(phase); look(&cancel, &mut seen, at(clock)); }
            for n in [0, 100, 300] {
                clock += 50;
                cancel.bytes_at(n, at(clock));
                cancel.set_phase(WorkerPhase::Downloading);
                look(&cancel, &mut seen, at(clock));
            }
            cancel.set_phase(WorkerPhase::CheckingDownload);
            look(&cancel, &mut seen, at(clock));
            cancel.file_done();
            cancel.set_phase(WorkerPhase::Preparing);
            look(&cancel, &mut seen, at(clock));
        }
        let lines: Vec<String> = seen.iter().map(|(phase, d)| d.line(*phase)).collect();
        assert_eq!(lines[0], "Connecting to the update server…", "before the first byte");
        let first_byte = seen.iter().position(|(_, d)| d.started).unwrap();
        for ((phase, d), line) in seen.iter().zip(&lines).skip(first_byte) {
            // Only the very last part's check, with the bar full, says its own words.
            if d.part >= WHOLE && *phase == WorkerPhase::CheckingDownload { assert_eq!(line, "Checking the download…"); continue; }
            assert!(line.starts_with("Step 1 of 4: Downloading the update… "), "{phase:?} {d:?}: {line:?}");
        }
        for pair in seen.windows(2) { assert!(pair[1].1.part >= pair[0].1.part, "the bar went back: {pair:?}"); }
        let percents: std::collections::BTreeSet<u32> = seen.iter().map(|(_, d)| d.percent()).collect();
        assert!(percents.len() > 30, "the bar moves in many small steps, not 0 and 100: {percents:?}");
        assert!(seen.iter().any(|(_, d)| (45..=55).contains(&d.percent())), "the archive fills half the bar while its parts are unknown");
        assert_eq!(seen.last().unwrap().1.percent(), 100);
        assert_eq!(seen.last().unwrap().1.secs_left, Some(0));
        // Everything is downloaded: now the last checks may say their words.
        cancel.set_phase(WorkerPhase::CheckingAssets);
        assert_eq!(cancel.download_at(at(clock)).line(WorkerPhase::CheckingAssets), "Checking installed cores and data…");
    }

    #[test]
    fn more_files_than_expected_share_out_the_rest_and_a_retry_from_nothing_does_not_go_back() {
        let cancel = Cancel::new();
        let t0 = Instant::now();
        cancel.begin_file(1000);
        cancel.bytes_at(0, t0);
        cancel.bytes_at(800, t0);
        assert_eq!(cancel.download().percent(), 80);
        // A part kept from an earlier try was bad: the file comes again from nothing.
        cancel.bytes_at(0, t0);
        assert_eq!(cancel.download().percent(), 80, "never back");
        cancel.bytes_at(500, t0);
        assert_eq!(cancel.download().percent(), 90, "the rest of the bar over the bytes still to come");
        cancel.bytes_at(1000, t0);
        cancel.file_done();
        assert_eq!(cancel.download().percent(), 100);
        // A file nobody counted on: the bar stays where it is and moves on from there.
        cancel.begin_file(1000);
        assert_eq!(cancel.download().percent(), 100);
        let parts = Cancel::new();
        parts.more_files_may_follow();
        parts.begin_file(100);
        parts.bytes_at(0, t0);
        parts.bytes_at(100, t0);
        parts.file_done();
        assert_eq!(parts.download().percent(), 50);
        parts.files_known(0);
        assert_eq!(parts.download().percent(), 100, "no part changed: done at once, forward");
    }

    #[test]
    fn a_lost_connection_says_its_try_until_bytes_come_again_and_the_time_left_is_honest() {
        let cancel = Cancel::new();
        let t0 = Instant::now();
        let s = |n| t0 + std::time::Duration::from_secs(n);
        cancel.begin_file(6_000_000);
        cancel.bytes_at(0, s(0));
        cancel.bytes_at(1_000_000, s(1));
        assert_eq!(cancel.download_at(s(2)).secs_left, None, "no guess from the first moments");
        cancel.bytes_at(2_000_000, s(10));
        // 2 MB in the 9 s since the first new byte; 4 MB left.
        assert_eq!(cancel.download_at(s(10)).secs_left, Some(18));
        assert_eq!(cancel.download_at(s(10)).line(WorkerPhase::Downloading), "Step 1 of 4: Downloading the update… 33% · Less than a minute left");
        cancel.retrying(2, 3);
        assert_eq!(cancel.download_at(s(11)).line(WorkerPhase::Fallback), "Connection lost, trying again (2/3)");
        assert_eq!(cancel.download_at(s(12)).line(WorkerPhase::Fallback), "Connection lost, trying again (2/3)", "the same words while it waits");
        cancel.bytes_at(0, s(12));
        cancel.bytes_at(100, s(13));
        assert!(cancel.download_at(s(13)).line(WorkerPhase::Downloading).starts_with("Step 1 of 4: Downloading the update… 33%"));
        assert_eq!(time_left(59), "Less than a minute left");
        assert_eq!(time_left(61), "About 2 min left");
        assert_eq!(time_left(3600 + 5 * 60), "About 1 h 05 min left");
    }

    #[test]
    fn the_download_lines_are_translated_whole() {
        use crate::i18n::Lang;
        let line = format!("{} · {}", Step::Downloading.line(42), time_left(200));
        let de = Lang::De.table().unwrap();
        assert_eq!(de.translate(&line).as_deref(), Some("Schritt 1 von 4: Update wird geladen… 42 % · Noch etwa 4 min"));
        assert_eq!(de.translate("Connection lost, trying again (2/3)").as_deref(), Some("Verbindung verloren, neuer Versuch (2/3)"));
        for lang in [Lang::Es, Lang::Pt, Lang::Fr, Lang::It, Lang::Ko] {
            for en in [line.as_str(), "Step 1 of 4: Downloading the update… 7% · Less than a minute left", "About 1 h 05 min left", "Connection lost, trying again (2/3)"] {
                let got = lang.table().unwrap().translate(en);
                assert!(got.as_deref().is_some_and(|t| t != en && !t.contains("left") && !t.contains("Downloading")), "{lang:?} {en:?}: {got:?}");
            }
        }
    }

    #[test]
    fn step_progress_never_goes_back_and_a_new_step_starts_from_nothing() {
        let cancel = Cancel::new();
        cancel.set_phase(WorkerPhase::Unpacking);
        cancel.work_begin(1000);
        cancel.work_at(400);
        cancel.work_at(300);
        assert_eq!(cancel.snapshot().work_done, 400, "a smaller count is not a step back");
        cancel.work_add(5000);
        assert_eq!(cancel.snapshot().work_done, 1000, "never more than the total");
        cancel.set_phase(WorkerPhase::Validating);
        assert_eq!((cancel.snapshot().work_done, cancel.snapshot().work_total), (1000, 1000), "the same step keeps its count");
        cancel.set_phase(WorkerPhase::MovingAssets);
        assert_eq!((cancel.snapshot().work_done, cancel.snapshot().work_total), (0, 0));
        assert_eq!(WorkerPhase::CheckingAssets.step(), Step::Downloading, "the Brick's checks between downloads keep step 1");
        assert!(WorkerPhase::Preparing.step() < WorkerPhase::Unpacking.step() && WorkerPhase::Unpacking.step() < WorkerPhase::Flushing.step());
    }

    #[test]
    fn phase_observations_preserve_labels_file_scope_and_cancellation_boundaries() {
        let cancel = Cancel::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let records = Arc::clone(&seen);
        cancel.observe(move |s| records.lock().unwrap().push(s));
        cancel.begin_file(100);
        cancel.set_phase(WorkerPhase::Downloading);
        cancel.bytes(40);
        cancel.set_phase(WorkerPhase::CheckingDownload);
        assert_eq!(cancel.status(), Some("Checking the download…"));
        assert_eq!((cancel.snapshot().bytes, cancel.snapshot().total), (40, 100));
        assert!(!cancel.snapshot().cancel_effective, "a valid app/OS download has no later cancellation check");
        cancel.boundary(CancelBoundary::Activation);
        assert!(cancel.snapshot().cancel_effective, "Brick still checks before staging/moving assets");
        cancel.set_phase(WorkerPhase::CheckingAssets);
        assert_eq!(cancel.snapshot().scope, BytesScope::None);
        assert_eq!(cancel.progress_total(), 100, "the existing page getter keeps its behavior");
        cancel.begin_file(7);
        assert_eq!((cancel.snapshot().bytes, cancel.snapshot().total), (0, 7), "the next component is a new file scope");
        cancel.set_phase(WorkerPhase::MovingAssets);
        cancel.cancel();
        assert!(cancel.is_cancelled() && !cancel.snapshot().cancel_effective);
        assert_eq!(cancel.status(), Some("Preparing the update…"));
        let records = seen.lock().unwrap();
        assert!(records.iter().any(|s| s.phase == WorkerPhase::CheckingDownload && s.bytes == 40));
    }

    #[test]
    fn observers_run_without_holding_the_progress_lock() {
        let cancel = Cancel::new();
        let reader = cancel.clone();
        cancel.observe(move |s| assert_eq!(reader.status(), s.phase.label()));
        cancel.set_phase(WorkerPhase::Connecting);
        cancel.set_phase(WorkerPhase::Unpacking);
        // Break the test-only observer's captured handle; production observers capture only an event proxy.
        *cancel.0.observer.lock().unwrap() = None;
    }
}
