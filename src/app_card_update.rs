//! Part of the pocket-ui binary: the App's side of `pocket_ui::update::card`, the update file copied onto the card for a
//! handheld without Wi-Fi (docs/user/updates.md "No Wi-Fi? Update from the card").
//!
//! At the start a thread looks at the parts of the card a computer shows (only when a file is there does it read more).
//! A newer update that passes every check is said once on the free shelf ("Update to pocket-ui 0.5.1 found on the card"):
//! A opens Settings → System → Updates and installs it, the very install a Wi-Fi update runs; with the battery low the
//! page asks first and says to plug in the charger. A file that can't be installed is said once with its reason. Each
//! file is said once (its name, size and time in `SEEN_FILE`); a file that isn't newer (the one just installed, when the
//! card's first part couldn't be written to delete it) is never said. Settings → System → Updates → "Look on the card"
//! looks again at any time and says what it found.

use std::path::Path;

use pocket_ui::notice::Notice;
use pocket_ui::update::{self, card::Look};

use super::{App, SettingsPage, UpdateAction, UpdateEvent, UpdateOutcome, UserEvent};

/// The last file said at the start.
const SEEN_FILE: &str = "card-update-seen";

fn seen(data: &Path) -> Option<String> {
    std::fs::read_to_string(data.join(SEEN_FILE)).ok().map(|s| s.trim().to_string())
}

/// Journeys (POCKETUI_MOCK): POCKETUI_CARD_UPDATE=<version> stands for a good update file of that version on the card
/// (none can be signed with the real key there); its install then fails on the missing file, which the journey sees.
pub(crate) fn mocked() -> Option<Look> {
    let version = std::env::var_os("POCKETUI_MOCK").and_then(|_| std::env::var("POCKETUI_CARD_UPDATE").ok()).filter(|v| !v.is_empty())?;
    let app = update::Release { url: format!("pocket-ui-{version}-aarch64.tar.gz"), version: version.clone(), preferred_sources: Vec::new(),
        mirrors: Vec::new(), sha256: "0".repeat(64), size: 1, notes: "Mock notes: what is new in this version.".into() };
    Some(Look::Offer(update::card::Found { file: "/nonexistent/pocket-ui-update-mock.zip".into(),
        manifest: update::Manifest { format: 1, app: Some(app), ..Default::default() }, id: format!("mock|{version}") }))
}

impl App {
    /// The look at the start, on a thread. Tests and previews (POCKETUI_MOCK) look only where POCKETUI_CARD_PLACES says.
    pub(crate) fn card_update_start(&mut self) {
        let mock = mocked();
        if mock.is_none() && std::env::var_os("POCKETUI_MOCK").is_some() && std::env::var_os("POCKETUI_CARD_PLACES").is_none() {
            return;
        }
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let look = mock.unwrap_or_else(update::card::look);
            if !matches!(look, Look::Nothing) {
                println!("update from the card: {}", look.message());
            }
            let _ = proxy.send_event(UserEvent::Update(UpdateEvent::CardLooked { at_start: true, look }));
        });
    }

    /// The start's answer: an offer or a refusal not said before waits for the free shelf; nothing else is said.
    pub(crate) fn card_update_found_at_start(&mut self, look: Look) {
        let id = match &look {
            Look::Offer(found) => found.id.clone(),
            Look::Refused { id, .. } => id.clone(),
            Look::Nothing | Look::NotNewer { .. } => return,
        };
        if seen(&self.data_dir).as_deref() == Some(id.as_str()) {
            return;
        }
        if let Look::Offer(found) = &look {
            if !self.updates.is_busy() {
                self.updates.manifest = Some(found.manifest.clone());
                self.updates.card = Some(found.clone());
            }
        }
        self.updates.card_pending = Some(look);
        self.card_update_tick();
    }

    /// Says the start's find once the shelf is free, never over a game, a page or another notice, not in Kids Mode.
    pub(crate) fn card_update_tick(&mut self) {
        if self.updates.card_pending.is_none() || self.running.is_some() || self.notice.is_some() || !self.shelf_notice_free() || self.kids_live() {
            return;
        }
        let Some(look) = self.updates.card_pending.take() else { return };
        let (id, notice) = match look {
            Look::Offer(found) => (found.id.clone(), Notice::card_update(found.version())),
            Look::Refused { id, why } => (id, Notice::new(why)),
            Look::Nothing | Look::NotNewer { .. } => return,
        };
        let _ = pocket_ui::fsutil::write_atomic(&self.data_dir.join(SEEN_FILE), id.as_bytes());
        self.notice = Some(notice);
        self.notice_painted = false;
        self.request_redraw();
    }

    /// A on "Update to … found on the card": the Updates page, installing; with the battery low it asks first (the page
    /// says to plug in the charger).
    pub(crate) fn install_card_update_from_notice(&mut self) {
        self.updates.sel = 0;
        self.settings_page = Some(SettingsPage::Updates);
        if self.updates.card.is_none() || self.updates.app_available().is_none() || self.updates.is_busy() {
            return;
        }
        if pocket_ui::update_hint::battery_warning(pocket_ui::status_corner::status().battery).is_some() {
            self.updates.set_confirmation(true);
        } else {
            self.update_action(UpdateAction::InstallNow);
        }
    }

    /// "Look on the card" answered: an update is asked about (Install now / Later), anything else is said on the page.
    pub(crate) fn card_update_looked(&mut self, look: Look) -> UpdateOutcome {
        match look {
            Look::Offer(found) => {
                self.updates.manifest = Some(found.manifest.clone());
                self.updates.card = Some(found);
                self.updates.message = None;
                self.updates.finish(UpdateOutcome::Checked);
                self.updates.set_confirmation(true);
                self.updates.sel = 0;
                UpdateOutcome::Checked
            }
            other => {
                self.updates.message = Some(other.message());
                UpdateOutcome::Checked
            }
        }
    }
}

#[cfg(test)]
mod file_source_tests {
    use super::*;
    #[test]
    fn file_source_rule_card_update_seen() {
        let d = tempfile::tempdir().unwrap(); let file = d.path().join(SEEN_FILE);
        for (label, bytes) in crate::file_faults::faults(b"reader-update.zip|123|456") {
            let _ = std::fs::remove_file(&file);
            if let Some(bytes) = bytes { std::fs::write(&file, bytes).unwrap(); }
            let id = seen(d.path());
            if label == "valid" { assert_eq!(id.as_deref(), Some("reader-update.zip|123|456")); }
            if label == "missing" { assert!(id.is_none()); }
        }
    }
}
