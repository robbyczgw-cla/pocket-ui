//! Part of the pocket-ui binary: the App's side of `pocket_ui::update_hint`.
//!
//! A newer pocket-ui is said once on the shelf ("pocket-ui 0.3.0 is ready", A for Settings → System → Updates, B to
//! close). The look is a request for the signed list on a thread of its own, started `FIRST_LOOK` after the start and
//! only with Wi-Fi up; with Wi-Fi off there is no timer at all, the turn of the loop that a change of Wi-Fi brings
//! asks again. During a game nothing happens either, and the notice waits until the shelf is free, so it is never over
//! a game, a page, the first start or another notice. Kids Mode, the Steam Deck and development builds never look.

use std::time::Instant;

use pocket_ui::gfx::Canvas;
use pocket_ui::notice::Notice;
use pocket_ui::palette::pal;
use pocket_ui::update::{self, Manifest};
use pocket_ui::update_hint::{self, State};

use super::{App, SettingsPage, UpdateEvent, UserEvent};

/// What the App keeps of the hint.
pub(crate) struct UpdateHintUi {
    /// When the next look (or the notice's turn) is due; None: nothing more to do this run.
    look: Option<Instant>,
    pub(crate) state: State,
    /// A request is out.
    checking: bool,
    /// Looks in a row that failed (`update_hint::retry_after`).
    failures: u32,
    /// The version found and not said yet: it waits for a free shelf.
    pending: Option<String>,
}

impl UpdateHintUi {
    pub(crate) fn new(state: State, now: Instant) -> UpdateHintUi {
        // A mock run (journeys, snapshots), a development build and a device whose OS owns updates never look.
        let looks = update_hint::looks(update::release_build(), &state, pocket_ui::device::current().host_owned());
        let silent = !looks || std::env::var_os("POCKETUI_MOCK").is_some();
        // The journey tests' stand-in for a list that names a newer version (a mock run asks no server).
        let found = std::env::var_os("POCKETUI_MOCK").and_then(|_| std::env::var("POCKETUI_UPDATE_HINT").ok()).filter(|v| !v.is_empty());
        if let Some(version) = found.filter(|_| !state.off) {
            return UpdateHintUi { look: Some(now), pending: update::newer(&version, update::app_version()).then_some(version).filter(|v| state.said.as_deref() != Some(v)), state, checking: false, failures: 0 };
        }
        UpdateHintUi { look: (!silent).then_some(now + update_hint::FIRST_LOOK), state, checking: false, failures: 0, pending: None }
    }
}

/// Under `POCKETUI_MOCK` with `POCKETUI_UPDATE_HINT=<version>`: a list naming that pocket-ui, so the journeys reach "Install
/// pocket-ui <version>", its notes and the question after it without a network.
pub(crate) fn mock_manifest() -> Option<Manifest> {
    let version = std::env::var_os("POCKETUI_MOCK").and_then(|_| std::env::var("POCKETUI_UPDATE_HINT").ok()).filter(|v| !v.is_empty())?;
    let app = update::Release { version, url: "mock.tar.gz".into(), mirrors: Vec::new(), sha256: String::new(), size: 1, notes: "Mock notes: what is new in this version.".into() };
    Some(Manifest { format: 1, app: Some(app), ..Default::default() })
}

impl App {
    /// Called every turn of the loop; the time the next look is due, if a timer is needed for it.
    pub(crate) fn update_hint_tick(&mut self, now: Instant) -> Option<Instant> {
        let due = self.update_hint.look?;
        if self.running.is_some() || self.update_hint.checking {
            return None;
        }
        if now < due {
            return Some(due);
        }
        // A page of Updates that is busy with its own download or look is left alone.
        let facts = update_hint::Facts {
            auto: !self.update_hint.state.off,
            game: self.running.is_some(),
            kids: self.kids_live(),
            busy: self.update_hint.checking || self.updates.busy.is_some(),
            wifi: pocket_ui::status_corner::status().wifi,
            idle: now.duration_since(self.last_input) >= update_hint::IDLE_FOR,
            shelf_free: self.shelf_notice_free(),
            pending: self.update_hint.pending.is_some(),
        };
        match update_hint::step(facts) {
            update_hint::Step::Stop => {
                self.update_hint.look = None;
                None
            }
            // No Wi-Fi: no timer; its change wakes the loop (`status_corner`), and this asks again.
            update_hint::Step::Pause => None,
            update_hint::Step::Retry => {
                self.update_hint.look = Some(now + update_hint::AGAIN);
                self.update_hint.look
            }
            update_hint::Step::Say => {
                if let Some(version) = self.update_hint.pending.clone() {
                    self.say_update_ready(&version, now);
                }
                self.update_hint.look
            }
            update_hint::Step::Look => {
                self.update_hint.checking = true;
                let (url, proxy) = (self.manifest_url(), self.proxy.clone());
                std::thread::spawn(move || {
                    let _ = proxy.send_event(UserEvent::Update(UpdateEvent::Looked(update::fetch_manifest(&url))));
                });
                None
            }
        }
    }

    /// The list came back: remember it for the Updates page, and find out whether a version is new to the player.
    pub(crate) fn update_hint_looked(&mut self, result: Result<Manifest, String>, now: Instant) {
        self.update_hint.checking = false;
        // Switched Off while the request was out: its answer is not used, and nothing more is asked.
        if self.update_hint.state.off {
            self.update_hint.look = None;
            return;
        }
        match result {
            Ok(manifest) => {
                self.update_hint.pending = update_hint::due(&manifest, update::app_version(), &self.update_hint.state);
                // The Updates page opened later shows what was just read; it never replaces a look it is making itself.
                if self.updates.busy.is_none() {
                    self.updates.manifest = Some(manifest);
                }
                self.update_hint.failures = 0;
                self.update_hint.look = Some(if self.update_hint.pending.is_some() { now } else { now + update_hint::EVERY });
            }
            Err(why) => {
                println!("update hint: {why}");
                self.update_hint.failures += 1;
                self.update_hint.look = Some(now + update_hint::retry_after(self.update_hint.failures));
            }
        }
    }

    /// The notice itself, and the memory that it was said (so a restart, or B, does not bring it back).
    fn say_update_ready(&mut self, version: &str, now: Instant) {
        self.notice = Some(Notice::update_ready(version));
        self.update_hint.pending = None;
        self.update_hint.state.said = Some(version.to_string());
        self.update_hint.state.save(&self.data_dir);
        self.update_hint.look = Some(now + update_hint::EVERY);
        self.request_redraw();
    }

    /// Settings → System → Updates → "Check automatically": On looks soon (Wi-Fi and idle permitting), Off drops the
    /// look, anything it found and a request's answer that is still on its way.
    pub(crate) fn set_update_auto(&mut self, on: bool, now: Instant) {
        self.update_hint.state.set_auto(on);
        self.update_hint.state.save(&self.data_dir);
        self.update_hint.failures = 0;
        self.update_hint.pending = None;
        self.update_hint.look = on.then_some(now + update_hint::AGAIN).filter(|_| self.update_hint_may_look());
    }

    /// Whether this build and device ever look (not a development build, not a Deck, not a mock run).
    fn update_hint_may_look(&self) -> bool {
        update_hint::looks(update::release_build(), &update_hint::State::default(), pocket_ui::device::current().host_owned()) && std::env::var_os("POCKETUI_MOCK").is_none()
    }

    /// A newer pocket-ui is known (from the quiet look or the page): the dot on Settings → System and its Updates row.
    pub(crate) fn update_dot(&self) -> bool {
        self.updates.manifest.as_ref().and_then(|m| m.app.as_ref()).is_some_and(|a| update::newer(&a.version, update::app_version()))
    }

    /// The dot, `after` the end of a name that ends at `x`, centred on `mid`.
    pub(crate) fn paint_update_dot(c: &mut Canvas, x: i32, mid: i32, font: u32) {
        let d = (pocket_ui::type_scale::px(font) as i32 * 2 / 5).max(10);
        c.round_rect(x + d, mid - d / 2, d, d, d / 2, pal().amber);
    }

    /// Whether the switch is On.
    pub(crate) fn update_auto(&self) -> bool {
        !self.update_hint.state.off
    }

    /// A on the notice: gone, and the Updates page is up (B from there goes back through Settings to the shelf).
    pub(crate) fn open_updates_from_notice(&mut self) {
        self.updates.sel = 0;
        self.settings_page = Some(SettingsPage::Updates);
    }
}
