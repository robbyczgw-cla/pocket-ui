//! Read-only evidence from the installed launchers' existing formats. These classifications never drive recovery.

use std::path::{Path, PathBuf};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HealthObservation { #[default] Unobserved, Watching, Confirming, NotPending, Confirmed, TimedOut, Broken }

impl HealthObservation {
    pub fn code(self) -> &'static str {
        match self { Self::Unobserved => "unobserved", Self::Watching => "watching", Self::Confirming => "confirming", Self::NotPending => "not-pending", Self::Confirmed => "confirmed", Self::TimedOut => "timed-out", Self::Broken => "broken" }
    }
}

static HEALTH: Mutex<HealthObservation> = Mutex::new(HealthObservation::Unobserved);

pub fn record_health(health: HealthObservation) { if let Ok(mut current) = HEALTH.lock() { *current = health; } }
fn health() -> HealthObservation { HEALTH.lock().map(|h| *h).unwrap_or_default() }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout { #[default] Unobserved, Rocknix, Brick }
impl Layout {
    pub fn code(self) -> &'static str { match self { Self::Unobserved => "unobserved", Self::Rocknix => "rocknix", Self::Brick => "brick" } }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Pending { #[default] None, Ours(String), Other(String), Unreadable }
impl Pending {
    pub fn code(&self) -> &'static str { match self { Self::None => "none", Self::Ours(_) => "ours", Self::Other(_) => "other", Self::Unreadable => "unreadable" } }
    pub fn version(&self) -> Option<&str> { match self { Self::Ours(v) | Self::Other(v) => Some(v), _ => None } }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Recovery { #[default] Unobserved, NotInstalled, LegacyFlat, Clean, Staging, Ready, ActivationInterrupted, AssetRecovery, ConfirmationPending, ConfirmedMarkerRetained, Incomplete, Unreadable }
impl Recovery {
    pub fn code(self) -> &'static str {
        match self { Self::Unobserved => "unobserved", Self::NotInstalled => "not-installed", Self::LegacyFlat => "legacy-flat", Self::Clean => "clean", Self::Staging => "staging", Self::Ready => "ready", Self::ActivationInterrupted => "activation-interrupted", Self::AssetRecovery => "asset-recovery", Self::ConfirmationPending => "confirmation-pending", Self::ConfirmedMarkerRetained => "confirmed-marker-retained", Self::Incomplete => "incomplete", Self::Unreadable => "unreadable" }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Milestone { #[default] Unobserved, NotPending, Pending, Confirmed, Stable }
impl Milestone {
    pub fn code(self) -> &'static str { match self { Self::Unobserved => "unobserved", Self::NotPending => "not-pending", Self::Pending => "pending", Self::Confirmed => "confirmed", Self::Stable => "stable" } }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inspection {
    pub layout: Layout,
    pub pending: Pending,
    pub recovery: Recovery,
    pub milestone: Milestone,
    pub health: HealthObservation,
    pub recorded_stable: bool,
}

/// Only release-shaped identifiers cross into the public status line. Paths, URLs and free text never do.
pub fn public_version(raw: &str) -> Option<&str> {
    let v = raw.trim();
    let core = v.split(['-', '+']).next().unwrap_or("");
    (!v.is_empty() && v.len() <= 120
        && v.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+'))
        && core.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())))
        .then_some(v)
}

fn present(path: &Path) -> bool { std::fs::symlink_metadata(path).is_ok() }

fn version_file(path: &Path) -> std::io::Result<String> {
    let file = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 256 { return Err(std::io::Error::from(std::io::ErrorKind::InvalidData)); }
    let mut text = String::new();
    file.take(257).read_to_string(&mut text)?;
    public_version(&text).map(String::from).ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidData))
}

fn pending(root: &Path, running: &str) -> Pending {
    let path = root.join("update-pending");
    match version_file(&path) {
        Ok(v) if v == running => Pending::Ours(v),
        Ok(v) => Pending::Other(v),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !present(&path) => Pending::None,
        Err(_) => Pending::Unreadable,
    }
}

impl Inspection {


    /// Worker-only filesystem inspection. A poll can overlap an install; it is evidence, never a recovery verdict.
    pub fn read(layout: Layout, root: &Path, running: &str, release: &Path, health: HealthObservation) -> Self {
        if layout == Layout::Unobserved { return Self::default(); }
        let state = if layout == Layout::Brick { root.join("storage/pocket-ui") } else { root.to_path_buf() };
        let pending = pending(&state, running);
        let recorded_stable = match layout {
            Layout::Brick => version_file(&state.join("known-good")).is_ok_and(|v| v == running),
            Layout::Rocknix => std::fs::read_link(root.join("known-good")).ok().and_then(|p| root.join(p).canonicalize().ok()).zip(release.canonicalize().ok()).is_some_and(|(good, running)| good == running),
            _ => false,
        };
        let milestone = if pending != Pending::None { Milestone::Pending } else if recorded_stable { Milestone::Stable } else if health == HealthObservation::Confirmed { Milestone::Confirmed } else { Milestone::NotPending };
        let mut recovery = match std::fs::metadata(root) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Recovery::NotInstalled,
            Err(_) => Recovery::Unreadable,
            Ok(_) if layout == Layout::Brick => {
                let active = root.join("app/start.sh").is_file();
                let assets = ["app/.asset-moves", "app/.restore-assets", "app.previous/.restore-assets"].iter().any(|p| present(&root.join(p)))
                    || (!active && present(&root.join("app.previous/.asset-moves")));
                if assets { Recovery::AssetRecovery }
                else if !active && !present(&root.join("app")) && (root.join("app.previous/start.sh").is_file() || root.join("app.new/.ready").is_file()) { Recovery::ActivationInterrupted }
                else if !active { Recovery::Incomplete }
                else if root.join("app.new/.ready").is_file() { Recovery::Ready }
                else if present(&root.join("app.new")) { Recovery::Staging }
                else { Recovery::Clean }
            }
            Ok(_) => {
                let active = root.join("current/pocket-ui").is_file() && root.join("current/pocket-core").is_file();
                let staging = std::fs::read_dir(root.join("releases")).ok().is_some_and(|entries| entries.flatten().any(|e| e.file_name().to_string_lossy().starts_with(".staging-")));
                if active && staging { Recovery::Staging }
                else if active { Recovery::Clean }
                else if !present(&root.join("current")) && root.join("pocket-ui").is_file() && root.join("pocket-core").is_file() { Recovery::LegacyFlat }
                else if staging { Recovery::Staging }
                else { Recovery::Incomplete }
            }
        };
        if matches!(recovery, Recovery::Clean) && pending != Pending::None {
            recovery = if matches!(pending, Pending::Ours(_)) && health == HealthObservation::Confirmed { Recovery::ConfirmedMarkerRetained } else { Recovery::ConfirmationPending };
        }
        Self { layout, pending, recovery, milestone, health, recorded_stable }
    }

    pub fn stability_scope(&self) -> &'static str {
        match (self.recorded_stable, self.layout) { (true, Layout::Rocknix) => "release", (true, Layout::Brick) => "version", _ => "none" }
    }
}

/// One worker observes metadata at most every five seconds, or after a job result. Only changes wake the UI.
/// Dropping the sender ends it; no filesystem work happens in the status projection.
pub fn watch(changed: impl Fn(Inspection) + Send + 'static) -> Sender<()> {
    let (send, receive) = mpsc::channel();
    let _ = std::thread::Builder::new().name("update-status".into()).spawn(move || {
        #[cfg(feature = "brick")]
        let card = super::brick_card();
        #[cfg(not(feature = "brick"))]
        let card: Option<PathBuf> = None;
        let (layout, root) = card.map_or_else(|| (Layout::Rocknix, PathBuf::from(super::APP_ROOT)), |card| (Layout::Brick, card));
        let release = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
        let mut previous = None;
        loop {
            let next = Inspection::read(layout, &root, super::app_version(), &release, health());
            if previous.as_ref() != Some(&next) { changed(next.clone()); previous = Some(next); }
            match receive.recv_timeout(Duration::from_secs(5)) { Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}, Err(mpsc::RecvTimeoutError::Disconnected) => break }
        }
    });
    send
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(root: &Path, name: &str, bytes: &str) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn rocknix_pending_ownership_confirmation_and_exact_release_stability_stay_distinct() {
        let root = crate::testutil::TestDir::new("update-observe");
        for bin in ["pocket-ui", "pocket-core"] { file(&root, &format!("releases/1/{bin}"), "program"); }
        std::os::unix::fs::symlink("releases/1", root.join("current")).unwrap();
        std::os::unix::fs::symlink("releases/1", root.join("known-good")).unwrap();
        file(&root, "update-pending", "2\n");
        let read = |h| Inspection::read(Layout::Rocknix, &root, "1", &root.join("releases/1"), h);
        assert_eq!(read(HealthObservation::Confirmed).pending, Pending::Other("2".into()));
        assert_eq!(read(HealthObservation::Confirmed).milestone, Milestone::Pending);
        assert_eq!(std::fs::read_to_string(root.join("update-pending")).unwrap(), "2\n", "inspection never confirms or rewrites a marker");
        file(&root, "update-pending", "1");
        assert_eq!(read(HealthObservation::Confirmed).recovery, Recovery::ConfirmedMarkerRetained);
        std::fs::remove_file(root.join("update-pending")).unwrap();
        assert_eq!(read(HealthObservation::NotPending).milestone, Milestone::Stable);
        assert_eq!(read(HealthObservation::NotPending).stability_scope(), "release");
        std::fs::remove_file(root.join("known-good")).unwrap();
        assert_eq!(read(HealthObservation::Confirmed).milestone, Milestone::Confirmed);
    }

    #[test]
    fn brick_recovery_markers_are_observed_without_moving_or_deleting_anything() {
        let root = crate::testutil::TestDir::new("brick-observe");
        file(&root, "app.previous/start.sh", "old");
        file(&root, "app.new/start.sh", "new");
        file(&root, "app.new/.ready", "");
        let read = || Inspection::read(Layout::Brick, &root, "1", Path::new("unused"), HealthObservation::Unobserved);
        assert_eq!(read().recovery, Recovery::ActivationInterrupted);
        file(&root, "app.previous/.asset-moves", "cores/core.so\n");
        assert_eq!(read().recovery, Recovery::AssetRecovery);
        assert_eq!(std::fs::read_to_string(root.join("app.previous/.asset-moves")).unwrap(), "cores/core.so\n");
        assert!(root.join("app.new/.ready").is_file() && !root.join("app").exists());
        file(&root, "storage/pocket-ui/known-good", "1\n");
        assert_eq!(read().milestone, Milestone::Stable, "this records the existing version marker, not a frame or UI uptime");
        assert_eq!(read().stability_scope(), "version");
        file(&root, "storage/pocket-ui/update-pending", "1");
        assert_eq!(read().milestone, Milestone::Pending, "a known-good version does not erase a new pending install");
    }

    #[test]
    fn version_projection_rejects_addresses_free_text_and_control_characters() {
        for value in ["https://example.invalid/?token=secret", "network name", "5GNetwork", "1.2.3\tsecret", "", "../1"] { assert!(public_version(value).is_none(), "{value:?}"); }
        for value in ["1", "0.3.1", "0.3.1-rc1", "20261101", "0.3.1+dev"] { assert_eq!(public_version(value), Some(value)); }
    }

    #[test]
    fn an_odd_pending_marker_is_unreadable_without_following_it() {
        let root = crate::testutil::TestDir::new("update-odd-marker");
        file(&root, "unrelated", "654321");
        std::os::unix::fs::symlink("unrelated", root.join("update-pending")).unwrap();
        let state = Inspection::read(Layout::Rocknix, &root, "1", &root, HealthObservation::Unobserved);
        assert_eq!(state.pending, Pending::Unreadable);
        assert_eq!(state.pending.version(), None);
        assert!(root.join("update-pending").is_symlink());
        assert_eq!(std::fs::read_to_string(root.join("unrelated")).unwrap(), "654321");
    }
}
