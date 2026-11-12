//! `POCKETUI_FRAME_DUMP=<dir>`: the event loop tick by tick on the fixed 60 fps clock of `pocket_ui::frame_dump`
//! (docs/dev/environment.md). Per tick: wait for its real time, fix the UI clock at tick k's time, the script's words of
//! this tick, then the loop's events and the frame they ask for, which `draw` hands to the dump. The test driver has
//! no window: there the frame of every tick is painted here.
//!
//! `POCKETUI_FRAME_DUMP_SCRIPT=<file>`: `<frame> <word>` lines with the command FIFO's button words (up down left
//! right a b x y l1 r1 l2 r2 home select start volup voldown), `down:<word>` / `up:<word>` to hold one, `wake` and
//! `quit`; `end <frame>` ends the dump and pocket-ui there. Without a script it ends when pocket-ui quits; presses
//! from the FIFO or the pad are written down with their frame either way.

use std::time::Duration;

use pocket_ui::frame_dump;
use pocket_ui::pad::{Button, PadEvent};
use winit::event_loop::EventLoop;
use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};

use crate::{test_driver_dir, App, UserEvent, CANCEL, CONFIRM};

/// The first frame may take this long before the dump starts without it.
const FIRST_FRAME_AT_LATEST: Duration = Duration::from_secs(60);

fn button(word: &str) -> Option<Button> {
    Some(match word {
        "up" => Button::Up,
        "down" => Button::Down,
        "left" => Button::Left,
        "right" => Button::Right,
        "a" => CONFIRM,
        "b" => CANCEL,
        "l1" => Button::L1,
        "r1" => Button::R1,
        "l2" => Button::L2,
        "r2" => Button::R2,
        "home" => Button::Mode,
        "x" => Button::North,
        "y" => Button::West,
        "select" => Button::Select,
        "start" => Button::Start,
        "volup" => Button::VolUp,
        "voldown" => Button::VolDown,
        _ => return None,
    })
}

/// The events of one script word; None for a word the script can't have.
fn events(word: &str) -> Option<Vec<UserEvent>> {
    match word {
        "wake" => return Some(vec![UserEvent::Wake]),
        "quit" => return Some(vec![UserEvent::Quit]),
        _ => {}
    }
    let (word, presses): (&str, &[bool]) = match word.split_once(':') {
        Some(("down", w)) => (w, &[true]),
        Some(("up", w)) => (w, &[false]),
        _ => (word, &[true, false]),
    };
    let button = button(word)?;
    Some(presses.iter().map(|&pressed| UserEvent::Pad(PadEvent::Button { source: usize::MAX, button, pressed })).collect())
}

pub(crate) fn run(mut event_loop: EventLoop<UserEvent>, app: &mut App) {
    let proxy = event_loop.create_proxy();
    let script = std::env::var_os(frame_dump::SCRIPT_ENV).map(|p| {
        let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("frame-dump: script {}: {e}", p.to_string_lossy()));
        let (lines, end) = frame_dump::script(&text).unwrap_or_else(|e| panic!("frame-dump: {e}"));
        if let Some((_, w)) = lines.iter().find(|(_, w)| events(w).is_none()) {
            panic!("frame-dump: script: no word {w}");
        }
        (lines, end)
    });
    let (lines, end) = script.unwrap_or_default();
    // Ordinary running until the first frame is on screen (the window, the library, the covers).
    let started = std::time::Instant::now();
    while !app.drew_first && started.elapsed() < FIRST_FRAME_AT_LATEST {
        if let PumpStatus::Exit(_) = event_loop.pump_app_events(Some(Duration::from_millis(10)), app) {
            return;
        }
    }
    let headless = test_driver_dir().is_some();
    let real0 = frame_dump::start();
    let (mut tick, mut next, mut late) = (0u64, 0usize, 0u64);
    println!("frame-dump: to {}, {} script lines, end {end:?}", frame_dump::dir().map_or_else(String::new, |d| d.display().to_string()), lines.len());
    loop {
        if end.is_some_and(|e| tick >= e) {
            break;
        }
        // Never ahead of real time, so presses from outside land where they were sent.
        let due = real0 + frame_dump::tick_ns(tick);
        let now = frame_dump::real_ns();
        if now < due {
            std::thread::sleep(Duration::from_nanos(due - now));
        }
        late = frame_dump::real_ns().saturating_sub(due) / 1_000_000;
        frame_dump::set_tick(tick);
        while next < lines.len() && lines[next].0 <= tick {
            let word = &lines[next].1;
            frame_dump::event(|| format!("script {word}"));
            for ev in events(word).unwrap_or_default() {
                let _ = proxy.send_event(ev);
            }
            next += 1;
        }
        // Twice: a redraw asked for while the first turn's events are handled comes in the second.
        let mut exit = false;
        for _ in 0..2 {
            exit |= matches!(event_loop.pump_app_events(Some(Duration::ZERO), app), PumpStatus::Exit(_));
        }
        if headless && !exit {
            let (buf, w, h) = app.paint_headless(std::time::Instant::now());
            frame_dump::capture(&buf, w, h);
        }
        tick += 1;
        if exit {
            break;
        }
    }
    frame_dump::finish(tick, late);
    println!("frame-dump: {tick} frames at 60 fps, the last {late} ms after its time");
}
