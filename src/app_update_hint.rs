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

/// The schedule and its worker are one state. A stopped Kids Mode schedule intentionally retains its candidate,
/// matching the old behavior; toggling automatic checks is what schedules again.
enum HintPhase {
    Stopped { reason: StopReason, pending: Option<String> },
    Waiting { at: Instant, pending: Option<String> },
    Checking { resume: Option<Instant> },
}

#[derive(Clone, Copy)]
enum StopReason { Off, Kids, Development, HostOwned, Mock }

impl HintPhase {
    /// Keep the worker/schedule phase visible even when policy suppresses its eventual notice.
    fn status(&self, off: bool, kids: bool) -> (&'static str, Option<&'static str>) {
        let phase = match self { Self::Stopped { .. } => "stopped", Self::Waiting { .. } => "waiting", Self::Checking { .. } => "checking" };
        let reason = if off { Some("off") } else if kids { Some("kids") } else {
            match self {
                Self::Checking { .. } => Some("request"),
                Self::Waiting { .. } => None,
                Self::Stopped { reason, .. } => Some(match reason {
                    StopReason::Off => "off", StopReason::Kids => "stopped-after-kids", StopReason::Development => "development",
                    StopReason::HostOwned => "host-owned", StopReason::Mock => "mock",
                }),
            }
        };
        (phase, reason)
    }
    fn when(&self) -> Option<Instant> { match self { Self::Stopped { .. } => None, Self::Waiting { at, .. } => Some(*at), Self::Checking { resume } => *resume } }
    fn checking(&self) -> bool { matches!(self, Self::Checking { .. }) }
    fn pending(&self) -> Option<&str> { match self { Self::Stopped { pending, .. } | Self::Waiting { pending, .. } => pending.as_deref(), Self::Checking { .. } => None } }
    fn reschedule(&mut self, due: Option<Instant>, reason: StopReason) {
        *self = if self.checking() { Self::Checking { resume: due } }
            else if let Some(at) = due { Self::Waiting { at, pending: None } }
            else { Self::Stopped { reason, pending: None } };
    }
}

/// What the App keeps of the hint. The persisted State format is unchanged.
pub(crate) struct UpdateHintUi {
    phase: HintPhase,
    pub(crate) state: State,
    failures: u32,
}

fn ineligible(state: &State) -> StopReason {
    if state.off { StopReason::Off }
    else if pocket_ui::device::current().host_owned() { StopReason::HostOwned }
    else if std::env::var_os("POCKETUI_MOCK").is_some() { StopReason::Mock }
    else { StopReason::Development }
}

impl UpdateHintUi {
    pub(crate) fn schedule_code(&self) -> &'static str { self.phase.status(false, false).0 }

    pub(crate) fn new(state: State, now: Instant) -> UpdateHintUi {
        let looks = update_hint::looks(update::release_build(), &state, pocket_ui::device::current().host_owned());
        let silent = !looks || std::env::var_os("POCKETUI_MOCK").is_some();
        let found = std::env::var_os("POCKETUI_MOCK").and_then(|_| std::env::var("POCKETUI_UPDATE_HINT").ok()).filter(|v| !v.is_empty());
        if let Some(version) = found.filter(|_| !state.off) {
            let pending = update::newer(&version, update::app_version()).then_some(version).filter(|v| state.said.as_deref() != Some(v));
            return UpdateHintUi { phase: HintPhase::Waiting { at: now, pending }, state, failures: 0 };
        }
        let phase = if silent { HintPhase::Stopped { reason: ineligible(&state), pending: None } }
            else { HintPhase::Waiting { at: now + update_hint::FIRST_LOOK, pending: None } };
        UpdateHintUi { phase, state, failures: 0 }
    }
}

#[cfg(test)]
mod characterisation {
    use super::*;

    #[test]
    fn suppression_never_hides_a_request_that_is_still_in_flight() {
        let now = Instant::now();
        let phase = HintPhase::Checking { resume: None };
        assert_eq!(phase.status(true, false), ("checking", Some("off")));
        assert_eq!(phase.status(false, false), ("checking", Some("request")));
        let phase = HintPhase::Waiting { at: now, pending: Some("99.0.0".into()) };
        assert_eq!(phase.status(false, true), ("waiting", Some("kids")), "the due tick has not stopped the schedule yet");
        let phase = HintPhase::Stopped { reason: StopReason::Kids, pending: Some("99.0.0".into()) };
        assert_eq!(phase.status(false, true), ("stopped", Some("kids")));
        assert_eq!(phase.status(false, false), ("stopped", Some("stopped-after-kids")));
    }

    #[test]
    fn changing_the_schedule_during_a_request_keeps_one_worker_in_flight() {
        let now = Instant::now();
        let mut phase = HintPhase::Checking { resume: Some(now) };
        phase.reschedule(None, StopReason::Off);
        assert!(phase.checking() && phase.when().is_none());
        phase.reschedule(Some(now + update_hint::AGAIN), StopReason::Development);
        assert!(phase.checking(), "On does not create another request while the old one is out");
        assert_eq!(phase.when(), Some(now + update_hint::AGAIN));
    }

    #[test]
    fn a_kids_stop_retains_its_candidate_until_the_switch_reschedules() {
        let now = Instant::now();
        let mut phase = HintPhase::Stopped { reason: StopReason::Kids, pending: Some("99.0.0".into()) };
        assert!(phase.when().is_none() && !phase.checking());
        assert_eq!(phase.pending(), Some("99.0.0"));
        phase.reschedule(Some(now), StopReason::Development);
        assert_eq!(phase.when(), Some(now));
        assert!(phase.pending().is_none());
    }

    #[test]
    fn an_off_notice_keeps_its_consumed_version_without_scheduling_or_pending_work() {
        let hint = UpdateHintUi::new(State { said: Some("99.0.0".into()), off: true }, Instant::now());
        assert_eq!(hint.state.said.as_deref(), Some("99.0.0"));
        assert!(hint.state.off);
        assert!(hint.phase.when().is_none() && hint.phase.pending().is_none() && !hint.phase.checking());
        assert_eq!(hint.failures, 0);
    }
}

/// Under `POCKETUI_MOCK` with `POCKETUI_UPDATE_HINT=<version>`: a list naming that pocket-ui, so the journeys reach "Install
/// pocket-ui <version>", its notes and the question after it without a network.
pub(crate) fn mock_manifest() -> Option<Manifest> {
    let version = std::env::var_os("POCKETUI_MOCK").and_then(|_| std::env::var("POCKETUI_UPDATE_HINT").ok()).filter(|v| !v.is_empty())?;
    let app = update::Release { version, url: "mock.tar.gz".into(), preferred_sources: Vec::new(), mirrors: Vec::new(), sha256: String::new(), size: 1, notes: "Mock notes: what is new in this version.".into() };
    Some(Manifest { format: 1, app: Some(app), ..Default::default() })
}

impl App {
    /// Called every turn of the loop; the time the next look is due, if a timer is needed for it.
    pub(crate) fn update_hint_tick(&mut self, now: Instant) -> Option<Instant> {
        let due = self.update_hint.phase.when()?;
        if self.running.is_some() || self.update_hint.phase.checking() {
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
            busy: self.update_hint.phase.checking() || self.updates.is_busy(),
            wifi: pocket_ui::status_corner::status().wifi,
            idle: now.duration_since(self.last_input) >= update_hint::IDLE_FOR,
            shelf_free: self.shelf_notice_free(),
            pending: self.update_hint.phase.pending().is_some(),
        };
        match update_hint::step(facts) {
            update_hint::Step::Stop => {
                let pending = self.update_hint.phase.pending().map(String::from);
                let reason = if self.update_hint.state.off { StopReason::Off } else { StopReason::Kids };
                self.update_hint.phase = HintPhase::Stopped { reason, pending };
                None
            }
            // No Wi-Fi: no timer; its change wakes the loop (`status_corner`), and this asks again.
            update_hint::Step::Pause => None,
            update_hint::Step::Retry => {
                let pending = self.update_hint.phase.pending().map(String::from);
                self.update_hint.phase = HintPhase::Waiting { at: now + update_hint::AGAIN, pending };
                self.update_hint.phase.when()
            }
            update_hint::Step::Say => {
                if let Some(version) = self.update_hint.phase.pending().map(String::from) {
                    self.say_update_ready(&version, now);
                }
                self.update_hint.phase.when()
            }
            update_hint::Step::Look => {
                self.update_hint.phase = HintPhase::Checking { resume: Some(due) };
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
        // Switched Off while the request was out: its answer is not used, and nothing more is asked.
        if self.update_hint.state.off {
            let pending = self.update_hint.phase.pending().map(String::from);
            self.update_hint.phase = HintPhase::Stopped { reason: StopReason::Off, pending };
            return;
        }
        match result {
            Ok(manifest) => {
                let pending = update_hint::due(&manifest, update::app_version(), &self.update_hint.state);
                // The Updates page opened later shows what was just read; it never replaces a look it is making itself.
                if !self.updates.is_busy() {
                    self.updates.manifest = Some(manifest);
                }
                self.update_hint.failures = 0;
                let at = if pending.is_some() { now } else { now + update_hint::EVERY };
                self.update_hint.phase = HintPhase::Waiting { at, pending };
            }
            Err(why) => {
                println!("update hint: {why}");
                self.update_hint.failures += 1;
                let pending = self.update_hint.phase.pending().map(String::from);
                self.update_hint.phase = HintPhase::Waiting { at: now + update_hint::retry_after(self.update_hint.failures), pending };
            }
        }
    }

    /// The notice itself, and the memory that it was said (so a restart, or B, does not bring it back).
    fn say_update_ready(&mut self, version: &str, now: Instant) {
        self.notice = Some(Notice::update_ready(version));
        self.update_hint.state.said = Some(version.to_string());
        self.update_hint.state.save(&self.data_dir);
        self.update_hint.phase = HintPhase::Waiting { at: now + update_hint::EVERY, pending: None };
        self.request_redraw();
    }

    /// A read-only explanation of the existing schedule; never schedules, saves or consumes a notice.
    pub(crate) fn update_notice_status(&self, now: Instant) -> (&'static str, &'static str, Option<&str>) {
        let hint = &self.update_hint;
        let version = hint.phase.pending().or(hint.state.said.as_deref());
        let shown = self.notice.as_ref().is_some_and(Notice::opens_updates)
            && self.where_now().covers.contains(&super::app_where::Cover::Notice);
        if shown { return ("shown", "none", version); }
        let (phase, policy) = hint.phase.status(hint.state.off, self.kids_live());
        if let Some(reason) = policy { return (phase, reason, version); }
        if hint.phase.pending().is_none() && hint.state.said.as_deref().is_some_and(|said|
            self.updates.manifest.as_ref().and_then(|m| m.app.as_ref()).is_none_or(|app| !update::newer(&app.version, said))) {
            return (phase, "already-said", version);
        }
        let reason = if self.running.is_some() { "game" }
            else if self.updates.is_busy() { "job" }
            else if hint.phase.when().is_some_and(|at| now < at) { if hint.failures > 0 { "backoff" } else { "scheduled" } }
            else if hint.phase.pending().is_some() { if self.shelf_notice_free() { "ready" } else { "shelf-covered" } }
            else if !pocket_ui::status_corner::status().wifi { "wifi-off" }
            else if now.duration_since(self.last_input) < update_hint::IDLE_FOR { "input" }
            else { "ready" };
        (phase, reason, version)
    }

    /// Settings → System → Updates → "Check automatically": On looks soon (Wi-Fi and idle permitting), Off drops the
    /// look, anything it found and a request's answer that is still on its way.
    pub(crate) fn set_update_auto(&mut self, on: bool, now: Instant) {
        self.update_hint.state.set_auto(on);
        self.update_hint.state.save(&self.data_dir);
        self.update_hint.failures = 0;
        let due = on.then_some(now + update_hint::AGAIN).filter(|_| self.update_hint_may_look());
        let reason = ineligible(&self.update_hint.state);
        self.update_hint.phase.reschedule(due, reason);
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
