//! Phone games (J2ME): A is the main key. FreeJ2ME-Plus puts the phone's middle key (OK, fire) on RetroPad Y and 9 on
//! A, so jumping in Bounce Tales and dropping a bomb in 3D Bomberman were on a side button (RG35XX Plus, 14.11.2026;
//! both games measured: only OK and 5 do it). Before a FreeJ2ME start pocket-ui writes RetroArch's core remap
//! `<remappings>/FreeJ2ME-Plus/FreeJ2ME-Plus.rmp` (`DEFAULT_REMAP`: A is OK, Y is 9) when the player has none of their
//! own. RetroArch loads it by itself (`auto_remaps_enable`) for every phone game without a remap of its own.
//! (0.4.2 port: main's module also has Runs on › D-pad "Number keys 2 4 6 8".)
use std::path::Path;

/// RetroArch's remap folder on ROCKNIX (`input_remapping_directory = "~/remappings"`), the core's own under it.
pub const REMAPS: &str = "/storage/remappings/FreeJ2ME-Plus";

/// RetroArch's remap for the core, read for every phone game without a remap of its own.
pub const CORE_REMAP: &str = "FreeJ2ME-Plus.rmp";

/// The default for every phone game: A sends RetroPad Y (1, FreeJ2ME's OK), Y sends A (8, its 9).
pub const DEFAULT_REMAP: &str = "# pocket-ui: phone games' main key OK (fire) on A, 9 on Y\n\
input_player1_btn_a = \"1\"\n\
input_player1_btn_y = \"8\"\n";

/// The button lines of a remap (`input_playerN_btn_…`), spaces taken out. RetroArch saves a remap again when the game
/// ends: its whole state, without pocket-ui's first line (RG35XX Plus, 14.11.2026), but with the same buttons.
fn buttons(text: &str) -> std::collections::BTreeSet<String> {
    text.lines()
        .map(|l| l.replace([' ', '\t'], ""))
        .filter(|l| l.starts_with("input_player") && l.contains("_btn_"))
        .collect()
}

/// The core's remap with A as OK, unless the player keeps a core remap of their own: pocket-ui's file is known by its
/// first line or, saved again by RetroArch, by its buttons.
pub fn apply_default(dir: &Path) -> std::io::Result<()> {
    let file = dir.join(CORE_REMAP);
    match std::fs::read_to_string(&file) {
        Err(_) => {
            std::fs::create_dir_all(dir)?;
            crate::fsutil::write_atomic(&file, DEFAULT_REMAP.as_bytes())
        }
        Ok(t) if t.starts_with("# pocket-ui:") && buttons(&t) != buttons(DEFAULT_REMAP) => {
            crate::fsutil::write_atomic(&file, DEFAULT_REMAP.as_bytes())
        }
        Ok(_) => Ok(()),
    }
}

/// Before a FreeJ2ME start.
pub fn before_start(core: &str) {
    if !core.starts_with("freej2me") || crate::proc::mock() {
        return;
    }
    if let Err(e) = apply_default(Path::new(REMAPS)) {
        eprintln!("j2me: default remap: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RG35XX Plus, 14.11.: jump (Bounce Tales) and bomb (3D Bomberman) are the phone's OK, which FreeJ2ME puts on Y.
    #[test]
    fn a_is_the_phones_ok_key_for_every_game_unless_the_player_has_a_core_remap() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(CORE_REMAP);
        apply_default(dir.path()).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("input_player1_btn_a = \"1\"") && text.contains("input_player1_btn_y = \"8\""), "A sends Y (OK), Y sends A (9): {text}");
        // As RetroArch saves it again when the game ends: same buttons, left as it is.
        let saved = "input_libretro_device_p1 = \"1\"\ninput_player1_btn_a = \"1\"\ninput_player1_btn_y = \"8\"\ninput_turbo_enable = \"true\"\n";
        std::fs::write(&file, saved).unwrap();
        apply_default(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);
        std::fs::write(&file, "input_player1_btn_b = \"8\"\n").unwrap();
        apply_default(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "input_player1_btn_b = \"8\"\n", "the player's own core remap stays");
    }
}
