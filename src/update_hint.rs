//! The quiet hint that a newer pocket-ui is out: one notice on the shelf, once for each version, with A going straight
//! to Settings → System → Updates. The rules are pure, so the shelf (`app_update_hint.rs`) and the tests share them.
//!
//! - **Where**: the same signed list the Updates page reads (`update::fetch_manifest`), looked at from the shelf some
//!   time after the start and when Wi-Fi is up, again every [`EVERY`] while the Pocket is running. Only `app` counts:
//!   the system image (`os`) is never announced from here, and neither are ports or the Store.
//! - **Once**: `update-hint.json` in the data folder remembers the version that was said ([`State::said`]). A version
//!   that was said is not said again, whether the player opened Updates or pressed B; the next version is a new hint.
//! - **Off**: `{"off": true}` in the same file stops the look altogether (no request is made then).
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
pub const EVERY: Duration = Duration::from_secs(12 * 3600);
/// After a look that failed (no route, the server not answering): sooner, but not hammering.
pub const RETRY: Duration = Duration::from_secs(3600);
/// How long to wait for the shelf to be free (a page or a notice is up) before asking again.
pub const AGAIN: Duration = Duration::from_secs(30);

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

    #[test]
    fn the_notice_names_the_version_and_the_way_there() {
        assert!(text("0.3.0").contains("0.3.0"));
        assert!(ACTIONS.contains("A ") && ACTIONS.contains("B ") && ACTIONS.contains("Updates"));
    }
}
