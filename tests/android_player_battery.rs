//! Android's player acts on the low-battery guard in every frame it draws (`android/src/player_battery.rs`,
//! `Player::battery`). 26.11.2026 in the emulator: a lost audio focus paused the game into its menu through
//! `nativeSuspend`, which sets `suspended` while the game stays on screen; `battery` returned on `suspended`, so the
//! 2 % alert waited there and the game was neither saved nor closed. The player only draws while it is in front, so
//! nothing in `battery` may wait for `suspended` or `paused`.

#[test]
fn the_battery_guard_acts_in_the_pause_menu_too() {
    let player = include_str!("../android/src/player.rs");
    let at = player.find("    fn battery(&mut self) {").expect("Player::battery");
    let body = &player[at..];
    let body = &body[..body.find("\n    }\n").expect("its end")];
    let code: String = body.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    for waits in ["self.suspended", "self.paused"] {
        assert!(!code.contains(waits), "battery() must not wait for {waits}:\n{code}");
    }
    assert!(code.contains("player_battery::take()"), "it takes the guard's news");
    // It runs first in every frame, before the quit check.
    let frame = &player[player.find("fn frame(&mut self").unwrap()..];
    assert!(frame.find("self.battery();").unwrap() < frame.find("if self.quit").unwrap());
}

/// Sol 26.11.: a save that fails at 2 % is said in the pause menu's own line too (the pill shows only over a running
/// game), as Save & quit says its failures.
#[test]
fn a_failed_save_at_2_percent_is_said_in_the_pause_menu_too() {
    let player = include_str!("../android/src/player.rs");
    let at = player.find("    fn battery(&mut self) {").unwrap();
    let body = &player[at..];
    let body = &body[..body.find("\n    }\n").unwrap()];
    let err = &body[body.find("Err(e) =>").expect("the failed save")..];
    assert!(err.contains("self.message = said(e") && err.contains("self.last_ok = false"), "{err}");
    // The pages without a message line (Zoom, Achievements) give way to the one that shows it.
    assert!(err.contains("Menu::Zoom | Menu::Achievements") && err.contains("self.menu = Menu::Pause"), "{err}");
}

/// Sol 26.11.: a game from the Handheld view that can't start goes back without a dialog only when the shelf has the
/// reason to say (`player-start-error` written); a full or broken app folder keeps the visible dialog.
#[test]
fn the_start_error_skips_the_dialog_only_when_the_shelf_can_say_it() {
    let java = include_str!("../android/java/xyz/pocketui/app/PlayerActivity.java");
    let line = java.lines().find(|l| l.contains("!ready && fromHandheld()")).expect("the Handheld view's way back");
    assert!(line.contains("\"player-start-error\").isFile()"), "{line}");
}
