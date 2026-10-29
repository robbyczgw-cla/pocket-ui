//! UI click stream across a standalone guest exit; scripts/audio-bench/ui-guest.sh (from fix/audio-after-guest-sol).
use pocket_ui::ui_sound::{self, Config, Ctx, Sound};
use std::time::{Duration, Instant};

fn main() {
    ui_sound::start();
    // pocket-ui starts the mixer at launch, seconds before anyone presses a button: it sets the shelf's quantum then.
    std::thread::sleep(Duration::from_secs(1));
    let start = Instant::now();
    // Baseline 0..4, pause menu over the guest 5..10, shelf 10..70,
    // three seconds quiet, then recovery clicks 73..78.
    // No two-second silence: exercise the stream that survives the transition.
    for n in 0..312 {
        // A game starts (pocket-ui stops its sounds and gives the game ROCKNIX's quantum). The quiet at 70-73 s is just
        // no clicks: the output closes by itself after 2 s.
        // UI_GUEST_HOSTED=1: the game is pocket-core's, which keeps the shelf's quantum (`stop_for(false)`).
        if n == 16 {
            ui_sound::stop_for(std::env::var_os("UI_GUEST_HOSTED").is_none());
        }
        // The guest ends at 10 s (ui-guest-case.sh): pocket-ui's game-exit path (`main.rs`, every game).
        if n == 40 {
            ui_sound::game_ended();
        }
        let in_game = (20..40).contains(&n);
        if !(16..20).contains(&n) && !(280..292).contains(&n) {
            ui_sound::play(
                Sound::Nav,
                &Ctx {
                    config: Config::default(),
                    in_game,
                    pause_menu: in_game,
                    asleep: false,
                },
            );
        }
        if let Some(wait) = (start + Duration::from_millis((n + 1) * 250)).checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
    ui_sound::stop();
    std::thread::sleep(Duration::from_millis(300));
}
