//! The letter jump of the lists: L2 and R2 go to the previous and next first letter, and the letter shows large for
//! a moment, as a phone's fast scroll does.
//!
//! The rows are taken in the order the list shows them (games and systems are sorted by name). Every row has a
//! group: its first letter, '#' for names that start with a digit or a sign, and `TOP` for the rows above the
//! alphabet (Home's recent games, Favorites, PC streaming). Like Up and Down, the jump stops at the ends of the list
//! instead of wrapping round: R2 in the last letter goes to the last row, L2 in the first row does nothing.
//!
//! The Mini V2's L2 and R2 are analog triggers: `Triggers` turns their travel into presses (from half way, let go
//! below a third), and drops the button event a controller may send as well for the same pull.

use std::time::{Duration, Instant};

use crate::pad::{Axis, Button, PadEvent};

/// The group of rows above the alphabet (recent games and the like): no letter is shown for it.
pub const TOP: char = '\0';

/// How long the letter stays after the last jump, and how much of that is its fade.
pub const SHOW: Duration = Duration::from_millis(700);
pub const FADE: Duration = Duration::from_millis(220);

/// A held L2 or R2 goes on jumping this often (Up and Down repeat much faster; letters need time to be read).
pub const REPEAT: Duration = Duration::from_millis(200);

/// The group a name sorts under: its first letter in capitals, '#' for a digit or sign.
pub fn initial(title: &str) -> char {
    crate::cabinet::initial(title)
}

/// Where L2 (`forward` false) or R2 moves the selection `at` in a list of `groups`; None when it stays.
///
/// R2 goes to the first row of the next group, or to the last row from the last group. L2 goes to the first row of
/// the selection's group, and from there to the first row of the group before.
pub fn jump(groups: &[char], at: usize, forward: bool) -> Option<usize> {
    let last = groups.len().checked_sub(1)?;
    let at = at.min(last);
    let to = if forward {
        if at == last {
            return None;
        }
        (at + 1..=last).find(|&k| groups[k] != groups[at]).unwrap_or(last)
    } else {
        if at == 0 {
            return None;
        }
        let from = if groups[at - 1] == groups[at] { at } else { at - 1 };
        let mut k = from;
        while k > 0 && groups[k - 1] == groups[from] {
            k -= 1;
        }
        k
    };
    (to != at).then_some(to)
}

/// The letter shown large after a jump.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flash {
    pub letter: char,
    pub at: Instant,
    /// Pinned for `--snapshot`: one picture is painted whenever the machine gets to it, so the letter shows whole, as
    /// in the jump's first frame, instead of fading with however long this run takes (`alpha`, `needs_frames`).
    pinned: bool,
}

impl Flash {
    /// A flash for a jump that landed in `group`; None for the rows above the alphabet.
    pub fn new(group: char, now: Instant) -> Option<Flash> {
        (group != TOP).then_some(Flash { letter: group, at: now, pinned: false })
    }

    /// The flash of a jump, for a snapshot: the single picture shows the letter whole, whatever the clock says when it
    /// is painted (a snapshot must not depend on how loaded the machine is; `at` is not read then).
    pub fn pinned(letter: char) -> Option<Flash> {
        (letter != TOP).then_some(Flash { letter, at: Instant::now(), pinned: true })
    }

    /// How opaque it is at `now` (255 = fully), None once it is gone.
    pub fn alpha(&self, now: Instant) -> Option<u8> {
        if self.pinned {
            return Some(255);
        }
        let age = now.saturating_duration_since(self.at);
        if age >= SHOW {
            return None;
        }
        let left = SHOW - age;
        // Settings > Sound & feel > Motion: Off shows it whole until it goes.
        if left >= FADE || !crate::motion::allowed(crate::motion::Kind::Fade) {
            return Some(255);
        }
        Some((255.0 * left.as_secs_f32() / FADE.as_secs_f32()) as u8)
    }

    /// Frames are needed until one has been painted after it is gone (`motion::SETTLE`): otherwise its last, faint
    /// picture stays on screen until the next input.
    pub fn needs_frames(&self, now: Instant) -> bool {
        !self.pinned && now < self.at + SHOW + crate::motion::SETTLE
    }
}

/// What `Triggers::feed` made of a pad event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Not L2 or R2: pass the event on as it is.
    Other,
    /// L2 or R2 that changes nothing (travel inside the dead band, or the second report of one pull): drop it.
    Drop,
    /// L2 or R2 went down (true) or up (false).
    Button(Button, bool),
}

/// Analog travel from which a trigger counts as pressed, and below which it counts as let go again.
const PRESS: f32 = 0.5;
const RELEASE: f32 = 0.3;

/// L2 and R2 as buttons, whether they arrive as travel (the Mini V2), as buttons, or both.
#[derive(Default, Debug)]
pub struct Triggers {
    l2: bool,
    r2: bool,
}

impl Triggers {
    pub fn feed(&mut self, e: &PadEvent) -> Edge {
        let (button, pressed) = match *e {
            PadEvent::Axis { axis: Axis::L2, value, .. } => (Button::L2, self.travel(self.l2, value)),
            PadEvent::Axis { axis: Axis::R2, value, .. } => (Button::R2, self.travel(self.r2, value)),
            PadEvent::Button { button: b @ (Button::L2 | Button::R2), pressed, .. } => (b, pressed),
            _ => return Edge::Other,
        };
        let down = if button == Button::L2 { &mut self.l2 } else { &mut self.r2 };
        if *down == pressed {
            return Edge::Drop;
        }
        *down = pressed;
        Edge::Button(button, pressed)
    }

    /// Pressed or not after `value`, with a dead band between the two thresholds.
    fn travel(&self, was: bool, value: f32) -> bool {
        if was {
            value > RELEASE
        } else {
            value >= PRESS
        }
    }

    /// Both let go (the screen changed hands, a game started).
    pub fn reset(&mut self) {
        *self = Triggers::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups(s: &str) -> Vec<char> {
        s.chars().map(|c| if c == '.' { TOP } else { c }).collect()
    }

    #[test]
    fn r2_goes_to_the_next_letter_and_from_the_last_one_to_the_end() {
        let g = groups("##AAABBDDDDZZ");
        assert_eq!(jump(&g, 0, true), Some(2), "# to A");
        assert_eq!(jump(&g, 3, true), Some(5), "from inside A");
        assert_eq!(jump(&g, 5, true), Some(7));
        assert_eq!(jump(&g, 7, true), Some(11));
        assert_eq!(jump(&g, 11, true), Some(12), "the last letter: to the last row");
        assert_eq!(jump(&g, 12, true), None, "at the end it stays (no wrap, as Down)");
    }

    #[test]
    fn l2_goes_to_the_start_of_the_letter_then_the_letter_before() {
        let g = groups("##AAABBDDDDZZ");
        assert_eq!(jump(&g, 9, false), Some(7), "to the start of D first");
        assert_eq!(jump(&g, 7, false), Some(5), "then to B");
        assert_eq!(jump(&g, 5, false), Some(2));
        assert_eq!(jump(&g, 2, false), Some(0));
        assert_eq!(jump(&g, 1, false), Some(0));
        assert_eq!(jump(&g, 0, false), None, "at the top it stays (no wrap, as Up)");
    }

    #[test]
    fn home_rows_above_the_alphabet_are_one_group() {
        // Continue, two recent games, Favorites, then the systems by name.
        let g = groups("....GGGNPPS");
        assert_eq!(jump(&g, 0, true), Some(4), "from Continue to the first system");
        assert_eq!(jump(&g, 2, true), Some(4));
        assert_eq!(jump(&g, 4, false), Some(0), "back to Continue");
        assert_eq!(jump(&g, 0, false), None);
    }

    #[test]
    fn short_and_empty_lists() {
        assert_eq!(jump(&[], 0, true), None);
        assert_eq!(jump(&groups("A"), 0, true), None);
        assert_eq!(jump(&groups("A"), 0, false), None);
        assert_eq!(jump(&groups("AAAA"), 1, true), Some(3), "one letter only: R2 goes to the end");
        assert_eq!(jump(&groups("AAAA"), 3, false), Some(0));
        assert_eq!(jump(&groups("AB"), 7, false), Some(0), "a selection past the end counts as the last row");
    }

    #[test]
    fn initials_group_digits_and_signs() {
        assert_eq!(initial("legend of Zelda"), 'L');
        assert_eq!(initial("1942"), '#');
        assert_eq!(initial("'98 Koshien"), '#');
        assert_eq!(initial("Ōkami"), 'Ō');
    }

    #[test]
    fn the_flash_holds_then_fades_and_is_never_shown_for_the_top_rows() {
        let t = Instant::now();
        assert!(Flash::new(TOP, t).is_none());
        let f = Flash::new('M', t).unwrap();
        assert_eq!(f.alpha(t), Some(255));
        assert_eq!(f.alpha(t + SHOW - FADE), Some(255));
        let mid = f.alpha(t + SHOW - FADE / 2).unwrap();
        assert!(mid > 100 && mid < 155, "{mid}");
        assert_eq!(f.alpha(t + SHOW), None);
        assert!(f.needs_frames(t + SHOW), "one more frame, without it, so nothing faint is left on screen");
        assert!(!f.needs_frames(t + SHOW + crate::motion::SETTLE));
    }

    #[test]
    fn analog_triggers_press_from_half_way_with_a_dead_band() {
        let mut t = Triggers::default();
        let axis = |axis, value| PadEvent::Axis { source: 0, axis, value };
        assert_eq!(t.feed(&axis(Axis::R2, 0.2)), Edge::Drop);
        assert_eq!(t.feed(&axis(Axis::R2, 0.6)), Edge::Button(Button::R2, true));
        assert_eq!(t.feed(&axis(Axis::R2, 1.0)), Edge::Drop, "still down");
        assert_eq!(t.feed(&axis(Axis::R2, 0.4)), Edge::Drop, "inside the dead band: still down");
        assert_eq!(t.feed(&axis(Axis::R2, 0.1)), Edge::Button(Button::R2, false));
        assert_eq!(t.feed(&axis(Axis::R2, 0.45)), Edge::Drop, "not yet half way");
        assert_eq!(t.feed(&axis(Axis::L2, 0.9)), Edge::Button(Button::L2, true), "L2 on its own");
        assert_eq!(t.feed(&PadEvent::Axis { source: 0, axis: Axis::LeftX, value: 1.0 }), Edge::Other);
        assert_eq!(t.feed(&PadEvent::Button { source: 0, button: Button::Up, pressed: true }), Edge::Other);
    }

    #[test]
    fn a_pull_reported_as_travel_and_as_a_button_counts_once() {
        let mut t = Triggers::default();
        let button = |pressed| PadEvent::Button { source: 1, button: Button::L2, pressed };
        assert_eq!(t.feed(&button(true)), Edge::Button(Button::L2, true));
        assert_eq!(t.feed(&PadEvent::Axis { source: 1, axis: Axis::L2, value: 1.0 }), Edge::Drop);
        assert_eq!(t.feed(&PadEvent::Axis { source: 1, axis: Axis::L2, value: 0.0 }), Edge::Button(Button::L2, false));
        assert_eq!(t.feed(&button(false)), Edge::Drop);
        t.feed(&button(true));
        t.reset();
        assert_eq!(t.feed(&button(true)), Edge::Button(Button::L2, true), "after a reset nothing is held");
    }
}
