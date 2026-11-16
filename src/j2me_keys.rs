//! Phone games (J2ME) on a handheld without sticks. FreeJ2ME-Plus puts the D-pad on the phone's arrow keys and the number
//! keys 2 4 6 8 only on the left stick, 5 on its press (`freej2me_libretro.c`; no core option moves them). Most games
//! take the arrows, some read only the number keys: on an RG35XX Plus those could not be played.
//!
//! Runs on › D-pad (`emu_choice::RA_SETTINGS`, per game or console in system.cfg like every RetroArch row) picks
//! "Number keys 2 4 6 8": before the start pocket-ui writes RetroArch's per-game remap file for the game, the D-pad on
//! the left stick's four directions and Y on its press, so FreeJ2ME-Plus gets 2 4 6 8 and 5. RetroArch loads
//! `<remappings>/FreeJ2ME-Plus/<game>.rmp` by itself (`auto_remaps_enable`). Back on "Arrow keys" the file goes again,
//! but only a file pocket-ui wrote: a remap the player made in RetroArch's own menu stays.
//!
//! A is the main key. FreeJ2ME-Plus puts the phone's middle key (OK, fire) on RetroPad Y and 9 on A, so jumping in
//! Bounce Tales and dropping a bomb in 3D Bomberman were on a side button (RG35XX Plus, 14.11.2026; both games measured:
//! only OK and 5 do it). pocket-ui's core remap `FreeJ2ME-Plus.rmp` (`DEFAULT_REMAP`, written when the player has
//! none) swaps the two for every phone game: A is OK, Y is 9. The number keys' remap swaps them too.
use std::path::{Path, PathBuf};

/// The system.cfg key (`j2me["<game>"].j2mekeys`, `j2me.j2mekeys`).
/// The game card's one line for a phone game (`game_card`): its keys are no console's (pocket-ui's remap puts OK on A,
/// `DEFAULT_REMAP`; Select and Start are the two soft keys; the rest in docs/user/systems.md).
pub const CARD_LINE: &str = "A is OK, SELECT and START are the soft keys.";
pub const KEY: &str = "j2mekeys";
pub const ARROWS: &str = "arrows";
pub const NUMBERS: &str = "numbers";

/// RetroArch's remap folder on ROCKNIX (`input_remapping_directory = "~/remappings"`), the core's own under it.
pub const REMAPS: &str = "/storage/remappings/FreeJ2ME-Plus";

/// The remap: RetroPad D-pad → left analog (RetroArch's ids 16 X+, 17 X−, 18 Y+, 19 Y−), A → Y (1, OK), Y → L3 (14).
/// FreeJ2ME-Plus reads the left stick as 2 4 6 8, Y as OK and L3 as 5; every other button keeps its key. RetroArch
/// loads only the game's remap when there is one, so it carries `DEFAULT_REMAP`'s A too. The first line marks it as
/// pocket-ui's.
pub const REMAP: &str = "# pocket-ui: phone game D-pad on the number keys 2 4 6 8, A is OK, Y is 5 (Runs on > D-pad)\n\
input_player1_btn_a = \"1\"\n\
input_player1_btn_up = \"19\"\n\
input_player1_btn_down = \"18\"\n\
input_player1_btn_left = \"17\"\n\
input_player1_btn_right = \"16\"\n\
input_player1_btn_y = \"14\"\n";

/// RetroArch's remap for the core, read for every phone game without a remap of its own (`auto_remaps_enable`).
pub const CORE_REMAP: &str = "FreeJ2ME-Plus.rmp";

/// The default for every phone game: A sends RetroPad Y (1, FreeJ2ME's OK), Y sends A (8, its 9).
pub const DEFAULT_REMAP: &str = "# pocket-ui: phone games' main key OK (fire) on A, 9 on Y\n\
input_player1_btn_a = \"1\"\n\
input_player1_btn_y = \"8\"\n";

/// The number keys' remap before A became OK (14.11.2026): still pocket-ui's when RetroArch saved it again.
const OLD_REMAP: &str = "input_player1_btn_up = \"19\"\ninput_player1_btn_down = \"18\"\ninput_player1_btn_left = \"17\"\n\
input_player1_btn_right = \"16\"\ninput_player1_btn_y = \"14\"\n";

/// The button lines of a remap (`input_playerN_btn_…`), spaces taken out. RetroArch saves a remap again when the game
/// ends: its whole state, without pocket-ui's first line (RG35XX Plus, 14.11.2026), but with the same buttons.
fn buttons(text: &str) -> std::collections::BTreeSet<String> {
    text.lines()
        .map(|l| l.replace([' ', '\t'], ""))
        .filter(|l| l.starts_with("input_player") && l.contains("_btn_"))
        .collect()
}

/// A remap pocket-ui wrote: its first line, or, saved again by RetroArch, exactly the buttons of one of `versions`.
fn is_ours(text: &str, versions: &[&str]) -> bool {
    text.starts_with("# pocket-ui:") || versions.iter().any(|v| buttons(v) == buttons(text))
}

/// Before a FreeJ2ME start: the core's remap with A as OK, unless the player keeps a core remap of their own.
pub fn apply_default(dir: &Path) -> std::io::Result<()> {
    let file = dir.join(CORE_REMAP);
    match std::fs::read_to_string(&file) {
        Err(_) => {
            std::fs::create_dir_all(dir)?;
            crate::fsutil::write_atomic(&file, DEFAULT_REMAP.as_bytes())
        }
        Ok(t) if is_ours(&t, &[DEFAULT_REMAP]) && buttons(&t) != buttons(DEFAULT_REMAP) => crate::fsutil::write_atomic(&file, DEFAULT_REMAP.as_bytes()),
        Ok(_) => Ok(()),
    }
}

/// The game's remap file: RetroArch names it after the content file without its extension.
pub fn remap_file(dir: &Path, rom: &Path) -> Option<PathBuf> {
    Some(dir.join(format!("{}.rmp", rom.file_stem()?.to_str()?)))
}

/// Before a FreeJ2ME start: the game's remap as `numbers` asks. Never touches a remap pocket-ui didn't write.
pub fn apply(dir: &Path, rom: &Path, numbers: bool) -> std::io::Result<()> {
    let Some(file) = remap_file(dir, rom) else { return Ok(()) };
    let current = std::fs::read_to_string(&file).ok();
    let ours = current.as_deref().is_some_and(|t| is_ours(t, &[REMAP, OLD_REMAP]));
    match (numbers, current.is_some()) {
        (true, false) => {
            std::fs::create_dir_all(dir)?;
            crate::fsutil::write_atomic(&file, REMAP.as_bytes())
        }
        (true, true) if ours && current.as_deref().map(buttons) != Some(buttons(REMAP)) => crate::fsutil::write_atomic(&file, REMAP.as_bytes()),
        (false, true) if ours => std::fs::remove_file(&file),
        _ => Ok(()),
    }
}

/// The start's choice from system.cfg (the game's, else the console's, else arrows) and its remap, for `core`.
pub fn before_start(system: &str, core: &str, rom: &Path) {
    if !core.starts_with("freej2me") || crate::proc::mock() { return; }
    let cfg = crate::emu_choice::Cfg::read(Path::new(crate::emu_choice::SYSTEM_CFG));
    let Some(setting) = crate::emu_choice::RA_SETTINGS.iter().find(|s| s.key == KEY) else { return };
    let file = rom.file_name().and_then(|f| f.to_str());
    let numbers = crate::emu_choice::effective_value(&cfg, system, file, setting) == NUMBERS;
    if let Err(e) = apply_default(Path::new(REMAPS)) {
        eprintln!("j2me: default remap: {e}");
    }
    if let Err(e) = apply(Path::new(REMAPS), rom, numbers) {
        eprintln!("j2me: D-pad remap for {}: {e}", rom.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_keys_write_the_games_remap_and_arrows_take_it_away_again() {
        let dir = tempfile::tempdir().unwrap();
        let rom = Path::new("/storage/roms/j2me/Snake II.jar");
        let file = dir.path().join("Snake II.rmp");
        apply(dir.path(), rom, true).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("input_player1_btn_up = \"19\"") && text.contains("input_player1_btn_y = \"14\""), "{text}");
        apply(dir.path(), rom, false).unwrap();
        assert!(!file.exists(), "arrow keys: the remap is gone");
    }

    /// RG35XX Plus, 14.11.: jump (Bounce Tales) and bomb (3D Bomberman) are the phone's OK, which FreeJ2ME puts on Y.
    #[test]
    fn a_is_the_phones_ok_key_for_every_game_unless_the_player_has_a_core_remap() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(CORE_REMAP);
        apply_default(dir.path()).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("input_player1_btn_a = \"1\"") && text.contains("input_player1_btn_y = \"8\""), "A sends Y (OK), Y sends A (9): {text}");
        std::fs::write(&file, "# pocket-ui: older\ninput_player1_btn_a = \"0\"\n").unwrap();
        apply_default(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), DEFAULT_REMAP, "pocket-ui's own file is brought up to date");
        std::fs::write(&file, "input_player1_btn_b = \"8\"\n").unwrap();
        apply_default(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "input_player1_btn_b = \"8\"\n", "the player's own core remap stays");
        assert!(REMAP.contains("input_player1_btn_a = \"1\""), "on the number keys A is OK too: the game's remap replaces the core's");
    }

    /// RG35XX Plus, 14.11.: RetroArch saved the core remap again when the game ended, its whole state and without
    /// pocket-ui's first line. pocket-ui still knows it as its own: "Arrow keys" takes a saved number-keys remap away.
    #[test]
    fn a_remap_retroarch_saved_again_is_still_pocket_uis() {
        let dir = tempfile::tempdir().unwrap();
        let rom = Path::new("/storage/roms/j2me/Bounce.jar");
        let saved = |buttons: &str| format!("input_libretro_device_p1 = \"1\"\ninput_player1_analog_dpad_mode = \"1\"\n{buttons}input_remap_port_p1 = \"0\"\ninput_turbo_enable = \"true\"\n");
        let game = dir.path().join("Bounce.rmp");
        for numbers in [REMAP, OLD_REMAP] {
            let body: String = numbers.lines().filter(|l| !l.starts_with('#')).map(|l| format!("{l}\n")).collect();
            std::fs::write(&game, saved(&body)).unwrap();
            apply(dir.path(), rom, false).unwrap();
            assert!(!game.exists(), "arrow keys again: the saved number-keys remap goes");
        }
        let core = dir.path().join(CORE_REMAP);
        std::fs::write(&core, saved("input_player1_btn_a = \"1\"\ninput_player1_btn_y = \"8\"\n")).unwrap();
        let before = std::fs::read_to_string(&core).unwrap();
        apply_default(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(&core).unwrap(), before, "same buttons: left as RetroArch saved it");
        std::fs::write(&core, saved("input_player1_btn_a = \"0\"\n")).unwrap();
        apply_default(dir.path()).unwrap();
        assert!(std::fs::read_to_string(&core).unwrap().contains("btn_a = \"0\""), "other buttons: the player's, never touched");
    }

    #[test]
    fn a_remap_the_player_made_in_retroarch_is_never_touched() {
        let dir = tempfile::tempdir().unwrap();
        let rom = Path::new("/storage/roms/j2me/Bounce.jar");
        let file = dir.path().join("Bounce.rmp");
        std::fs::write(&file, "input_player1_btn_a = \"8\"\n").unwrap();
        apply(dir.path(), rom, true).unwrap();
        apply(dir.path(), rom, false).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "input_player1_btn_a = \"8\"\n");
    }

    #[test]
    fn the_dpad_row_is_only_on_phone_games() {
        let s = crate::emu_choice::RA_SETTINGS.iter().find(|s| s.key == KEY).unwrap();
        assert!(s.applies("j2me") && !s.applies("snes") && s.own);
        assert_eq!(s.unset, ARROWS, "without a choice phone games keep the arrows");
    }
}
