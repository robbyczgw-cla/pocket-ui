//! D-pad as left stick for N64, PSP and Dreamcast games on ROCKNIX's own emulators. On the H700 handhelds these systems
//! stay on ROCKNIX (`hostsys::H700_STAYS_ON_ROCKNIX`), and exactly those handhelds (RG35XX Plus/SP/2024, RG28XX, RG34XX…)
//! have no sticks: Mario 64 could not walk. pocket-core does it for its own games (`controls::dpad_to_stick`); this does
//! the same for ROCKNIX's defaults there (es_systems.cfg of the H700 image 20261001): RetroArch's Mupen64Plus-Next for
//! N64, RetroArch's Flycast for Dreamcast, standalone PPSSPP for PSP.
//!
//! Runs on › D-pad (`emu_choice::RA_SETTINGS`, key `dpadstick`, per game or console in system.cfg): Automatic (the left
//! stick on a handheld without sticks), Left stick, or D-pad.
//! - RetroArch: before the start pocket-ui writes the game's remap `<remappings>/<core>/<game>.rmp` with the D-pad on the
//!   left stick's four directions (RetroArch's ids 16 X+, 17 X−, 18 Y+, 19 Y−; the same remap `j2me_keys` uses and that
//!   was played on an RG35XX Plus). RetroArch loads it by itself (`auto_remaps_enable`). Never when the player keeps a
//!   remap of their own for the game or the core; pocket-ui's own file goes again when the choice is D-pad.
//! - Standalone PPSSPP: its one `controls.ini` (`/storage/.config/ppsspp/PSP/SYSTEM/controls.ini`) gets the D-pad's keys
//!   on the analog nub (`An.Up = 10-4003,10-19`) instead of the PSP's D-pad, and back. Only while its mapping is ROCKNIX's
//!   or this one: a mapping the player changed in PPSSPP stays.
//! Standalone Mupen64Plus and Flycast (not the default anywhere) keep their own controls.
use std::path::Path;

/// The system.cfg key (`n64["<game>"].dpadstick`, `n64.dpadstick`).
pub const KEY: &str = "dpadstick";
pub const AUTO: &str = "auto";
pub const STICK: &str = "stick";
pub const DPAD: &str = "dpad";
/// The systems it is for (the row is shown for these only).
pub const SYSTEMS: &[&str] = &["n64", "psp", "dreamcast"];

/// RetroArch's remap folder on ROCKNIX (`input_remapping_directory = "~/remappings"`).
pub const REMAPS: &str = "/storage/remappings";
/// Standalone PPSSPP's controls on ROCKNIX (start_ppsspp.sh copies /usr/config/ppsspp there once).
pub const PPSSPP_CONTROLS: &str = "/storage/.config/ppsspp/PSP/SYSTEM/controls.ini";

/// The game's remap: the D-pad on the left stick, every other button as it is. The first line marks it as pocket-ui's.
pub const REMAP: &str = "# pocket-ui: D-pad as left stick (Runs on > D-pad)\n\
input_player1_btn_up = \"19\"\n\
input_player1_btn_down = \"18\"\n\
input_player1_btn_left = \"17\"\n\
input_player1_btn_right = \"16\"\n";

/// RetroArch's remap folder of a core: its `corename` from the core's .info (H700 image 20261001). None: not one of
/// these systems' RetroArch cores.
pub fn remap_folder(core: &str) -> Option<&'static str> {
    Some(match core {
        "mupen64plus_next" | "mupen64plus_next_develop" => "Mupen64Plus-Next",
        "mupen64plus_next_gles2" => "mupen64plus_next_gles2",
        "mupen64plus_next_gles3" => "mupen64plus_next_gles3",
        "parallel_n64" => "ParaLLEl N64",
        "flycast" | "flycast2021" => "Flycast",
        "flycast_gles2" => "Flycast GLES2",
        "ppsspp" => "PPSSPP",
        _ => return None,
    })
}

/// Whether the D-pad moves the left stick: the choice, Automatic going by the handheld's sticks.
pub fn wanted(value: &str, has_sticks: bool) -> bool {
    match value {
        STICK => true,
        DPAD => false,
        _ => !has_sticks,
    }
}

fn buttons(text: &str) -> std::collections::BTreeSet<String> {
    text.lines()
        .map(|l| l.replace([' ', '\t'], ""))
        .filter(|l| l.starts_with("input_player") && l.contains("_btn_"))
        .collect()
}

/// A remap pocket-ui wrote: its first line, or, saved again by RetroArch without it, exactly its buttons.
fn is_ours(text: &str) -> bool {
    text.starts_with("# pocket-ui: D-pad as left stick") || buttons(text) == buttons(REMAP)
}

/// Before a RetroArch start: the game's remap in `folder` (`<remappings>/<core>`) as `on` asks. A remap of the player's
/// own for the game or the core (`<core>.rmp`) is left alone and none is written.
pub fn apply_remap(folder: &Path, rom: &Path, on: bool) -> std::io::Result<()> {
    let Some(stem) = rom.file_stem().and_then(|s| s.to_str()) else { return Ok(()) };
    let file = folder.join(format!("{stem}.rmp"));
    let current = std::fs::read_to_string(&file).ok();
    let ours = current.as_deref().is_some_and(is_ours);
    let core_name = folder.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let core_remap = folder.join(format!("{core_name}.rmp")).exists();
    match (on, current.is_some()) {
        (true, false) if !core_remap => {
            std::fs::create_dir_all(folder)?;
            crate::fsutil::write_atomic(&file, REMAP.as_bytes())
        }
        (false, true) if ours => std::fs::remove_file(&file),
        _ => Ok(()),
    }
}

/// PPSSPP's D-pad keys and analog keys in ROCKNIX's `controls.ini` (the H700 image's /usr/config copy).
const PPSSPP_DPAD: [(&str, &str); 4] = [("Up", "10-19"), ("Down", "10-20"), ("Left", "10-21"), ("Right", "10-22")];
const PPSSPP_NUB: [(&str, &str); 4] = [("An.Up", "10-4003"), ("An.Down", "10-4002"), ("An.Left", "10-4001"), ("An.Right", "10-4000")];

fn ini_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim()))
}

fn ini_with(text: &str, key: &str, value: &str) -> String {
    let mut out: Vec<String> = text.lines().map(|l| match l.split_once('=') {
        Some((k, _)) if k.trim() == key => format!("{key} = {value}"),
        _ => l.to_string(),
    }).collect();
    if text.ends_with('\n') { out.push(String::new()); }
    out.join("\n")
}

/// PPSSPP's controls with the D-pad on the nub (`on`) or on the PSP's D-pad. None: the player changed these keys in
/// PPSSPP (or the file is not ROCKNIX's), so they stay.
pub fn ppsspp_controls(text: &str, on: bool) -> Option<String> {
    let mut dpad_ours = true;
    for ((d, key), (n, nub)) in PPSSPP_DPAD.iter().zip(PPSSPP_NUB.iter()) {
        let (dv, nv) = (ini_value(text, d)?, ini_value(text, n)?);
        let stock = dv == *key && nv == *nub;
        let moved = dv.is_empty() && nv == format!("{nub},{key}");
        dpad_ours &= stock || moved;
    }
    if !dpad_ours { return None; }
    let mut out = text.to_string();
    for ((d, key), (n, nub)) in PPSSPP_DPAD.iter().zip(PPSSPP_NUB.iter()) {
        out = ini_with(&out, d, if on { "" } else { key });
        out = ini_with(&out, n, &if on { format!("{nub},{key}") } else { nub.to_string() });
    }
    Some(out)
}

/// Before a start on ROCKNIX's emulator: the choice from system.cfg (the game's, else the console's, else Automatic)
/// applied to `core`'s controls.
pub fn before_start(system: &str, core: &str, rom: &Path) {
    if !SYSTEMS.contains(&system) || crate::proc::mock() { return; }
    before_start_in(system,core,rom,Path::new(crate::emu_choice::SYSTEM_CFG),Path::new(REMAPS),Path::new(PPSSPP_CONTROLS),crate::device::current().has_sticks());
}

fn before_start_in(system: &str, core: &str, rom: &Path, cfg_path: &Path, remaps: &Path, controls: &Path, has_sticks: bool) {
    let cfg = crate::emu_choice::Cfg::read(cfg_path);
    let Some(setting) = crate::emu_choice::RA_SETTINGS.iter().find(|s| s.key == KEY) else { return };
    let file = rom.file_name().and_then(|f| f.to_str());
    let on = wanted(&crate::emu_choice::effective_value(&cfg, system, file, setting), has_sticks);
    if let Some(folder) = remap_folder(core) {
        if let Err(e) = apply_remap(&remaps.join(folder), rom, on) {
            eprintln!("dpad-stick: remap for {}: {e}", rom.display());
        }
    } else if core == "ppsspp-sa" {
        let path = controls;
        // Before PPSSPP's first start the file is still ROCKNIX's copy in /usr/config: start_ppsspp.sh copies it.
        let Ok(text) = std::fs::read_to_string(path) else { return };
        if let Some(next) = ppsspp_controls(&text, on).filter(|n| *n != text) {
            if let Err(e) = crate::fsutil::write_atomic(path, next.as_bytes()) {
                eprintln!("dpad-stick: PPSSPP controls: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_source_rule_standalone_dpad_settings() {
        let d=crate::testutil::TestDir::new("dpad-standalone-file-rule");let cfg=d.join("system.cfg");let remaps=d.join("remaps");let controls=d.join("controls.ini");let rom=d.join("Rule.iso");
        for (label,bytes) in crate::file_faults::faults(b"dreamcast.dpadstick=stick\n") {
            let _=std::fs::remove_file(&cfg);if let Some(bytes)=bytes {std::fs::write(&cfg,bytes).unwrap();}let output=remaps.join("Flycast/Rule.rmp");let _=std::fs::remove_file(&output);
            before_start_in("dreamcast","flycast",&rom,&cfg,&remaps,&controls,true);
            if label=="valid" {assert!(std::fs::read_to_string(output).unwrap().contains("input_player1_btn_up = \"19\""));}
        }
        std::fs::write(&cfg,b"psp.dpadstick=stick\n").unwrap();
        let seed=PPSSPP_DPAD.iter().chain(PPSSPP_NUB.iter()).map(|(k,v)|format!("{k} = {v}\n")).collect::<String>();
        for (label,bytes) in crate::file_faults::faults(seed.as_bytes()) {
            let _=std::fs::remove_file(&controls);if let Some(bytes)=bytes {std::fs::write(&controls,bytes).unwrap();}
            before_start_in("psp","ppsspp-sa",&rom,&cfg,&remaps,&controls,true);
            if label=="valid" {let got=std::fs::read_to_string(&controls).unwrap();assert_eq!(ini_value(&got,"Up"),Some(""));assert_eq!(ini_value(&got,"An.Up"),Some("10-4003,10-19"));}
        }
    }

    const ROCKNIX_PPSSPP: &str = "[ControlMapping]\nUp = 10-19\nDown = 10-20\nLeft = 10-21\nRight = 10-22\nCircle = 10-189\n\
An.Up = 10-4003\nAn.Down = 10-4002\nAn.Left = 10-4001\nAn.Right = 10-4000\nRightAn.Up = 10-4007\n";

    #[test]
    fn automatic_puts_the_d_pad_on_the_stick_only_without_sticks() {
        assert!(wanted(AUTO, false) && !wanted(AUTO, true));
        assert!(wanted(STICK, true) && !wanted(DPAD, false));
        assert!(wanted("", false), "nothing set is Automatic");
        // The cores ROCKNIX's H700 image starts for N64 and Dreamcast by default, by their RetroArch remap folders.
        assert_eq!(remap_folder("mupen64plus_next"), Some("Mupen64Plus-Next"));
        assert_eq!(remap_folder("flycast2021"), Some("Flycast"));
        assert_eq!(remap_folder("snes9x"), None);
    }

    #[test]
    fn mario_64_on_retroarch_gets_the_d_pad_on_the_left_stick_and_a_players_own_remap_stays() {
        let dir = crate::testutil::TestDir::new("dpadstick-ra");
        let folder = dir.join("Mupen64Plus-Next");
        let rom = Path::new("/storage/roms/n64/Super Mario 64 (USA).z64");
        let file = folder.join("Super Mario 64 (USA).rmp");
        apply_remap(&folder, rom, true).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("input_player1_btn_up = \"19\"") && text.contains("input_player1_btn_right = \"16\""));
        // RetroArch saved it again without the first line: still ours, so D-pad takes it away.
        std::fs::write(&file, text.lines().skip(1).collect::<Vec<_>>().join("\n")).unwrap();
        apply_remap(&folder, rom, false).unwrap();
        assert!(!file.exists());
        // The player's own remap for the game stays, both ways.
        std::fs::write(&file, "input_player1_btn_a = \"0\"\n").unwrap();
        apply_remap(&folder, rom, true).unwrap();
        apply_remap(&folder, rom, false).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "input_player1_btn_a = \"0\"\n");
        std::fs::remove_file(&file).unwrap();
        // A core remap of theirs: no game remap is written over it.
        std::fs::write(folder.join("Mupen64Plus-Next.rmp"), "input_player1_btn_b = \"8\"\n").unwrap();
        apply_remap(&folder, rom, true).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn standalone_ppsspp_moves_the_d_pad_to_the_nub_and_back_but_never_a_players_mapping() {
        let on = ppsspp_controls(ROCKNIX_PPSSPP, true).unwrap();
        assert!(on.contains("\nUp = \n") && on.contains("An.Up = 10-4003,10-19") && on.contains("An.Right = 10-4000,10-22"));
        assert!(on.contains("Circle = 10-189") && on.contains("RightAn.Up = 10-4007"), "the other keys stay");
        assert_eq!(ppsspp_controls(&on, true).unwrap(), on, "again: nothing changes");
        assert_eq!(ppsspp_controls(&on, false).unwrap(), ROCKNIX_PPSSPP, "back to ROCKNIX's exactly");
        let own = ROCKNIX_PPSSPP.replace("Up = 10-19", "Up = 10-33");
        assert_eq!(ppsspp_controls(&own, true), None);
    }
}
