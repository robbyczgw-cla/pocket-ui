//! The optional TrimUI cheat database in Settings → Games → Extras.
//! All inspection, hashing and card writes run on the same worker lock as the pause-menu download.
use std::path::{Path, PathBuf};
use std::sync::{atomic::{AtomicU64, Ordering}, Arc, mpsc};
use crate::{emu_menu::Row, update::{self, Cancel}};

pub const ABOUT: &str = "Codes for many games, used from the pause menu (HOME › Options › Cheats).";
pub const CONTINUE: &str = "You can go back. The next try continues the download.";
pub const CONNECT: &str = "Connect to Wi-Fi first to use these.";
pub const REMOVE: &str = "Remove cheats? A again removes them.";
const REMOVE_ERROR: &str = "Cheats couldn't be removed. Check the SD card and try again.";
const OFFER_SIZE: u64 = 31_367_177;

pub fn mb(bytes: u64) -> u64 { bytes.saturating_add((1 << 20) - 1) >> 20 }

/// Shared by the pause menu and Extras; the action is always an explicit A press.
pub fn get_row(size: u64, online: bool, error: Option<&str>) -> Row {
    Row { name: "Cheats".into(),
        value: if !online { "Needs Wi-Fi".into() } else if error.is_some() { "Retry".into() } else { format!("Get · {} MB", mb(size)) },
        desc: if !online { CONNECT.into() } else { error.unwrap_or(ABOUT).into() } }
}

#[derive(Clone)]
pub struct Source {
    pub descriptor: PathBuf,
    pub destination: PathBuf,
    // Only optional, known pack files; never a cheat preference directory or a custom database.
    legacy: Vec<PathBuf>,
}

impl Source {
    pub fn for_card(card: &Path) -> Self {
        Self { descriptor: card.join("app/database/cheats-download.json"),
            destination: card.join("storage/pocket-ui/cheats/cht.pack"),
            legacy: vec![card.join("app/cores/cht.pack"), card.join("app.previous/cores/cht.pack")] }
    }
    fn inspect(&self, cancel: &Cancel) -> Facts {
        let offered = update::cheats::description(&self.descriptor).ok();
        let installed = std::iter::once(&self.destination).chain(&self.legacy)
            .find(|p| !cancel.is_cancelled() && crate::cheat_download::installed(&[(*p).clone()]));
        let installed_bytes = installed.and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len());
        let update = installed.zip(offered.as_ref()).is_some_and(|(path, rel)|
            installed_bytes != Some(rel.size) || installed_hash(path, cancel).as_deref() != Some(rel.sha256.to_ascii_lowercase().as_str()));
        // Find an existing ancestor; do not create a directory just to paint a row.
        let free = self.destination.ancestors().find(|p| p.is_dir()).and_then(crate::starter::free_bytes);
        Facts { size: offered.as_ref().map_or(OFFER_SIZE, |r| r.size), installed: installed_bytes, update,
            available: offered.is_some(), free }
    }
    fn remove(&self) -> Result<(), String> {
        for file in std::iter::once(&self.destination).chain(&self.legacy) {
            match std::fs::remove_file(file) {
                Ok(()) => crate::fsutil::sync_dir(file.parent().unwrap()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(_) => return Err(REMOVE_ERROR.into()),
            }
        }
        // A cancelled download must not become installed on a future retry after removal.
        if let Some(parent) = self.destination.parent() {
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    let owned = name.strip_prefix(".cheats-").and_then(|s| s.strip_suffix(".part").or_else(|| s.strip_suffix(".writing")))
                        .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()));
                    if owned || name == ".legacy.part" {
                        std::fs::remove_file(entry.path()).map_err(|_| REMOVE_ERROR)?;
                    }
                }
            }
            crate::fsutil::sync_dir(parent);
        }
        Ok(())
    }
}

fn installed_hash(path: &Path, cancel: &Cancel) -> Option<String> {
    use std::io::Read;
    use sha2::Digest;
    if cancel.is_cancelled() || crate::covers::game_on() { return None; }
    let mut file = std::fs::File::open(path).ok()?;
    let mut hash = sha2::Sha256::new();
    let mut buffer = [0u8; 64 << 10];
    loop {
        if cancel.is_cancelled() || crate::covers::game_on() { return None; }
        let n = file.read(&mut buffer).ok()?;
        if n == 0 { break; }
        hash.update(&buffer[..n]);
    }
    Some(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Clone)]
struct Facts { size: u64, installed: Option<u64>, update: bool, available: bool, free: Option<u64> }
impl Default for Facts {
    fn default() -> Self { Self { size: OFFER_SIZE, installed: None, update: false, available: true, free: None } }
}

#[derive(Clone, Copy, PartialEq)]
enum Work { Inspect, Download, Remove }
struct Reply { facts: Facts, error: Option<String> }

pub struct Page {
    source: Source,
    facts: Facts,
    receive: Option<mpsc::Receiver<Reply>>,
    work: Work,
    cancel: Cancel,
    bytes: Arc<AtomicU64>,
    error: Option<String>,
    confirm: bool,
    demo: bool,
}

impl Page {
    /// Construction does no I/O. Inspection runs only when the Extras page is entered.
    pub fn new(source: Source) -> Self {
        Self { source, facts: Facts::default(), receive: None, work: Work::Inspect,
            cancel: Cancel::new(), bytes: Arc::default(), error: None, confirm: false, demo: false }
    }
    pub fn inspect(&mut self) { if !self.busy() { self.start(Work::Inspect); } }
    pub fn busy(&self) -> bool { self.receive.is_some() }
    /// A support-file projection from memory only, independent of app-update jobs.
    pub fn status(&self) -> (&'static str, u64, u64) {
        if self.busy() {
            let phase = match self.work {
                Work::Inspect => "checking", Work::Remove => "removing",
                Work::Download if self.cancel.phase() == Some(update::WorkerPhase::SavingCheats) => "saving",
                Work::Download if self.cancel.progress_total() == 0 => "checking",
                Work::Download => "downloading",
            };
            (phase, self.bytes.load(Ordering::Relaxed), self.cancel.progress_total())
        } else {
            (if self.error.is_some() { "retry" } else if self.confirm { "confirming-remove" }
                else if self.installed() { if self.facts.update { "update" } else { "ready" } }
                else { "available" }, 0, 0)
        }
    }
    pub fn installed(&self) -> bool { self.facts.installed.is_some() }
    pub fn cancel(&mut self) { self.cancel.cancel(); self.confirm = false; }
    pub fn poll(&mut self) -> bool {
        if self.demo { return false; }
        let Some(receive) = &self.receive else { return false };
        match receive.try_recv() {
            Ok(reply) => { self.facts = reply.facts; self.error = reply.error; self.receive = None; true }
            Err(mpsc::TryRecvError::Disconnected) => { self.receive = None; true }
            Err(mpsc::TryRecvError::Empty) => false,
        }
    }
    pub fn activate(&mut self, online: bool) {
        if !self.can_activate(online) { return; }
        if self.confirm { self.start(Work::Remove); }
        else if self.installed() && (!self.facts.update || !online) { self.confirm = true; }
        else if online {
            if !self.facts.available { self.error = Some(update::cheats::UNAVAILABLE.into()); }
            else if !self.low_space() { self.start(Work::Download); }
        }
    }
    /// X also allows removing an older installed database instead of taking its offered update.
    pub fn confirm_remove(&mut self) { if self.installed() && !self.busy() { self.confirm = true; } }
    pub fn can_activate(&self, online: bool) -> bool {
        !self.busy() && (self.confirm || (self.installed() && (!self.facts.update || !online))
            || (online && self.facts.available && !self.low_space()))
    }
    fn low_space(&self) -> bool { self.facts.free.is_some_and(|free| free < update::cheats::space_needed(self.facts.size)) }
    fn start(&mut self, work: Work) {
        if self.demo {
            self.confirm = false;
            if work == Work::Remove { self.facts.installed = None; self.facts.update = false; }
            return;
        }
        self.confirm = false; self.error = None; self.work = work;
        self.bytes.store(0, Ordering::Relaxed); self.cancel = Cancel::new();
        let (send, receive) = mpsc::channel(); self.receive = Some(receive);
        let source = self.source.clone(); let cancel = self.cancel.clone(); let bytes = self.bytes.clone();
        std::thread::spawn(move || {
            let Some(_writer) = crate::optional_data::lock(&cancel) else { return };
            if cancel.is_cancelled() { return; }
            let result = match work {
                Work::Inspect => Ok(()),
                Work::Remove => source.remove(),
                Work::Download => {
                    if crate::covers::game_on() { Err("Close the game before downloading extras.".into()) }
                    else { update::cheats::replace(&source.descriptor, &source.destination, &cancel,
                        &|n| { bytes.store(n, Ordering::Relaxed); }) }
                }
            };
            if cancel.is_cancelled() { return; }
            let facts = source.inspect(&cancel);
            if cancel.is_cancelled() { return; }
            let _ = send.send(Reply { facts, error: result.err() });
        });
    }
    pub fn row(&self, online: bool) -> Row {
        if self.busy() {
            let value = match self.work {
                Work::Inspect => "Checking…".into(), Work::Remove => "Removing…".into(),
                Work::Download if self.cancel.phase() == Some(update::WorkerPhase::SavingCheats) => "Saving…".into(),
                Work::Download => {
                    let total = self.cancel.progress_total();
                    if total == 0 { "Checking…".into() }
                    else { format!("{}%", self.bytes.load(Ordering::Relaxed).saturating_mul(100).checked_div(total).unwrap_or(0).min(100)) }
                }
            };
            return Row { name: "Cheats".into(), value, desc: CONTINUE.into() };
        }
        if self.confirm { return Row { name: "Cheats".into(), value: "Remove?".into(), desc: REMOVE.into() }; }
        if self.installed() && (!self.facts.update || !online) {
            return Row { name: "Cheats".into(), value: format!("Installed · {} MB", mb(self.facts.installed.unwrap())),
                desc: self.error.clone().unwrap_or_else(|| "Codes for many games. A offers to remove the downloaded cheats.".into()) };
        }
        if !online { return get_row(self.facts.size, false, None); }
        if !self.facts.available {
            return Row { name: "Cheats".into(), value: "Not available".into(), desc: update::cheats::UNAVAILABLE.into() };
        }
        if self.low_space() {
            let needed = mb(update::cheats::space_needed(self.facts.size));
            let remove = if self.installed() { "\nX offers to remove the installed cheats." } else { "" };
            return Row { name: "Cheats".into(), value: format!("Needs {needed} MB"),
                desc: format!("Not enough space on the SD card: {needed} MB needed, {} MB free.{remove}", self.facts.free.unwrap() >> 20) };
        }
        if self.facts.update && self.error.is_none() {
            return Row { name: "Cheats".into(), value: format!("Update · {} MB", mb(self.facts.size)),
                desc: "New codes are available. A updates them; X offers to remove them. The installed codes keep working until the update finishes.".into() };
        }
        get_row(self.facts.size, true, self.error.as_deref())
    }
    pub fn alert(&self, online: bool) -> bool {
        self.error.is_some() || (online && !self.busy() && (!self.facts.available
            || (self.low_space() && (!self.installed() || self.facts.update))))
    }
    pub fn action(&self, online: bool) -> &'static str {
        if self.confirm { "Remove" } else if self.installed() && (!self.facts.update || !online) { "Choose" }
        else if self.error.is_some() { "Retry" } else if self.facts.update { "Update" } else { "Get" }
    }
    /// Render fixtures use exactly the same state-to-row code, without card or network access.
    pub fn snapshot(state: &str) -> Self {
        let mut page = Self::new(Source::for_card(Path::new("")));
        page.demo = true;
        match state {
            "installed" | "installed-offline" => page.facts.installed = Some(OFFER_SIZE),
            "update" => { page.facts.installed = Some(OFFER_SIZE); page.facts.update = true; }
            "no-space" => page.facts.free = Some(12 << 20),
            "update-no-space" => { page.facts.free = Some(12 << 20); page.facts.installed = Some(OFFER_SIZE); page.facts.update = true; }
            "error" => page.error = Some("The cheat download is damaged. Try again.".into()),
            "downloading" | "saving" | "checking" => {
                let (_, receive) = mpsc::channel(); page.receive = Some(receive); page.work = Work::Download;
                if state != "checking" { page.cancel.begin_file(OFFER_SIZE); page.bytes.store((OFFER_SIZE * 42).div_ceil(100), Ordering::Relaxed); }
                if state == "saving" { page.cancel.set_phase(update::WorkerPhase::SavingCheats); }
            }
            "remove" => { page.facts.installed = Some(OFFER_SIZE); page.confirm = true; }
            _ => (),
        }
        page
    }
}
impl Drop for Page { fn drop(&mut self) { self.cancel.cancel(); } }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn fixture(dir: &Path) -> Source {
        let source = Source::for_card(dir);
        std::fs::create_dir_all(source.descriptor.parent().unwrap()).unwrap();
        std::fs::create_dir_all(source.destination.parent().unwrap()).unwrap();
        let pack = crate::cheat_pack::build(&[("Nintendo - Game Boy", vec![("Tetris.cht".into(), b"cheat0_code = \"01\"\n".to_vec())])]);
        std::fs::write(&source.destination, &pack).unwrap();
        std::fs::write(&source.descriptor, serde_json::to_vec(&serde_json::json!({
            "version":"1", "url":"file:///never-needed", "size":pack.len(), "sha256":crate::store::sha256_hex(&pack)
        })).unwrap()).unwrap();
        source
    }
    fn finish(page: &mut Page) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while page.busy() && std::time::Instant::now() < until {
            page.poll(); std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!page.busy());
    }
    #[test]
    fn signed_description_offers_update_only_for_a_different_pack_and_installed_stays_offline() {
        let dir = TestDir::new("extras-cheats-update"); let source = fixture(&dir);
        let facts = source.inspect(&Cancel::new()); assert!(facts.installed.is_some()); assert!(!facts.update);
        let mut json: serde_json::Value = serde_json::from_slice(&std::fs::read(&source.descriptor).unwrap()).unwrap();
        json["sha256"] = "0".repeat(64).into();
        std::fs::write(&source.descriptor, serde_json::to_vec(&json).unwrap()).unwrap();
        let mut page = Page::new(source); page.inspect(); finish(&mut page);
        assert!(page.row(true).value.starts_with("Update · "));
        assert!(page.row(false).value.starts_with("Installed · "));
    }
    #[test]
    fn removal_needs_two_presses_works_offline_and_keeps_per_game_settings_and_unknown_files() {
        let dir = TestDir::new("extras-cheats-remove"); let source = fixture(&dir);
        let prefs = dir.join("storage/pocket-ui/cheats/gb-Tetris.json");
        std::fs::write(&prefs, "remember my switches").unwrap();
        let unrelated = source.destination.with_file_name(".cheats-personal.part");
        std::fs::write(&unrelated, "keep").unwrap();
        let partial = source.destination.with_file_name(format!(".cheats-{}.part", "a".repeat(64)));
        std::fs::write(&partial, "checkpoint").unwrap();
        std::fs::create_dir_all(source.legacy[0].parent().unwrap()).unwrap();
        std::fs::copy(&source.destination, &source.legacy[0]).unwrap();
        let mut page = Page::new(source.clone()); page.inspect(); finish(&mut page);
        std::fs::remove_file(&source.descriptor).unwrap();
        page.activate(false); assert_eq!(page.row(false).desc, REMOVE); assert!(source.destination.exists());
        page.activate(false); finish(&mut page);
        assert!(!source.destination.exists()); assert!(!source.legacy[0].exists()); assert!(!partial.exists());
        assert_eq!(std::fs::read_to_string(prefs).unwrap(), "remember my switches"); assert!(unrelated.exists());
        assert_eq!(page.row(false).value, "Needs Wi-Fi");
    }
    #[test]
    fn low_space_counts_checkpoint_room_and_does_not_start_a_download() {
        let mut page = Page::snapshot("no-space");
        page.facts.free = Some((12 << 20) + (1 << 19));
        assert_eq!(page.row(true).value, "Needs 62 MB");
        assert!(page.row(true).desc.contains("12 MB free"));
        page.activate(true); assert!(!page.busy()); assert!(page.alert(true)); assert!(!page.can_activate(true));
    }
    #[test]
    fn an_offline_old_database_can_be_removed_even_when_an_update_is_offered() {
        let mut page = Page::snapshot("update");
        page.activate(false);
        assert_eq!(page.row(false).desc, REMOVE);
        assert_eq!(page.action(false), "Remove");
    }
    #[test]
    fn cancelling_a_queued_optional_worker_does_not_wait_for_the_active_download() {
        let first = Cancel::new();
        let _writer = crate::optional_data::lock(&first).unwrap();
        let cancel = Cancel::new(); let waiting = cancel.clone();
        let (send, receive) = mpsc::channel();
        let worker = std::thread::spawn(move || { let _ = send.send(crate::optional_data::lock(&waiting).is_some()); });
        cancel.cancel();
        assert!(!receive.recv_timeout(std::time::Duration::from_secs(1)).unwrap());
        worker.join().unwrap();
    }
}
