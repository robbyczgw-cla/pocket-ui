//! RetroAchievements' **Hardcore** (docs/user/achievements.md "Hardcore"): one switch for every game, on Settings >
//! RetroAchievements (kept with the account, `Account::hardcore`). In Hardcore the site counts an unlock only when the
//! game was played straight: so the host loads no save state (Load, Earlier, the shortcut, Continue's moment, the
//! Cabinet TV's hand-over), has no rewind or cheats, and lets the game pause only after it has run a
//! moment (rc_client's rule against frame stepping). The game's own saves (battery saves, memory cards) stay.
//!
//! - Continue in Hardcore starts the game afresh, from its in-game save, and the shelf and the card say so; the moment
//!   stays on the card (under Load once Hardcore is off again).
//! - Switching Hardcore on during a game starts the game again after asking (rc_client allows nothing else); off goes at
//!   once.
//! - A game RetroAchievements doesn't know has nothing to protect: its states work (`session::hardcore_applies`; a game
//!   known to have none from its tally starts as usual, [`at_start`]).
//!
//! The words here are what pocket-core, pocket-ui and the Android player show.

use std::path::Path;

use super::account::Account;
use super::snapshot::Summary;

/// The pause menu's line in place of Save, Load and Earlier.
pub const MENU_LINE: &str = "Hardcore: no save states, rewind or cheats. Save in the game before you quit.";
/// Over the game when Hardcore started it from its in-game save, though a moment was kept.
pub const STARTED_FRESH: &str = "Hardcore: started from the in-game save";
/// A Load, Earlier or the load shortcut in Hardcore.
pub const NO_LOAD: &str = "Loading states is off in Hardcore";
/// Save or the save shortcut in Hardcore.
pub const NO_SAVE: &str = "Save states are off in Hardcore";
pub const NO_REWIND: &str = "Rewind is off in Hardcore";
pub const NO_CHEATS: &str = "Cheats are off in Hardcore";
/// HOME again right after a pause (rc_client_can_pause): the game plays this moment first.
pub const PAUSE_SOON: &str = "Hardcore: the menu opens in a moment";
/// Switched on during a game: what the game does now.
pub const RESTARTED: &str = "Hardcore on: the game started again";
/// Switched off during a game.
pub const OFF_NOW: &str = "Hardcore off: save states, rewind and cheats are back";
/// The game card's line where "Continue: 2 h ago" would be.
pub const CARD_LINE: &str = "Hardcore: starts from the in-game save";

/// Whether pocket-ui offers Hardcore at all: off until RetroAchievements has approved pocket-ui as an emulator. Until
/// then the server answers every set with its "Unknown Emulator" warning and counts no hardcore unlock from pocket-ui
/// (RA-HARDCORE.md, 14.11.2026), so the switch would promise what the site won't do. With it off the Settings row and
/// the pause menu's row are hidden, no start is in Hardcore ([`at_start`]), and the guide says softcore only; the
/// code and its tests stay. The day RetroAchievements approves pocket-ui: true here, and the guide's section back
/// (docs/dev/hardcore-guide.md).
pub const OFFERED: bool = false;

/// The command pocket-ui sends pocket-core when Hardcore is switched during the game (`hardcore on` / `hardcore off`).
pub const COMMAND: &str = "hardcore";

/// pocket-ui's data folder for the shelf's question ([`skips_moments`]), set once when the app starts. Unset (pocket-core,
/// tests of other modules): the shelf's rules are as without Hardcore.
static SHELF_DATA: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// The app's data folder, for [`skips_moments`].
pub fn shelf_data(dir: &Path) {
    let _ = SHELF_DATA.set(dir.to_path_buf());
}

/// The shelf (`frozen::resume_files`): a start of this pocket-core game would be in Hardcore, so it doesn't continue
/// from its moment and its case stands closed. Reads the account (and the game's tally): off the UI thread.
pub fn skips_moments(system: &str, rom: &Path) -> bool {
    SHELF_DATA.get().is_some_and(|d| at_start(d, system, rom))
}

/// Whether the player chose Hardcore (an account signed in with the switch on).
pub fn chosen(data_dir: &Path) -> bool {
    Account::load(data_dir).is_some_and(|a| a.hardcore)
}

/// Whether a start of this game is in Hardcore: chosen, the console has achievements, and the game isn't known to have
/// none. A game played before without a set on the site (its tally says so) starts as usual, with its moment.
pub fn at_start(data_dir: &Path, system: &str, rom: &Path) -> bool {
    OFFERED && chosen(data_dir) && applies_to(data_dir, system, rom)
}

/// [`at_start`] without the account: the console has achievements and the game isn't known to have none.
pub fn applies_to(data_dir: &Path, system: &str, rom: &Path) -> bool {
    !rom.as_os_str().is_empty()
        && super::consoles::console_id(system).is_some()
        && !super::is_chd(rom)
        && Summary::load(data_dir, system, rom).is_none_or(|s| s.game_id != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn hardcore_starts_only_games_with_achievements_and_only_when_chosen() {
        let dir = TestDir::new("ra-hardcore-start");
        let rom = dir.join("Game.gb");
        std::fs::write(&rom, b"game").unwrap();
        assert!(!at_start(&dir, "gb", &rom), "not signed in");
        let mut a = Account { username: "Alex".into(), token: "Tk".into(), display_name: String::new(), hardcore: false };
        a.save(&dir).unwrap();
        assert!(!at_start(&dir, "gb", &rom), "signed in, switch off");
        a.hardcore = true;
        a.save(&dir).unwrap();
        // While pocket-ui doesn't offer Hardcore, a switch left on (an account file from a test build) changes nothing.
        assert_eq!(at_start(&dir, "gb", &rom), OFFERED);
        assert!(applies_to(&dir, "gb", &rom), "a game never seen: Hardcore until the site says it has no set");
        assert!(!applies_to(&dir, "ports", &rom), "a system without achievements");
        assert!(!applies_to(&dir, "psx", &dir.join("Disc.chd")), "a CHD can't be identified");
        Summary { game_id: 0, ..Default::default() }.save(&dir, "gb", &rom);
        assert!(!applies_to(&dir, "gb", &rom), "known to have no achievements: its moment is its own");
        Summary { game_id: 1234, total: 40, ..Default::default() }.save(&dir, "gb", &rom);
        assert!(applies_to(&dir, "gb", &rom));
        // The switch is kept with the account, and an account from before it reads as off.
        std::fs::write(Account::path(&dir), r#"{"username":"Alex","token":"Tk"}"#).unwrap();
        assert!(!chosen(&dir));
    }
}
