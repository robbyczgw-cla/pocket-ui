//! Public player operations are inventoried from Linux's command enum. A new operation must be wired
//! into Android or have an explicit, reviewable reason here; platform IPC itself is not a requirement.
#[test]
fn every_linux_player_operation_has_an_android_hook_or_an_explicit_exception() {
    const LINUX: &str = include_str!("../src/core_command.rs");
    const ANDROID: &str = include_str!("../android/src/player.rs");
    let supported = [
        ("AudioStats", "player audio:"), ("Pause", "self.pause(true)"),
        ("Resume", "self.pause(false)"), ("Sleep", "nativeSuspend"),
        ("Wake", "nativeForeground"), ("Autosave", "fn automatic("),
        ("Shot", "ui_sound::Sound::Shutter"), ("Preview", "fn refresh_live("), ("Save", "fn state("),
        ("Load", "fn load("), ("Slot", "fn places("),
        ("Reload", "fn change_option("), ("Players", "nativeRoster"),
        ("Cheats", "cheats::CheatMenu"), ("Look", "picture::Settings"),
        ("Note", "paint_note("), ("Osd", "fn paint_level_bar("), ("Press", "nativeKey"),
        ("Axis", "fn analog("), ("Option", "self.core.set_option("),
        ("Control", "Menu::Controls"), ("Set", "fn restore_defaults("),
        ("Quit", "self.leave_last_picture("), ("Disc", "self.core.set_disc(target)"),
    ];
    let exceptions = [
        ("Unknown", "The FIFO parser's no-op, with no player function."),
        ("ScreenOff", "Android owns display power; the Activity lifecycle triggers Suspend."),
        ("PixelPath", "The Pixel Path developer tool has no Android surface yet."),
        ("Restart", "Restart without restoring a moment is not yet offered in the Android menu."),
        ("AchievementDemo", "Linux test-only unlock-card injection; no production action."),
        ("Peek", "Linux test-only memory inspection; no production action."),
        ("Hardcore", "Android chooses Hardcore at game start; changing it in play is not yet wired."),
    ];
    let body = LINUX.split("pub enum Command<'a> {").nth(1).unwrap().split("\n}").next().unwrap();
    let body = body.lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join(" ");
    let mut depth = 0;
    let mut start = 0;
    let mut operations = Vec::new();
    for (at, c) in body.char_indices() {
        match c { '(' => depth += 1, ')' => depth -= 1, ',' if depth == 0 => {
            let declaration = body[start..at].trim();
            operations.push(declaration.split('(').next().unwrap().trim());
            start = at + 1;
        }, _ => {} }
    }
    for operation in &operations {
        let hook = supported.iter().find(|(name, _)| name == operation);
        let exception = exceptions.iter().find(|(name, _)| name == operation);
        assert!(hook.is_some() ^ exception.is_some(), "{} needs exactly one hook or exception", operation);
        if let Some((_, hook)) = hook { assert!(ANDROID.contains(hook), "{} lost its Android hook: {}", operation, hook); }
        if let Some((_, why)) = exception { assert!(!why.is_empty()); }
    }
    assert_eq!(operations.len(), supported.len() + exceptions.len(), "remove stale inventory entries too");
}

/// The device's volume keys show pocket-ui's level bar at the top, not Android's volume panel, on the shelf and in a game
/// (as on Linux, `osd`): the handheld view takes them from winit (`handle_volume_keys`, else winit leaves them to Android)
/// into the quick settings' Vol± (`quick.rs`), the player from Android's key dispatch into `nativeVolumeKey`. The rounding
/// is `volume_steps`' (host tests there); this guards the wiring the host can't run.
#[test]
fn the_volume_keys_step_the_volume_with_pocket_uis_bar_on_the_shelf_and_in_a_game() {
    const HANDHELD: &str = include_str!("../android/handheld/app_android.rs");
    const PLAYER: &str = include_str!("../android/src/player.rs");
    const PLAYER_JAVA: &str = include_str!("../android/java/xyz/pocketui/app/PlayerActivity.java");
    assert!(HANDHELD.contains(".handle_volume_keys()"), "the handheld view leaves the volume keys to Android's panel");
    for (key, button) in [("AudioVolumeUp", "VolUp"), ("AudioVolumeDown", "VolDown")] {
        assert!(HANDHELD.contains(&format!("KeyCode::{key}) => Some(Button::{button})")), "{key} isn't the shelf's {button}");
    }
    assert!(PLAYER_JAVA.contains("nativeVolumeKey(up)"), "the player hands the volume keys to Android");
    assert!(!PLAYER_JAVA.contains("KEYCODE_VOLUME_DOWN)return super.dispatchKeyEvent(event)"), "the player hands the volume keys to Android");
    assert!(PLAYER.contains("fn Java_xyz_pocketui_app_PlayerActivity_nativeVolumeKey(") && PLAYER.contains("paint_level_bar(&mut c"));
}
