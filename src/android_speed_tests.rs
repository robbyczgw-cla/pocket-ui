//! Android's player runs fast-forward and rewind with Linux's code (`play_speed`, `hotkeys`, `rewind`): its glue, tested here.
#[path = "../android/src/player_speed.rs"]
mod player_speed;

/// The player wires Linux's speed parts in: its keys go through `player_speed` (HOME's tap, HOME + R2 / L2), its refresh
/// runs `play_speed` (runs, last_run, the rewind hold, the badge), and Speed, rumble, fan's two rows change its settings.
/// (The player itself needs Android's JNI; the device check of 18.11. saw FF 4x, REWIND and the rows on the Mini V2.)
#[test]
fn the_player_runs_fast_forward_and_rewind_with_the_shared_code() {
    let src = include_str!("../android/src/player.rs");
    for wired in [
        "self.speed.key(key, down)",
        "self.speed.trigger(",
        "self.speed.hold.turn(",
        "pocket_ui::play_speed::runs(",
        "pocket_ui::play_speed::last_run(",
        "pocket_ui::play_speed::badge(",
        "self.speed.history.step_back()",
        "self.speed.history.push(",
        "pause_options::Entry::FastForward =>",
        "pause_options::Entry::Rewind =>",
    ] {
        assert!(src.contains(wired), "player.rs lacks {wired}");
    }
    // Android's Back and the system's Home stay Android's: only BUTTON_MODE is the shortcuts' HOME.
    assert_eq!(player_speed::HOME_KEY, 110);
    assert!(player_speed::button_of(4).is_none() && player_speed::button_of(3).is_none());
}
