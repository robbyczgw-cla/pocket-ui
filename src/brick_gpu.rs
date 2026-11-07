//! pocket-ui's half of the Brick's GPU transitions (docs/TRIMUI-BRICK.md, "The GPU"). A page that opens slides and fades in: on the CPU
//! that is the whole page painted again every frame (8 to 13 ms on the Brick's A133), on the GPU one picture moved. When `pocket-fb` draws
//! with the GPU (`--gl`), pocket-ui hands it the transition over the sway IPC (`pocket_transition`) right before the page's first picture
//! and paints no transition itself; pocket-fb then slides and fades that picture at the screen's rate, whatever the paint costs. Without
//! the GPU (pocket-fb says no to `pocket_gpu`, or it is sway) everything stays on the CPU. Only behind the `brick` feature.

use std::cell::Cell;
use std::sync::OnceLock;
use std::time::Duration;

use crate::control::ipc;
use crate::gfx::Canvas;
use crate::motion::{self, Enter, Kind, Transition};

thread_local! {
    /// The transition of the page about to be painted, until its first picture says the page colour.
    static PENDING: Cell<Option<Transition>> = const { Cell::new(None) };
}

const ASK: Duration = Duration::from_millis(300);

/// Whether the screen's bridge draws transitions: asked once, said once in the log.
fn available() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let on = ipc::request(ipc::RUN_COMMAND, "pocket_gpu", ASK).is_ok_and(|r| ipc::all_succeeded(&r));
        println!("brick: page transitions {}", if on { "on the GPU (pocket-fb --gl)" } else { "on the CPU" });
        on
    })
}

/// A page has just opened (`motion_frame_begin`): its transition goes to the GPU, if there is one, and not to the CPU.
pub fn begin(transition: &mut Option<Transition>) {
    if transition.is_some() && available() {
        PENDING.with(|p| p.set(transition.take()));
    }
}

/// The page is painted (`motion_after_paint`): the transition goes out with the page colour, before the picture does.
pub fn after_paint(c: &Canvas) {
    let Some(t) = PENDING.with(|p| p.take()) else { return };
    let bg = c.buf.get((c.h / 2 * c.w + c.w - 2).max(0) as usize).copied().unwrap_or(0) & 0xff_ffff;
    let command = format!("pocket_transition {} {bg:06x} {} {}", words(t), u8::from(motion::cozy()), u8::from(motion::allowed(Kind::Move)));
    if !ipc::request(ipc::RUN_COMMAND, &command, ASK).is_ok_and(|r| ipc::all_succeeded(&r)) {
        println!("brick: the GPU didn't take a page transition, the page came without one");
    }
}

/// The transition as `pocket_transition` reads it: kind, the label's row (Rise) and the page's top.
pub fn words(t: Transition) -> String {
    let (kind, from_y) = match t.enter {
        Enter::Push => ("push", 0),
        Enter::Pop => ("pop", 0),
        Enter::Rise { from_y } => ("rise", from_y),
        Enter::Fade => ("fade", 0),
    };
    format!("{kind} {from_y} {}", t.top)
}

/// `pocket_transition <kind> <from_y> <top> <page colour, hex> <cozy 0|1> <moves 0|1>` back to the enter, the page's top, the page colour
/// and the two settings; None when it doesn't read.
pub fn read(words: &str) -> Option<(Enter, i32, u32, bool, bool)> {
    let w: Vec<&str> = words.split_whitespace().collect();
    let [kind, from_y, top, bg, cozy, moves] = w[..] else { return None };
    let from_y = from_y.parse().ok()?;
    let enter = match kind {
        "push" => Enter::Push,
        "pop" => Enter::Pop,
        "rise" => Enter::Rise { from_y },
        "fade" => Enter::Fade,
        _ => return None,
    };
    Some((enter, top.parse().ok()?, u32::from_str_radix(bg, 16).ok()?, cozy == "1", moves == "1"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn a_transition_reads_back_as_it_was_sent() {
        let t = Transition::new(Enter::Rise { from_y: 300 }, Instant::now(), 112).unwrap();
        let line = format!("{} 1a2b3c 1 0", words(t));
        assert_eq!(read(&line), Some((Enter::Rise { from_y: 300 }, 112, 0x1a2b3c, true, false)));
        assert_eq!(read("push 0 112"), None);
        assert_eq!(read("sideways 0 112 000000 0 1"), None);
    }

    #[test]
    fn the_gpus_look_ends_where_the_cpus_does() {
        let at = Instant::now();
        let t = Transition::new(Enter::Push, at, 112).unwrap();
        let (dx, dy, fade) = t.look(at).unwrap();
        assert!(dx > 10 && dy == 0 && fade > 0.5, "the first frame is a frame ahead: {dx} {dy} {fade}");
        assert_eq!(t.look(at + Duration::from_secs(2)), None);
        let (dx, _, fade) = t.look(t.ends() - Duration::from_millis(1)).unwrap();
        assert!(dx.abs() <= 1 && fade < 0.05, "{dx} {fade}");
    }
}
