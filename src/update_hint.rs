//! The quiet hint that a newer pocket-ui is out: one notice on the shelf, once for each version, with A going straight
//! to Settings → System → Updates. The rules are pure, so the shelf (`app_update_hint.rs`) and the tests share them.
//!
//! - **Where**: the same signed list the Updates page reads (`update::fetch_manifest`), looked at from the shelf some
//!   time after the start and when Wi-Fi is up, again every [`EVERY`] while the Pocket is running. Only `app` counts:
//!   the system image (`os`) is never announced from here, and neither are ports or the Store.
//! - **Once**: `update-hint.json` in the data folder remembers the version that was said ([`State::said`]). A version
//!   that was said is not said again, whether the player opened Updates or pressed B; the next version is a new hint.
//! - **Off**: Settings → System → Updates → "Check automatically" (`{"off": true}` in the same file) stops the look
//!   altogether: no request is made, nothing is announced, whatever was found before is forgotten ([`State::set_auto`]).
//! - **Calm**: a look is at most once a day ([`EVERY`], plus the one after the start), only when the shelf has been
//!   untouched for a moment ([`IDLE_FOR`]), and a failed one is asked again later and later ([`retry_after`]).
//! - **Only release builds** ([`looks`]): a build from main or a branch carries Cargo.toml's version, older than the
//!   released one, and would announce that release over the build being tested (06.11., RG35XX Plus).
//! - **Not where nobody should see it**: not over a game, not on a device whose OS owns updates (the Steam Deck), not
//!   in Kids Mode, not over a page or another notice (the shelf asks `shelf_notice_free`).

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::update::{newer, Manifest};

/// The memory, in the data folder.
pub const FILE: &str = "update-hint.json";

/// After the start: Wi-Fi and the time server have had their go (the clock check waits 45 s for the same reason).
pub const FIRST_LOOK: Duration = Duration::from_secs(60);
/// Between two looks while the Pocket keeps running, after one that found nothing new or that said its hint.
pub const EVERY: Duration = Duration::from_secs(24 * 3600);
/// How long to wait for the shelf to be free (a page or a notice is up) before asking again.
pub const AGAIN: Duration = Duration::from_secs(30);
/// How long the buttons must have been left alone before a look starts (it is a thread, but the Pocket is in a hand).
pub const IDLE_FOR: Duration = Duration::from_secs(10);
/// Below this the install asks for the charger first, unless it is plugged in.
pub const LOW_BATTERY: u8 = 20;

/// After `failures` looks in a row that failed (no route, the server not answering): an hour, then two, four, eight,
/// never more than a day; one that works starts over.
pub fn retry_after(failures: u32) -> Duration {
    Duration::from_secs(3600 << failures.saturating_sub(1).min(4)).min(EVERY)
}

/// Whether installing should first suggest the charger: under [`LOW_BATTERY`] and not charging. An unknown battery
/// (a Deck, the desktop) never warns.
pub fn low_battery(battery: Option<crate::status_corner::Battery>) -> bool {
    battery.is_some_and(|b| !b.charging && b.percent < LOW_BATTERY)
}

/// The line under "Install now / Later" when the battery is low and not charging: the charger first. Nothing otherwise.
pub fn battery_warning(battery: Option<crate::status_corner::Battery>) -> Option<String> {
    battery.filter(|b| low_battery(Some(*b))).map(|b| format!("The battery is at {}%. Plug in the charger first, so it cannot run out during the update.", b.percent))
}

/// What the shelf knows when its clock says a look or a notice is due.
#[derive(Clone, Copy, Debug, Default)]
pub struct Facts {
    /// The switch (`!State::off`).
    pub auto: bool,
    pub game: bool,
    pub kids: bool,
    /// A request is out, or the Updates page is busy with its own download or look.
    pub busy: bool,
    pub wifi: bool,
    /// The buttons have been left alone for [`IDLE_FOR`].
    pub idle: bool,
    /// No page, card or notice is over the shelf (`App::shelf_notice_free`).
    pub shelf_free: bool,
    /// A newer version was found and not said yet.
    pub pending: bool,
}

/// What to do now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing more this run (Off, Kids Mode).
    Stop,
    /// Nothing now and no timer: a game ending, Wi-Fi coming up or the work finishing wakes the loop again.
    Pause,
    /// Ask again in [`AGAIN`].
    Retry,
    /// Put the notice up.
    Say,
    /// Ask the signed list, on a thread.
    Look,
}

/// The one decision, for a look or notice that is due.
pub fn step(f: Facts) -> Step {
    if !f.auto || f.kids {
        return Step::Stop;
    }
    if f.game || f.busy {
        return Step::Pause;
    }
    if f.pending {
        return if f.shelf_free { Step::Say } else { Step::Retry };
    }
    if !f.wifi {
        return Step::Pause;
    }
    if f.idle { Step::Look } else { Step::Retry }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// The version that was last said on the shelf.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
    /// The look is switched off by hand.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub off: bool,
}

impl State {
    pub fn load(data: &Path) -> State {
        crate::fsutil::read_json_or_backup(&data.join(FILE)).unwrap_or_default()
    }

    pub fn save(&self, data: &Path) {
        crate::fsutil::write_json_soon(&data.join(FILE), self);
    }

    /// The switch in Updates: On looks, Off asks nothing and says nothing.
    pub fn set_auto(&mut self, on: bool) {
        self.off = !on;
    }
}

/// Whether the shelf looks for a newer version at all: only a release build, not switched off, not on a device whose
/// OS owns updates.
pub fn looks(release_build: bool, state: &State, host_owned: bool) -> bool {
    release_build && !state.off && !host_owned
}

/// The version to say: the list's `app` when it is later than the running one and not said yet.
pub fn due(manifest: &Manifest, current: &str, state: &State) -> Option<String> {
    let app = manifest.app.as_ref()?;
    (newer(&app.version, current) && !state.said.as_deref().is_some_and(|said| !newer(&app.version, said))).then(|| app.version.clone())
}

/// What the notice says (the first line; the last line is [`ACTIONS`]).
pub fn text(version: &str) -> String {
    format!("pocket-ui {version} is ready. Your games and settings stay.")
}

/// The notice's last line: how it goes away, and the one press to the page.
pub const ACTIONS: &str = "Press A for Settings › System › Updates, B to close.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use crate::update::Release;

    fn list(app: Option<&str>) -> Manifest {
        let release = |v: &str| Release { version: v.into(), url: format!("pocket-ui-{v}-aarch64.tar.gz"), mirrors: Vec::new(), sha256: String::new(), size: 1, notes: String::new() };
        Manifest { format: 1, app: app.map(release), os: Some(release("20261201")), ..Default::default() }
    }

    /// 06.11.: a main build on the RG35XX Plus showed "pocket-ui 0.2.2 is ready" during a test.
    #[test]
    fn only_a_release_build_looks() {
        let fresh = State::default();
        assert!(!looks(false, &fresh, false), "a development build never says a release is ready");
        assert!(looks(true, &fresh, false));
        assert!(!looks(true, &State { off: true, ..State::default() }, false));
        assert!(!looks(true, &fresh, true), "the Steam Deck's OS owns updates");
    }

    #[test]
    fn a_later_version_is_said_and_the_running_or_an_older_one_is_not() {
        let fresh = State::default();
        assert_eq!(due(&list(Some("0.3.0")), "0.2.0", &fresh).as_deref(), Some("0.3.0"));
        assert_eq!(due(&list(Some("0.10.0")), "0.9.3", &fresh).as_deref(), Some("0.10.0"), "versions compare by number");
        assert_eq!(due(&list(Some("0.2.0")), "0.2.0", &fresh), None, "nothing new");
        assert_eq!(due(&list(Some("0.1.14")), "0.2.0", &fresh), None, "an old list offers no way back");
        assert_eq!(due(&list(None), "0.2.0", &fresh), None, "a list with only the system in it says nothing here");
    }

    #[test]
    fn each_version_is_said_once() {
        let said = State { said: Some("0.3.0".into()), off: false };
        assert_eq!(due(&list(Some("0.3.0")), "0.2.0", &said), None, "said already, whatever the player did with it");
        assert_eq!(due(&list(Some("0.3.1")), "0.2.0", &said).as_deref(), Some("0.3.1"), "the next version is a new hint");
        assert_eq!(due(&list(Some("0.3.0")), "0.2.0", &State { said: Some("0.4.0".into()), off: false }), None, "a list gone back is quiet");
    }

    #[test]
    fn the_memory_survives_a_restart_and_a_broken_file_is_a_fresh_one() {
        let dir = TestDir::new("update-hint");
        assert_eq!(State::load(&dir), State::default(), "no file: nothing said, not off");
        let state = State { said: Some("0.3.0".into()), off: true };
        state.save(&dir);
        crate::fsutil::settle();
        assert_eq!(State::load(&dir), state);
        // A file torn in the middle: the copy the card writer keeps beside it, or nothing said; never a stop.
        std::fs::write(dir.join(FILE), "{ not json").unwrap();
        let loaded = State::load(&dir);
        assert!(loaded == state || loaded == State::default(), "{loaded:?}");
        let fresh = TestDir::new("update-hint-torn");
        std::fs::write(fresh.join(FILE), "{ not json").unwrap();
        assert_eq!(State::load(&fresh), State::default(), "a torn file with no copy is a fresh start");
    }

    fn facts() -> Facts {
        Facts { auto: true, wifi: true, idle: true, shelf_free: true, ..Facts::default() }
    }

    #[test]
    fn a_look_needs_wifi_an_idle_shelf_and_no_game() {
        assert_eq!(step(facts()), Step::Look);
        assert_eq!(step(Facts { wifi: false, ..facts() }), Step::Pause, "no Wi-Fi: no look, no timer");
        assert_eq!(step(Facts { game: true, ..facts() }), Step::Pause, "never over a game");
        assert_eq!(step(Facts { busy: true, ..facts() }), Step::Pause, "the Updates page's own work is left alone");
        assert_eq!(step(Facts { idle: false, ..facts() }), Step::Retry, "buttons are being pressed: a little later");
        assert_eq!(step(Facts { kids: true, ..facts() }), Step::Stop);
    }

    #[test]
    fn the_notice_waits_for_a_free_shelf_and_never_for_a_game() {
        let found = Facts { pending: true, ..facts() };
        assert_eq!(step(found), Step::Say);
        assert_eq!(step(Facts { shelf_free: false, ..found }), Step::Retry, "a page or another notice is up");
        assert_eq!(step(Facts { game: true, ..found }), Step::Pause, "not during a game");
        assert_eq!(step(Facts { wifi: false, ..found }), Step::Say, "the news was found already: Wi-Fi is not needed to say it");
    }

    #[test]
    fn off_asks_nothing_and_says_nothing() {
        let off = Facts { auto: false, ..facts() };
        assert_eq!(step(off), Step::Stop);
        assert_eq!(step(Facts { pending: true, ..off }), Step::Stop, "even a version found before the switch was turned");
        assert_eq!(step(Facts { game: true, wifi: false, idle: false, ..off }), Step::Stop);
    }

    /// Three days on a Pocket that stays on, the way the App's timer runs it: one look after the start, then one a day;
    /// switched Off, none at all.
    #[test]
    fn a_look_once_a_day_and_none_when_off() {
        let looks = |auto: bool, days: u64| {
            let (mut now, mut due, mut looks) = (FIRST_LOOK, FIRST_LOOK, 0);
            while now < Duration::from_secs(days * 86400) {
                if now >= due {
                    match step(Facts { auto, ..facts() }) {
                        Step::Look => {
                            looks += 1;
                            due = now + EVERY;
                        }
                        Step::Retry => due = now + AGAIN,
                        Step::Stop | Step::Pause => break,
                        Step::Say => unreachable!(),
                    }
                }
                now += Duration::from_secs(30);
            }
            looks
        };
        assert_eq!(looks(true, 3), 3, "at 60 s after the start, then every 24 hours");
        assert_eq!(looks(false, 3), 0, "Off: not one request in three days");
    }

    #[test]
    fn a_failed_look_waits_longer_each_time_but_never_more_than_a_day() {
        let hours: Vec<u64> = (1..=8).map(|n| retry_after(n).as_secs() / 3600).collect();
        assert_eq!(hours, [1, 2, 4, 8, 16, 16, 16, 16]);
        assert!(retry_after(0) >= Duration::from_secs(3600) && retry_after(u32::MAX) <= EVERY);
    }

    #[test]
    fn under_a_fifth_and_off_the_charger_the_charger_comes_first() {
        use crate::status_corner::Battery;
        let battery = |percent, charging| Some(Battery { percent, charging });
        assert!(low_battery(battery(19, false)) && battery_warning(battery(19, false)).is_some_and(|w| w.contains("19%") && w.contains("charger")));
        assert!(!low_battery(battery(20, false)), "20 % is enough");
        assert!(!low_battery(battery(5, true)) && battery_warning(battery(5, true)).is_none(), "on the charger it goes ahead");
        assert!(!low_battery(None) && battery_warning(None).is_none(), "no battery (a Deck, the desktop): no warning");
    }

    #[test]
    fn the_switch_is_remembered_and_turns_the_look_off() {
        let dir = TestDir::new("update-hint-switch");
        let mut state = State { said: Some("0.3.0".into()), off: false };
        assert!(looks(true, &state, false));
        state.set_auto(false);
        state.save(&dir);
        crate::fsutil::settle();
        let back = State::load(&dir);
        assert!(back.off && back.said.as_deref() == Some("0.3.0"), "Off survives a restart and keeps what was said: {back:?}");
        assert!(!looks(true, &back, false));
        let mut on = back;
        on.set_auto(true);
        assert!(looks(true, &on, false) && !on.off);
    }

    #[test]
    fn the_notice_names_the_version_and_the_way_there() {
        assert!(text("0.3.0").contains("0.3.0"));
        assert!(ACTIONS.contains("A ") && ACTIONS.contains("B ") && ACTIONS.contains("Updates"));
    }
}
