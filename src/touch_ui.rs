//! Portable touch targets shared by native launchers.
use crate::pad::Button;

/// A rectangle in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Area {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Area {
        Area { x, y, w, h }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x as f32 && py >= self.y as f32 && px < (self.x + self.w) as f32 && py < (self.y + self.h) as f32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Row (or card) number `n` of the list on screen.
    Row(usize),
    /// A hint pill: pressing it is pressing that button.
    Button(Button),
    /// That button pressed this many times: a tab two steps away (History's Year from Week) goes there in one tap.
    Presses(Button, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapAction {
    Nothing,
    /// Move the selection by this many rows (negative: up).
    Steps(i32),
    /// The row was already selected: confirm it.
    Activate,
    Press(Button),
    /// The button pressed this many times in a row (`Target::Presses`).
    Presses(Button, u8),
}

/// What the last painted frame offered to touch: filled by the paint functions, emptied at the start of each.
#[derive(Default)]
pub struct HitMap {
    areas: Vec<(Area, Target)>,
    sel: Option<usize>,
    len: usize,
    pitch: (f32, f32),
}

impl HitMap {
    pub fn clear(&mut self) {
        self.areas.clear();
        self.sel = None;
        self.len = 0;
        self.pitch = (0.0, 0.0);
    }

    pub fn row(&mut self, area: Area, index: usize) {
        self.areas.push((area, Target::Row(index)));
    }

    pub fn button(&mut self, area: Area, button: Button) {
        self.areas.push((area, Target::Button(button)));
    }

    /// An area that presses `button` `times` times (a tab further than one step away).
    pub fn presses(&mut self, area: Area, button: Button, times: u8) {
        self.areas
            .push((area, if times == 1 { Target::Button(button) } else { Target::Presses(button, times) }));
    }

    /// The list's selected row, how many rows it has, and how far apart they are along x and y (0: not that way).
    pub fn list(&mut self, selected: usize, len: usize, pitch_x: f32, pitch_y: f32) {
        self.sel = Some(selected);
        self.len = len;
        self.pitch = (pitch_x, pitch_y);
    }

    /// A swipe's rows kept inside the list: some lists wrap around at their ends with the buttons, a swipe
    /// stops there.
    pub fn clamp_steps(&self, steps: i32) -> i32 {
        let Some(sel) = self.sel else { return 0 };
        let last = self.len.saturating_sub(1) as i32;
        (sel as i32 + steps).clamp(0, last) - sel as i32
    }

    pub fn pitch(&self) -> (f32, f32) {
        self.pitch
    }

    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }

    /// How many areas are registered so far (for `shift_x`).
    pub fn areas_len(&self) -> usize {
        self.areas.len()
    }

    /// The areas registered since `from` move `dx` pixels to the right: they were painted into a column that is
    /// then put in the middle of the screen (`app_column`).
    pub fn shift_x(&mut self, from: usize, dx: i32) {
        for (a, _) in self.areas.iter_mut().skip(from) {
            a.x += dx;
        }
    }

    /// What a tap at this pixel does. Later registrations lie on top of earlier ones.
    pub fn tap(&self, x: f32, y: f32) -> TapAction {
        let Some((_, target)) = self.areas.iter().rev().find(|(a, _)| a.contains(x, y)) else {
            return TapAction::Nothing;
        };
        match (*target, self.sel) {
            (Target::Button(b), _) => TapAction::Press(b),
            (Target::Presses(b, n), _) => TapAction::Presses(b, n),
            (Target::Row(i), Some(sel)) if i == sel => TapAction::Activate,
            (Target::Row(i), Some(sel)) => TapAction::Steps(i as i32 - sel as i32),
            (Target::Row(_), None) => TapAction::Nothing,
        }
    }
}

/// The button a hint pill's key stands for ("A", "SELECT", ...); `None` for combined keys such as "L1 R1".
pub fn hint_button(key: &str) -> Option<Button> {
    Some(match key {
        "A" => Button::East,
        "B" => Button::South,
        "X" => Button::North,
        "Y" => Button::West,
        "SELECT" => Button::Select,
        "START" => Button::Start,
        "L1" => Button::L1,
        "R1" => Button::R1,
        // "R2 Theme" on Settings → Look → Look.
        "R2" => Button::R2,
        // The cabinet's "UP Search" on its top shelf.
        "UP" => Button::Up,
        // "RIGHT Another" on a card of Surprise me.
        "RIGHT" => Button::Right,
        _ => return None,
    })
}
