//! Optional cheat data: one background worker, persistent partial downloads, no network on menu navigation.
use std::path::{Path, PathBuf};
use crate::cheats::CheatMenu;
use crate::emu_menu::Row;

pub const OFFLINE: &str = "Cheats need Wi-Fi once to download";

#[derive(Clone)]
pub struct Request {
    pub descriptor: PathBuf,
    pub destination: PathBuf,
    pub roots: Vec<PathBuf>,
    pub data: PathBuf,
    pub system: String,
    pub rom: PathBuf,
}

pub struct Page {
    request: Request,
    receive: Option<std::sync::mpsc::Receiver<Result<CheatMenu, String>>>,
    bytes: std::sync::Arc<std::sync::atomic::AtomicU64>,
    cancel: crate::update::Cancel,
    error: Option<String>,
    size: u64,
    demo_online: Option<bool>,
}

impl Page {
    pub fn new(request: Request) -> Self {
        Self { request, receive: None, bytes: Default::default(), cancel: Default::default(), error: None, size: 31_367_177, demo_online: None }
    }

    /// Deterministic --snapshot examples of the same rows the worker paints; never starts a thread.
    pub fn snapshot(state: &str) -> Self {
        let mut page = Self::new(Request { descriptor: PathBuf::new(), destination: PathBuf::new(), roots: Vec::new(),
            data: PathBuf::new(), system: "gb".into(), rom: PathBuf::new() });
        page.demo_online = Some(state != "cheats-offline");
        if state == "cheats-downloading" {
            let (_, receive) = std::sync::mpsc::channel();
            page.receive = Some(receive);
            page.cancel.begin_file(31_367_177);
            page.bytes.store(15_683_589, std::sync::atomic::Ordering::Relaxed);
        } else if state == "cheats-offline" {
            page.error = Some(OFFLINE.into());
        } else if state == "cheats-unavailable" {
            page.error = Some("Cheats aren't available in this build. Try updating pocket-ui.".into());
        }
        page
    }

    pub fn row(&self) -> Row {
        self.row_online(self.demo_online.unwrap_or_else(|| crate::wifi::ip().is_some()))
    }

    fn row_online(&self, online: bool) -> Row {
        if self.busy() {
            let bytes = self.bytes.load(std::sync::atomic::Ordering::Relaxed);
            let total = self.cancel.progress_total();
            let value = if self.cancel.phase() == Some(crate::update::WorkerPhase::SavingCheats) { "Saving…".into() }
                else if total == 0 { "Checking…".into() }
                else { format!("{}%", bytes.saturating_mul(100).checked_div(total).unwrap_or(0).min(100)) };
            Row { name: "Cheats".into(), value, desc: crate::cheat_data::CONTINUE.into() }
        } else {
            crate::cheat_data::get_row(self.size, online && self.error.as_deref() != Some(OFFLINE), self.error.as_deref())
        }
    }

    /// Read-only support fields from memory; never inspect the database here.
    pub fn status(&self) -> (&'static str, u64, u64) {
        if self.busy() {
            let total = self.cancel.progress_total();
            let phase = if self.cancel.phase() == Some(crate::update::WorkerPhase::SavingCheats) { "saving" }
                else if total == 0 { "checking" } else { "downloading" };
            (phase, self.bytes.load(std::sync::atomic::Ordering::Relaxed), total)
        } else {
            (match self.error.as_deref() { None => "available", Some(OFFLINE) => "offline", Some(_) => "retry" }, 0, 0)
        }
    }

    pub fn start(&mut self) {
        self.start_work(true);
    }

    pub fn check(&mut self) { self.start_work(false); }
    pub fn stop(&mut self) { self.cancel.cancel(); self.receive = None; }
    pub(crate) fn set_size(&mut self, size: u64) { self.size = size; }

    fn start_work(&mut self, download: bool) {
        if self.busy() { return; }
        self.cancel = Default::default();
        self.error = None;
        self.bytes.store(0, std::sync::atomic::Ordering::Relaxed);
        let request = self.request.clone();
        let bytes = self.bytes.clone();
        let cancel = self.cancel.clone();
        let (send, receive) = std::sync::mpsc::channel();
        self.receive = Some(receive);
        std::thread::spawn(move || {
            // Other optional packs share this lane until their cancelled worker has checkpointed.
            let Some(_guard) = crate::optional_data::lock(&cancel) else { return };
            if cancel.is_cancelled() { return; }
            if !download {
                let size = crate::update::cheats::description(&request.descriptor).ok().map(|rel| rel.size);
                let mut menu = CheatMenu::optional(request);
                if let Some(size) = size { menu.download_size(size); }
                let _ = send.send(Ok(menu));
                return;
            }
            let result = if installed(&request.roots) { Ok(()) } else {
                crate::update::cheats::install(&request.descriptor, &request.destination, &cancel, &|n| {
                    bytes.store(n, std::sync::atomic::Ordering::Relaxed);
                })
            }.map(|()| CheatMenu::optional(request));
            let _ = send.send(result);
        });
    }

    pub fn poll(&mut self) -> Option<CheatMenu> {
        use std::sync::mpsc::TryRecvError;
        match self.receive.as_ref()?.try_recv() {
            Ok(Ok(menu)) => { self.receive = None; Some(menu) }
            Ok(Err(error)) => { self.error = Some(error); self.receive = None; None }
            Err(TryRecvError::Disconnected) => {
                self.error = Some("Cheat download stopped. Try again.".into()); self.receive = None; None
            }
            Err(TryRecvError::Empty) => None,
        }
    }
    pub fn busy(&self) -> bool { self.receive.is_some() }
}
impl Drop for Page {
    fn drop(&mut self) { self.cancel.cancel(); }
}

pub fn installed(roots: &[PathBuf]) -> bool {
    ["gb", "gbc", "gba", "nes", "snes", "n64", "nds", "megadrive", "mastersystem", "gamegear", "psx", "psp",
        "dreamcast", "saturn", "pc", "fbn", "pcengine", "pcenginecd", "supergrafx", "fds", "sega32x", "segacd",
        "atari2600", "atari7800", "atarilynx", "tic-80"].iter().any(|system| available(roots, system))
}

pub fn available(roots: &[PathBuf], system: &str) -> bool {
    let Some(folder) = crate::cheats::folder(system) else { return false };
    roots.iter().any(|root| if root.is_dir() {
        std::fs::read_dir(root.join(folder)).is_ok_and(|mut entries| entries.any(|e| e.is_ok_and(|e| e.path().extension().is_some_and(|ext| ext == "cht"))))
    } else {
        crate::cheat_pack::list(root, folder).is_some_and(|entries| entries.iter().any(|e| crate::cheat_pack::read(e).is_some()))
    })
}

/// Bulk writes share the Brick updater's pacing budget. Always call after network children exit.
pub(crate) fn copy_to_card(source: &Path, dest: &Path) -> Result<(), String> {
    #[cfg(feature = "brick")]
    {
        let _idle = crate::update::brick_io::IdleIo::enter().map_err(|e| e.to_string())?;
        let mut from = std::fs::File::open(source).map_err(|e| e.to_string())?;
        let mut to = std::fs::File::create(dest).map_err(|e| e.to_string())?;
        crate::update::brick_io::Pacer::new().copy(&mut from, &mut to).map_err(|e| e.to_string())?;
    }
    #[cfg(not(feature = "brick"))]
    std::fs::copy(source, dest).map_err(|e| e.to_string())?;
    std::fs::File::open(dest).and_then(|f| f.sync_all()).map_err(|e| e.to_string())
}

/// Called by the update worker before retiring a legacy app folder. A failed copy keeps that app active.
pub fn preserve_legacy(card: &Path) -> Result<(), String> {
    let source = [card.join("app/cores/cht.pack"), card.join("app.previous/cores/cht.pack")]
        .into_iter().find(|p| p.is_file());
    let dest = card.join("storage/pocket-ui/cheats/cht.pack");
    if dest.is_file() { return Ok(()); }
    let Some(source) = source else { return Ok(()); };
    let parent = dest.parent().unwrap();
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(".legacy.part");
    copy_to_card(&source, &temp)?;
    if std::fs::metadata(&source).map_err(|e| e.to_string())?.len() != std::fs::metadata(&temp).map_err(|e| e.to_string())?.len()
        || crate::store::sha256_file(&source).map_err(|e| e.to_string())? != crate::store::sha256_file(&temp).map_err(|e| e.to_string())? {
        return Err("Couldn't keep the installed cheats. Nothing was changed.".into());
    }
    std::fs::File::open(&temp).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &dest).map_err(|e| e.to_string())?;
    crate::fsutil::sync_dir(parent);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    pub fn request(dir: &Path) -> Request {
        Request { descriptor: dir.join("cheats-download.json"), destination: dir.join("storage/cht.pack"),
            roots: vec![dir.join("storage/cht.pack")], data: dir.join("data"), system: "gb".into(), rom: "Tetris.gb".into() }
    }

    #[test]
    fn missing_database_offers_a_visible_download_and_wifi_explanation() {
        let dir = TestDir::new("cheats-missing");
        let page = Page::new(request(&dir));
        assert_eq!(page.row().name, "Cheats");
        assert_eq!(page.row_online(true).value, "Get · 30 MB");
        assert_eq!(page.row_online(true).desc, crate::cheat_data::ABOUT);
        assert_eq!(page.row_online(false).value, "Needs Wi-Fi");
        assert_eq!(page.status(), ("available", 0, 0));
    }

    #[test]
    fn existing_pack_survives_two_app_replacements_without_network() {
        let dir = TestDir::new("cheats-legacy");
        let bytes = crate::cheat_pack::build(&[("Nintendo - Game Boy", vec![("Tetris.cht".into(), b"cheat0_code = \"01\"\n".to_vec())])]);
        std::fs::create_dir_all(dir.join("app/cores")).unwrap();
        std::fs::write(dir.join("app/cores/cht.pack"), &bytes).unwrap();
        preserve_legacy(&dir).unwrap();
        std::fs::remove_dir_all(dir.join("app")).unwrap();
        preserve_legacy(&dir).unwrap();
        let saved = dir.join("storage/pocket-ui/cheats/cht.pack");
        assert_eq!(std::fs::read(&saved).unwrap(), bytes);
        assert!(available(&[saved], "gb"));
    }

    #[test]
    fn rocknix_database_and_legacy_pack_are_already_available_even_for_unmatched_games() {
        let dir = TestDir::new("cheats-rocknix");
        let folder = dir.join("Nintendo - Game Boy");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("Another game.cht"), "cheat0_code = \"01\"\n").unwrap();
        assert!(available(&[dir.to_path_buf()], "gb"));
        assert!(!available(&[dir.to_path_buf()], "gba"));
    }
    fn finish(page: &mut Page) -> Option<CheatMenu> {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while page.busy() && std::time::Instant::now() < until {
            if let Some(menu) = page.poll() { return Some(menu); }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!page.busy(), "the worker finishes within five seconds");
        None
    }

    #[test]
    fn opening_cheats_shows_progress_then_codes_and_an_offline_retry_is_visible() {
        let dir = TestDir::new("cheats-page-download");
        let request = request(&dir);
        let source = dir.join("source.pack");
        let bytes = crate::cheat_pack::build(&[("Nintendo - Game Boy", vec![("Tetris.cht".into(),
            b"cheat0_desc = \"Lives\"\ncheat0_code = \"01\"\n".to_vec())])]);
        let descriptor = serde_json::json!({"version":"1", "url":format!("file://{}", source.display()),
            "size":bytes.len(), "sha256":crate::store::sha256_hex(&bytes)});
        std::fs::write(&request.descriptor, serde_json::to_vec(&descriptor).unwrap()).unwrap();
        let mut page = Page::new(request.clone());
        page.start();
        assert!(page.busy());
        assert_eq!(page.row().name, "Cheats");
        assert!(finish(&mut page).is_none());
        assert_eq!(page.row().name, "Cheats");
        assert_eq!(page.row().value, "Needs Wi-Fi");
        assert_eq!(page.row().desc, crate::cheat_data::CONNECT);
        assert_eq!(page.status(), ("offline", 0, 0));
        std::fs::write(&source, bytes).unwrap();
        page.start();
        let mut menu = finish(&mut page).expect("a verified download replaces the explanation with codes");
        assert_eq!(menu.rows()[0].name, "Lives");
        assert_eq!(menu.download_status(), ("ready", 0, 0));
        assert!(menu.press(0));
        assert_eq!(crate::cheats::active_codes_in(&request.data, "gb", &request.rom, &request.roots), ["01"]);
        std::fs::remove_file(&source).unwrap();
        let offline = CheatMenu::optional(request);
        assert_eq!(offline.rows()[0].value, "On", "the next visit uses the downloaded pack offline");
    }

    #[test]
    fn installed_database_never_downloads_for_matching_or_unmatched_games() {
        let dir = TestDir::new("cheats-have-it");
        for packed in [false, true] {
            let mut request = request(&dir);
            let bytes = b"cheat0_desc = \"Lives\"\ncheat0_code = \"01\"\n";
            if packed {
                std::fs::write(dir.join("legacy.pack"), crate::cheat_pack::build(&[("Nintendo - Game Boy", vec![("Tetris.cht".into(), bytes.to_vec())])])).unwrap();
                request.roots = vec![dir.join("legacy.pack")];
            } else {
                let folder = dir.join("rocknix/Nintendo - Game Boy");
                std::fs::create_dir_all(&folder).unwrap();
                std::fs::write(folder.join("Tetris.cht"), bytes).unwrap();
                request.roots = vec![dir.join("rocknix")];
            }
            let mut matching = CheatMenu::optional(request.clone());
            matching.open();
            settle_menu(&mut matching);
            assert_eq!(matching.rows()[0].name, "Lives");
            request.rom = "Homebrew.gb".into();
            let mut unmatched = CheatMenu::optional(request);
            unmatched.open();
            settle_menu(&mut unmatched);
            assert_eq!(unmatched.rows()[0].name, "No cheats found for this game");
        }
    }

    #[test]
    fn first_upgrade_uses_previous_apps_pack_and_keeps_it_before_the_next_upgrade() {
        let dir = TestDir::new("cheats-first-upgrade");
        let pack = dir.join("app.previous/cores/cht.pack");
        std::fs::create_dir_all(pack.parent().unwrap()).unwrap();
        let bytes = crate::cheat_pack::build(&[("Nintendo - Game Boy", vec![("Tetris.cht".into(), b"cheat0_code = \"01\"\n".to_vec())])]);
        std::fs::write(&pack, &bytes).unwrap();
        let mut request = request(&dir);
        request.roots = vec![pack];
        let mut page = CheatMenu::optional(request);
        page.open();
        settle_menu(&mut page);
        assert_eq!(page.rows()[0].value, "Off");
        preserve_legacy(&dir).unwrap();
        std::fs::remove_dir_all(dir.join("app.previous")).unwrap();
        assert_eq!(std::fs::read(dir.join("storage/pocket-ui/cheats/cht.pack")).unwrap(), bytes);
    }

    fn settle_menu(menu: &mut CheatMenu) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while menu.downloading() && std::time::Instant::now() < until {
            menu.poll_download();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!menu.downloading());
    }

    #[test]
    fn opening_never_downloads_and_reopening_notices_extras_changes_without_losing_switches() {
        let dir = TestDir::new("cheats-explicit-get");
        let request = request(&dir);
        let bytes = crate::cheat_pack::build(&[("Nintendo - Game Boy", vec![("Tetris.cht".into(),
            b"cheat0_desc = \"Lives\"\ncheat0_code = \"01\"\n".to_vec())])]);
        let source = dir.join("source.pack");
        std::fs::write(&source, &bytes).unwrap();
        std::fs::write(&request.descriptor, serde_json::to_vec(&serde_json::json!({
            "version":"1", "url":format!("file://{}", source.display()),
            "size":bytes.len(), "sha256":crate::store::sha256_hex(&bytes)
        })).unwrap()).unwrap();
        let mut menu = CheatMenu::optional(request.clone());
        menu.open(); settle_menu(&mut menu);
        assert!(!request.destination.exists(), "opening the page must not fetch optional data");
        assert_eq!(menu.rows()[0].name, "Cheats");
        menu.press(0); settle_menu(&mut menu);
        assert_eq!(menu.rows()[0].name, "Lives");
        assert!(menu.press(0));
        menu.close();
        std::fs::remove_file(&request.destination).unwrap();
        menu.open(); settle_menu(&mut menu);
        assert_eq!(menu.rows()[0].name, "Cheats", "removal in Extras replaces cached codes with the Get row");
        std::fs::write(&request.destination, &bytes).unwrap();
        menu.open(); settle_menu(&mut menu);
        assert_eq!(menu.rows()[0].value, "On", "per-game preferences survive removal and reinstallation");
    }

    #[test]
    fn a_later_download_is_never_overwritten_with_a_legacy_pack() {
        let dir = TestDir::new("cheats-preserve-newer");
        std::fs::create_dir_all(dir.join("app/cores")).unwrap();
        std::fs::create_dir_all(dir.join("storage/pocket-ui/cheats")).unwrap();
        std::fs::write(dir.join("app/cores/cht.pack"), "old").unwrap();
        std::fs::write(dir.join("storage/pocket-ui/cheats/cht.pack"), "newer").unwrap();
        preserve_legacy(&dir).unwrap();
        assert_eq!(std::fs::read(dir.join("storage/pocket-ui/cheats/cht.pack")).unwrap(), b"newer");
    }

}
