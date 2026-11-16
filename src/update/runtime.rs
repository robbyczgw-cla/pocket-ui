//! In-memory observations around the existing installer effects. No phase performs an effect or checks cancellation.

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
        self.change(|p| { self.0.total.store(total, Ordering::Relaxed); p.scope = BytesScope::CurrentFile; p.bytes = 0; });
    }

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
    pub(super) fn bytes(&self, bytes: u64) {
        self.0.progress.lock().unwrap_or_else(|e| e.into_inner()).bytes = bytes;
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
        for lang in [Lang::Es, Lang::Pt, Lang::Fr, Lang::It] {
            for step in [Step::Downloading, Step::Unpacking, Step::Installing, Step::Restarting] {
                let line = step.line(7);
                let got = lang.table().unwrap().translate(&line);
                assert!(got.as_deref().is_some_and(|t| t != line && t.contains(&step.number().to_string())), "{lang:?} {line:?}: {got:?}");
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
