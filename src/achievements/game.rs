//! pocket-core's side of achievements, kept here so the game loop (src/bin/pocket_core.rs) only calls three things:
//! [`start`] after the core loaded the game, `Session::frame` after each emulated frame, and [`show`] with what the
//! session has to say, which goes to the HUD.

use std::path::Path;

use super::account::Account;
use super::http::{self, Curl};
use super::session::{Config, Notice, Session};
use super::snapshot::SNAPSHOT_FILE;
use crate::libretro::Core;
use crate::overlay::Hud;

/// Starts achievements for this game when an account is signed in and the console has them. Never blocks: hashing,
/// signing in and loading the set run on the session's threads. None otherwise (nothing runs then, nothing costs).
pub fn start(data_dir: &Path, system: &str, rom: &Path, core: &Core) -> Option<Session> {
    // The last game's list goes, so the pause menu never shows one that isn't this session's.
    let _ = std::fs::remove_file(SNAPSHOT_FILE);
    let account = Account::load(data_dir)?;
    let console = super::consoles::console_id(system)?;
    if rom.as_os_str().is_empty() {
        return None;
    }
    let agent = http::user_agent(Some((&core.name, &core.version)));
    let config = Config {
        data_dir: data_dir.to_path_buf(),
        account,
        system: system.to_string(),
        rom: rom.to_path_buf(),
        console,
        core_name: core.name.clone(),
        snapshot: SNAPSHOT_FILE.into(),
        verbose: std::env::var_os("POCKET_CORE_TRACE").is_some(),
    };
    Session::start(config, core.achievement_memory(), Box::new(Curl::new(agent)))
}

/// The session's news on the HUD: an unlock card (with its badge once it arrives), a line when the set is ready,
/// when the game is mastered, and when the account needs a new sign-in.
pub fn show(notices: Vec<Notice>, hud: &mut Hud) {
    for n in notices {
        match n {
            Notice::Unlocked { id, title, points, badge } => hud.achievement(id, &title, points, &badge),
            Notice::Badge { name, image } => hud.achievement_badge(&name, image.as_ref()),
            Notice::Ready { unlocked, total } if total > 0 => hud.toast(format!("Achievements: {unlocked} of {total}")),
            Notice::Ready { .. } => {}
            Notice::Completed { .. } => hud.toast("Every achievement of this game unlocked"),
            Notice::SignInAgain => hud.toast("RetroAchievements: sign in again in Settings"),
        }
    }
}
