//! The pause menu's Zoom page: the frozen picture of the game, larger (`app_zoom.rs` paints and reads the buttons).
//!
//! The picture is pocket-core's `shot` (the last frame as the screen showed it, whole; `preview::SHOT_FILE`) that the
//! menu already holds as its background, so Zoom costs no new work in the game and no new file. It shows a part of
//! it, 2x or 3x as large, and the D-pad moves that part. The place is kept as a share of the way the part can move
//! (0 to 1000 per mille, 500 the middle), so the same place stays in view when the level changes and on every
//! resolution.

use crate::pad::Button;

/// How much larger the picture gets: the two steps A switches between.
pub const LEVELS: [u32; 2] = [2, 3];

/// A press moves the part a quarter of its own size.
const STEP_QUARTERS: i32 = 4;

/// Handheld systems start Zoom at the lower edge of the picture: that is where their dialogue boxes are.
pub fn starts_low(system: &str) -> bool {
    matches!(system, "gb" | "gbc" | "gba" | "nds" | "psp" | "gamegear" | "ngp" | "ngpc" | "lynx" | "wonderswan" | "wonderswancolor" | "supervision" | "pokemini" | "virtualboy")
}

/// What a button on the page asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Nothing,
    /// Back to the menu.
    Close,
    /// The part or the level changed: paint again.
    Moved,
}

/// The part of the picture in view.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Zoom {
    /// Index into `LEVELS`.
    level: usize,
    /// The part's place, per mille of the way it can move: 0 is the left and top edge, 1000 the right and bottom edge.
    x: i32,
    y: i32,
}

impl Zoom {
    /// In the middle, at `level` times (anything but 3 is 2).
    pub fn new(level: u32) -> Zoom {
        Zoom { level: LEVELS.iter().position(|l| *l == level).unwrap_or(0), x: 500, y: 500 }
    }

    /// Centred across and at the bottom edge of the game picture, where a handheld's dialogue boxes are (`starts_low`).
    pub fn low(level: u32) -> Zoom {
        Zoom { y: 1000, ..Zoom::new(level) }
    }

    pub fn level(&self) -> u32 {
        LEVELS[self.level]
    }

    /// "2x" or "3x".
    pub fn label(&self) -> String {
        format!("{}x", self.level())
    }

    /// Per mille of the way the part can move that one press moves it: a quarter of the part's width when it is
    /// 1/level of the picture, which can move (level - 1)/level of it.
    fn step(&self) -> i32 {
        1000 / (STEP_QUARTERS * (self.level() as i32 - 1))
    }

    /// The part in view, in pixels of the screen picture (`frame_w` x `frame_h`): (x, y, width, height). It is the
    /// screen's share 1/level (so the game appears `level` times as large as on the screen) and moves inside the game's
    /// own rectangle `game` (the picture without the black borders or the screen frame; None: the whole screen). A game
    /// smaller than that share is shown whole.
    pub fn part(&self, frame_w: u32, frame_h: u32, game: Option<(u32, u32, u32, u32)>) -> (u32, u32, u32, u32) {
        let l = self.level();
        if frame_w == 0 || frame_h == 0 { return (0, 0, 0, 0); }
        let game = game.filter(|&(x, y, w, h)| x < frame_w && y < frame_h && w > 0 && h > 0);
        let (gx, gy, gw, gh) = game.map(|(x, y, w, h)| (x.min(frame_w), y.min(frame_h), w, h)).unwrap_or((0, 0, frame_w, frame_h));
        let (gw, gh) = (gw.min(frame_w - gx).max(1), gh.min(frame_h - gy).max(1));
        let (pw, ph) = ((frame_w / l).max(1).min(gw), (frame_h / l).max(1).min(gh));
        let (sx, sy) = ((gw - pw) as u64 * self.x as u64 / 1000, (gh - ph) as u64 * self.y as u64 / 1000);
        (gx + sx as u32, gy + sy as u32, pw, ph)
    }

    /// The D-pad moves the part (stopping at the picture's edge), `switch` (A) the level, `back` (B) leaves the page.
    pub fn press(&mut self, b: Button, switch: Button, back: Button) -> Action {
        let step = self.step();
        let before = *self;
        match b {
            b if b == back => return Action::Close,
            Button::Left => self.x = (self.x - step).max(0),
            Button::Right => self.x = (self.x + step).min(1000),
            Button::Up => self.y = (self.y - step).max(0),
            Button::Down => self.y = (self.y + step).min(1000),
            b if b == switch || b == Button::L1 || b == Button::R1 => self.level = (self.level + 1) % LEVELS.len(),
            _ => return Action::Nothing,
        }
        if *self == before {
            Action::Nothing
        } else {
            Action::Moved
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Button = Button::East;
    const B: Button = Button::South;

    #[test]
    fn it_opens_in_the_middle_showing_a_half_or_a_third_of_the_screen() {
        assert_eq!(Zoom::new(2).part(1240, 1080, None), (310, 270, 620, 540));
        assert_eq!(Zoom::new(3).part(1200, 900, None), (400, 300, 400, 300));
        assert_eq!(Zoom::new(7).level(), 2, "an unknown level is the first");
    }

    #[test]
    fn the_part_stays_inside_the_game_not_its_black_borders() {
        // A Game Boy Advance at 5x on the Mini V2's screen: 1200 x 800, centred.
        let game = Some((20, 140, 1200, 800));
        let mut z = Zoom::new(2);
        assert_eq!(z.part(1240, 1080, game), (20 + 290, 140 + 130, 620, 540), "the middle of the game");
        for _ in 0..8 {
            z.press(Button::Left, A, B);
            z.press(Button::Up, A, B);
        }
        assert_eq!(z.part(1240, 1080, game), (20, 140, 620, 540), "the game's top left corner, not the screen's");
        for _ in 0..8 {
            z.press(Button::Right, A, B);
            z.press(Button::Down, A, B);
        }
        assert_eq!(z.part(1240, 1080, game), (20 + 580, 140 + 260, 620, 540), "the game's bottom right corner");
    }

    #[test]
    fn handhelds_start_at_the_bottom_where_the_dialogue_is() {
        let game = Some((20, 140, 1200, 800));
        let low = Zoom::low(2);
        assert_eq!(low.part(1240, 1080, game), (20 + 290, 140 + 260, 620, 540));
        assert!(starts_low("gba") && starts_low("gb") && starts_low("nds") && !starts_low("snes") && !starts_low("psx"));
    }

    #[test]
    fn a_game_smaller_than_the_part_is_shown_whole() {
        assert_eq!(Zoom::new(2).part(1240, 1080, Some((400, 400, 300, 200))), (400, 400, 300, 200));
    }

    #[test]
    fn invalid_game_bounds_fall_back_to_a_visible_picture() {
        for (w, h) in [(640, 480), (1024, 768), (1240, 1080), (1920, 1080)] {
            for rect in [(w, 0, 1, 1), (0, h, 1, 1), (0, 0, 0, 1)] {
                assert_eq!(Zoom::new(2).part(w, h, Some(rect)), Zoom::new(2).part(w, h, None));
            }
        }
    }

    #[test]
    fn a_press_moves_a_quarter_of_the_way_at_2x() {
        let mut z = Zoom::new(2);
        assert_eq!(z.press(Button::Right, A, B), Action::Moved);
        // The way the part can move is 620 px; 250 per mille of it is 155, a quarter of the part's 620.
        assert_eq!(z.part(1240, 1080, None).0, 310 + 155);
    }

    #[test]
    fn a_switches_the_level_and_keeps_the_place_b_leaves() {
        let mut z = Zoom::new(2);
        z.press(Button::Right, A, B);
        z.press(Button::Right, A, B);
        let share = z.x;
        assert_eq!(z.press(A, A, B), Action::Moved);
        assert_eq!((z.level(), z.label(), z.x), (3, "3x".to_string(), share));
        assert_eq!(z.press(A, A, B), Action::Moved);
        assert_eq!(z.level(), 2, "round again");
        assert_eq!(z.press(B, A, B), Action::Close);
        assert_eq!(z.press(Button::Start, A, B), Action::Nothing);
    }

    #[test]
    fn the_part_is_always_inside_the_picture_whatever_the_size() {
        for (w, h) in [(1240u32, 1080u32), (1920, 1080), (1280, 960), (1024, 768), (640, 480), (3, 3), (1, 1)] {
            for level in LEVELS {
                for (x, y) in [(0, 0), (500, 500), (1000, 1000), (0, 1000)] {
                    let z = Zoom { level: LEVELS.iter().position(|l| *l == level).unwrap(), x, y };
                    let (px, py, pw, ph) = z.part(w, h, None);
                    assert!(pw > 0 && ph > 0, "even a tiny picture stays visible");
                    assert!(px + pw <= w && py + ph <= h, "{w}x{h} {level}x {x},{y}: {px},{py} {pw}x{ph}");
                }
            }
        }
    }
}
