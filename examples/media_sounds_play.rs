//! The media sounds through the real UI mixer, one after the other and as the Cozy launch queues them; recorded and
//! measured by scripts/ui-sounds/device-capture.sh. Prints, per request, the monotonic time it was asked (µs) and what.
//! `MEDIA_SOUNDS_VOLUME` is the UI volume step (1-10, default 10: the loudest a player can set).
use pocket_ui::ui_sound::{self, Config, Ctx, Sound};
use std::time::{Duration, Instant};

fn now_us() -> u128 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: a plain clock read into a local.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u128 * 1_000_000 + ts.tv_nsec as u128 / 1000
}

fn main() {
    let volume = std::env::var("MEDIA_SOUNDS_VOLUME").ok().and_then(|v| v.parse().ok()).unwrap_or(10);
    let ctx = Ctx { config: Config { on: true, volume }, in_game: false, pause_menu: false, asleep: false };
    ui_sound::start();
    // pocket-ui starts the mixer at launch, seconds before the first button: the shelf's quantum is set by then.
    std::thread::sleep(Duration::from_secs(2));
    let ask = |sound: Sound, after: Duration, db: f32| {
        println!("{}\t{sound:?}\t{}\t{db}", now_us(), after.as_micros());
        ui_sound::play_after(sound, &ctx, after, db);
    };
    let one = [Sound::CaseOpen, Sound::BoxOpen, Sound::CartIn, Sound::DiscIn, Sound::CardIn, Sound::Insert, Sound::CrtOn];
    for s in one {
        ask(s, Duration::ZERO, 0.0);
        std::thread::sleep(Duration::from_millis(1500));
    }
    ask(Sound::Insert, Duration::from_millis(170), ui_sound::QUICK_DB);
    std::thread::sleep(Duration::from_millis(1500));
    // The Cozy launch, as `app_cabinet::launch_animation` queues it: in at 790 ms (the CRT is pocket-core's).
    for (open, inside) in [(Sound::BoxOpen, Sound::CartIn), (Sound::CaseOpen, Sound::DiscIn), (Sound::CaseOpen, Sound::CardIn)] {
        let t = Instant::now();
        ask(open, Duration::ZERO, 0.0);
        ask(inside, Duration::from_millis(790).saturating_sub(ui_sound::hit_at(inside)), 0.0);
        std::thread::sleep(Duration::from_millis(3000).saturating_sub(t.elapsed()));
    }
    std::thread::sleep(Duration::from_millis(2500));
}
