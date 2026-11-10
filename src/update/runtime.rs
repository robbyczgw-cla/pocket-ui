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
}

#[derive(Default)]
struct Progress {
    phase: WorkerPhase,
    boundary: CancelBoundary,
    bytes: u64,
    scope: BytesScope,
}

impl Progress {
    fn snapshot(&self, total: u64) -> WorkerSnapshot {
        let cancel_effective = match self.boundary {
            CancelBoundary::Download => matches!(self.phase, WorkerPhase::Starting | WorkerPhase::Layout | WorkerPhase::Connecting | WorkerPhase::Fallback | WorkerPhase::Downloading),
            CancelBoundary::Activation => !matches!(self.phase, WorkerPhase::MovingAssets | WorkerPhase::Flushing | WorkerPhase::Activating | WorkerPhase::Pruning | WorkerPhase::StagingSystem),
        };
        let (bytes, total) = if self.scope == BytesScope::CurrentFile { (self.bytes, total) } else { (0, 0) };
        WorkerSnapshot { phase: self.phase, bytes, total, scope: self.scope, cancel_effective }
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
            p.phase = phase;
            if phase == WorkerPhase::CheckingAssets { p.scope = BytesScope::None; p.bytes = 0; }
        });
    }

    pub(super) fn boundary(&self, boundary: CancelBoundary) { self.change(|p| p.boundary = boundary); }

    pub(crate) fn begin_file(&self, total: u64) {
        self.change(|p| { self.0.total.store(total, Ordering::Relaxed); p.scope = BytesScope::CurrentFile; p.bytes = 0; });
    }

    /// Runs immediately before the existing progress callback, without changing its count or order.
    pub(super) fn bytes(&self, bytes: u64) {
        self.0.progress.lock().unwrap_or_else(|e| e.into_inner()).bytes = bytes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
