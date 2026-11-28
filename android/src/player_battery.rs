//! The low-battery guard in Android's player, as pocket-core's on Linux (`pocket_ui::low_battery`, docs/SLEEP.md): at
//! `WARN_AT` % on battery one warning in pocket-core's pill over the game; at `CRITICAL_AT` %, two readings in a row, the
//! game is saved and the player goes back to the shelf (an app can't switch the handheld off; Android does that itself at
//! 0 %, with the game then already saved). The decisions are `low_battery`'s and tested there
//! (`androids_player_warns_at_5_and_saves_and_closes_at_2_percent`).
//!
//! The reading (`PlayerActivity.batteryLevel`, Android's sticky battery broadcast) runs every `low_battery::EVERY` on a
//! thread of its own; the GL thread only takes the news (`take`), one atomic swap a frame. Each game starts a fresh
//! guard (`start`): a game started at 1 % warns at once and is saved and closed at the next reading.

use pocket_ui::low_battery::{self, Alert, Guard, News};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Once;

static NEWS: News = News::new();
/// Bumped by every game's start: the thread then forgets what it decided for the game before.
static GAME: AtomicU32 = AtomicU32::new(0);
static THREAD: Once = Once::new();

/// A game starts (`nativeCreate`): a fresh guard, its first reading at once. The thread lives as long as the player's
/// process (`:player`, kept for the next game by `PlayerWarm`).
pub fn start() {
    NEWS.clear();
    GAME.fetch_add(1, Ordering::SeqCst);
    THREAD.call_once(|| {
        let spawned = std::thread::Builder::new().name("battery-guard".into()).spawn(|| {
            let mut game = u32::MAX;
            let mut guard = Guard::default();
            loop {
                let now = GAME.load(Ordering::SeqCst);
                if now != game {
                    game = now;
                    guard = Guard::default();
                }
                let level = low_battery::parse_android(&pocket_ui::platform::android::string("batteryLevel"));
                // On the charger: whatever was said and not yet taken no longer holds.
                if level.is_some_and(|l| l.external_power) {
                    NEWS.clear();
                }
                if let Some(alert) = guard.observe(level) {
                    log::info!("player battery: {alert:?}");
                    NEWS.put(alert);
                    // A paused game draws only when asked: ask, so the news is taken now and not at the next press.
                    let _ = pocket_ui::platform::android::strings("wakeFrame", &[]);
                }
                std::thread::sleep(low_battery::EVERY);
            }
        });
        if let Err(error) = spawned {
            log::warn!("player battery: no guard thread: {error}");
        }
    });
}

/// What the guard said since the last frame, once.
pub fn take() -> Option<Alert> {
    NEWS.take()
}
