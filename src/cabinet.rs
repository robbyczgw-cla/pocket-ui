//! The Cabinet look: the games stand as their real cases on the shelves of a wooden cabinet, one shelf per
//! system (with the recent games and the favourites on top). The selected case slides out and turns to show
//! its cover; A puts the cartridge or disc into the console.
//!
//! This is the logic: which case a system's games come in, how the shelves are laid out, where a button
//! moves the cursor, and how the animations run. Painting is `cabinet_art`, the app's glue `app_cabinet.rs`.

use std::time::{Duration, Instant};

use crate::media::Medium;
use crate::pad::Button;
use crate::shelf::Item;

/// The few case shapes the games come in.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FormFactor {
    /// Game Boy, Game Boy Advance, Game Gear, Neo Geo Pocket, WonderSwan, PICO-8.
    HandheldCart,
    /// NES, SNES, N64, Mega Drive, Master System, Atari.
    ConsoleCart,
    /// CD jewel case: PlayStation, Saturn, Dreamcast, Mega-CD, PC Engine, DOS and the rest.
    JewelCase,
    /// Tall DVD-style case: PS2, GameCube, Wii.
    DvdCase,
    /// The slim PSP case for a UMD.
    UmdCase,
    /// Nintendo DS and 3DS game card boxes.
    CardBox,
}

/// What goes into the console when a game starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Media {
    Cart,
    Disc,
    Umd,
    Card,
}

/// A case's size on the shelf, in screen pixels: the spine (what faces you on the shelf), the front (the
/// cover side, shown when the case is turned) and the height. Thicknesses are exaggerated so a 24 px title
/// fits on every spine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Size {
    pub spine: i32,
    pub front: i32,
    pub h: i32,
}

impl FormFactor {
    pub const ALL: [FormFactor; 6] = [
        FormFactor::HandheldCart,
        FormFactor::ConsoleCart,
        FormFactor::JewelCase,
        FormFactor::CardBox,
        FormFactor::UmdCase,
        FormFactor::DvdCase,
    ];

    /// The case a ROCKNIX system's games come in; a CD case for anything without a box of its own.
    pub fn of(system: &str) -> FormFactor {
        use FormFactor::*;
        match system {
            "gb" | "gbh" | "gbc" | "gbch" | "gba" | "gbah" | "gamegear" | "ggh" | "ngp" | "ngpc" | "wonderswan" | "wonderswancolor" | "atarilynx" | "lynx"
            | "virtualboy" | "pokemini" | "supervision" | "gamate" | "gameandwatch" | "arduboy" | "pico-8" | "pico8" | "tic-80" | "tic80" => HandheldCart,
            "nes" | "nesh" | "famicom" | "fds" | "snes" | "snesh" | "sfc" | "satellaview" | "sufami" | "n64" | "megadrive" | "megadriveh"
            | "megadrive-japan" | "genesis" | "genh" | "sega32x" | "mastersystem" | "sg-1000" | "sg1000" | "atari2600" | "atari5200" | "atari7800"
            | "colecovision" | "intellivision" | "vectrex" | "odyssey2" | "videopac" | "channelf" | "neogeo" | "msx" | "msx2" => ConsoleCart,
            "ps2" | "gamecube" | "wii" | "wiiware" | "wiiu" | "xbox" | "ps3" => DvdCase,
            "psp" | "pspockets" => UmdCase,
            "nds" | "3ds" | "n3ds" | "psvita" | "switch" => CardBox,
            _ => JewelCase,
        }
    }

    /// The size of its typical medium (`media::Medium`); each case stands at its own medium's size.
    pub fn size(self) -> Size {
        self.typical().size()
    }

    /// The medium most of its games come in (for the tests and anything without a system).
    pub fn typical(self) -> Medium {
        match self {
            FormFactor::HandheldCart => Medium::Gb,
            FormFactor::ConsoleCart => Medium::Snes,
            FormFactor::JewelCase => Medium::Ps1,
            FormFactor::DvdCase => Medium::Ps2,
            FormFactor::UmdCase => Medium::Umd,
            FormFactor::CardBox => Medium::Ds,
        }
    }

    pub fn is_cart(self) -> bool {
        matches!(self, FormFactor::HandheldCart | FormFactor::ConsoleCart)
    }

    pub fn media(self) -> Media {
        match self {
            FormFactor::HandheldCart | FormFactor::ConsoleCart => Media::Cart,
            FormFactor::JewelCase | FormFactor::DvdCase => Media::Disc,
            FormFactor::UmdCase => Media::Umd,
            FormFactor::CardBox => Media::Card,
        }
    }

    /// Smallest first: the order favourites are grouped in.
    fn rank(self) -> usize {
        FormFactor::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }
}

/// Room between two cases on a shelf.
pub const GAP: i32 = 3;
/// Room above the tallest case of a shelf.
pub const HEADROOM: i32 = 26;
/// The shelf board under the cases (its top and front edge).
pub const BOARD: i32 = 34;
/// Room between the cabinet's inner wall and the first and last case.
pub const INSET: i32 = 14;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ShelfKey {
    /// The recent games (and PC streaming), "Continue".
    Continue,
    Favorites,
    /// Curator (docs/CURATOR.md): the Lens, the smart shelves and the stickers as boxes, as Home's Curator row
    /// lists them.
    Curator,
    /// The games of the Lens (START in the Lens), while they are open.
    Lens,
    /// A smart shelf's games (an index into Curator's shelves): pinned to Home, or open from the Curator shelf.
    SmartShelf(usize),
    /// A sticker's games (its id): pinned to Home, or open from the Curator shelf.
    Sticker(u32),
    /// Index into the library's systems.
    System(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Case {
    pub item: Item,
    /// What kind of case: how it goes into the console.
    pub ff: FormFactor,
    /// The case itself: its picture and size (`media`).
    pub medium: Medium,
}

/// One shelf: its cases from left to right and where each stands.
#[derive(Debug)]
pub struct Shelf {
    pub key: ShelfKey,
    pub cases: Vec<Case>,
    xs: Vec<i32>,
    /// From the left end of the first case to the right end of the last, insets included.
    pub width: i32,
    /// Inside height: the tallest case and its headroom.
    pub height: i32,
}

impl Shelf {
    pub fn new(key: ShelfKey, cases: Vec<Case>) -> Shelf {
        let mut xs = Vec::with_capacity(cases.len());
        let mut x = INSET;
        for c in &cases {
            xs.push(x);
            x += c.medium.size().spine + GAP;
        }
        let width = if cases.is_empty() { 2 * INSET } else { x - GAP + INSET };
        let height = cases.iter().map(|c| c.medium.size().h).max().unwrap_or(0) + HEADROOM;
        Shelf { key, cases, xs, width, height }
    }

    /// Left edge of case `i`, from the shelf's start.
    pub fn x(&self, i: usize) -> i32 {
        self.xs.get(i).copied().unwrap_or(self.width)
    }

    pub fn spine(&self, i: usize) -> i32 {
        self.cases.get(i).map_or(0, |c| c.medium.size().spine)
    }

    /// The case standing at `x` (shelf coordinates), if any.
    pub fn at(&self, x: i32) -> Option<usize> {
        let i = self.xs.partition_point(|&left| left <= x).checked_sub(1)?;
        (x < self.xs[i] + self.spine(i)).then_some(i)
    }

    /// The case whose middle is nearest to `x` (shelf coordinates).
    pub fn nearest(&self, x: i32) -> usize {
        let i = self.xs.partition_point(|&left| left <= x);
        let mid = |i: usize| self.xs[i] + self.spine(i) / 2;
        match i {
            0 => 0,
            _ if i >= self.xs.len() => self.xs.len().saturating_sub(1),
            _ if (mid(i) - x).abs() < (mid(i - 1) - x).abs() => i,
            _ => i - 1,
        }
    }

    /// The shelf's scroll that shows case `i` in a view `view` wide, moving as little as possible from
    /// `current`; 0 when the whole shelf fits. A case coming in keeps a few of its neighbours in view.
    pub fn scroll_to_show(&self, i: usize, view: i32, current: f32) -> f32 {
        const LEAD: i32 = 120;
        if self.width <= view {
            return 0.0;
        }
        let (left, right) = ((self.x(i) - LEAD) as f32, (self.x(i) + self.spine(i) + LEAD) as f32);
        let to = if left < current {
            left
        } else if right > current + view as f32 {
            right - view as f32
        } else {
            current
        };
        to.clamp(0.0, (self.width - view) as f32)
    }
}

/// Every shelf, top to bottom.
#[derive(Debug, Default)]
pub struct Cabinet {
    pub shelves: Vec<Shelf>,
    ys: Vec<i32>,
}

impl Cabinet {
    /// The recent games first, then the favourites (standing grouped by case), then Curator's shelves in the order
    /// given (its own boxes, an open list, the pinned smart shelves and stickers), then one shelf per system; empty
    /// ones are left out.
    pub fn build(recent: Vec<Case>, mut favorites: Vec<Case>, curator: Vec<(ShelfKey, Vec<Case>)>, systems: Vec<(usize, Vec<Case>)>) -> Cabinet {
        // Like on a real shelf, the small boxes stand together and the tall ones together (a stable sort keeps
        // their order within each kind).
        favorites.sort_by_key(|c| c.ff.rank());
        let shelves: Vec<Shelf> = std::iter::once((ShelfKey::Continue, recent))
            .chain(std::iter::once((ShelfKey::Favorites, favorites)))
            .chain(curator)
            .chain(systems.into_iter().map(|(s, cases)| (ShelfKey::System(s), cases)))
            .filter(|(_, cases)| !cases.is_empty())
            .map(|(key, cases)| Shelf::new(key, cases))
            .collect();
        let mut ys = Vec::with_capacity(shelves.len());
        let mut y = 0;
        for s in &shelves {
            ys.push(y);
            y += s.height + BOARD;
        }
        Cabinet { shelves, ys }
    }

    /// Top of shelf `i`'s compartment, from the cabinet's top.
    pub fn y(&self, i: usize) -> i32 {
        self.ys.get(i).copied().unwrap_or_else(|| self.height())
    }

    /// Compartment and board of shelf `i`.
    pub fn pitch(&self, i: usize) -> i32 {
        self.shelves.get(i).map_or(0, |s| s.height + BOARD)
    }

    /// All the shelves, boards included.
    pub fn height(&self) -> i32 {
        self.ys.last().map_or(0, |y| y + self.pitch(self.ys.len() - 1))
    }

    pub fn find(&self, key: ShelfKey, item: Item) -> Option<(usize, usize)> {
        let s = self.shelves.iter().position(|s| s.key == key)?;
        Some((s, self.shelves[s].cases.iter().position(|c| c.item == item)?))
    }
}

/// Where a button moves the cursor `at` (shelf, case); None when it stays. `scroll(shelf)` is how far each
/// shelf is scrolled (up and down pick the case standing nearest on screen), `initial` a game's first letter.
pub fn step(cab: &Cabinet, at: (usize, usize), b: Button, scroll: &dyn Fn(usize) -> f32, initial: &dyn Fn(Item) -> char) -> Option<(usize, usize)> {
    let (s, i) = at;
    let shelf = cab.shelves.get(s)?;
    let last = shelf.cases.len().checked_sub(1)?;
    let letter = |k: usize| initial(shelf.cases[k].item);
    let to = match b {
        Button::Left => (s, i.checked_sub(1)?),
        Button::Right if i < last => (s, i + 1),
        Button::Up | Button::Down => {
            let t = if b == Button::Up { s.checked_sub(1)? } else { s + 1 };
            let target = cab.shelves.get(t)?;
            let on_screen = shelf.x(i) + shelf.spine(i) / 2 - scroll(s).round() as i32;
            (t, target.nearest(on_screen + scroll(t).round() as i32))
        }
        Button::R1 if i < last => {
            let here = letter(i);
            (s, (i + 1..=last).find(|&k| letter(k) != here).unwrap_or(last))
        }
        Button::L1 if i > 0 => {
            // To the start of this letter, or from there to the start of the one before.
            let from = if letter(i - 1) == letter(i) { i } else { i - 1 };
            let here = letter(from);
            let mut k = from;
            while k > 0 && letter(k - 1) == here {
                k -= 1;
            }
            (s, k)
        }
        _ => return None,
    };
    (to != at).then_some(to)
}

/// First letter for the L1/R1 jumps: digits and signs count as one group, '#'.
pub fn initial(title: &str) -> char {
    match title.chars().find(|c| c.is_alphanumeric()) {
        Some(c) if c.is_alphabetic() => c.to_uppercase().next().unwrap_or(c),
        _ => '#',
    }
}

/// How long a case takes to slide out and turn, and to go back.
pub const PULL: Duration = Duration::from_millis(240);
pub const PUSH: Duration = Duration::from_millis(160);
/// Moving the view to another shelf or along one.
pub const SCROLL: Duration = Duration::from_millis(220);
/// From A to the cartridge or disc sitting in the console and its glow fading: the launch animation
/// (Settings → Look & feel → Launch animation). The game is started at A, not at the end of it: the animation plays
/// while the emulator loads (150-300 ms on a Mini V2), and a game whose window comes up sooner simply covers it.
pub const INSERT: Duration = Duration::from_millis(300);
/// When the cartridge or disc is in: the click (`ui_sound::Sound::Insert`) and the glow come here.
pub const IN_AT: Duration = Duration::from_millis(180);
/// The same in Cozy (Settings > Look & feel > Animations): long enough to read, the same shape (IN_AT at 0.6).
pub const INSERT_COZY: Duration = Duration::from_millis(1330);
pub const IN_AT_COZY: Duration = Duration::from_millis(800);

/// The launch animation's length in this mode.
pub fn insert_len(cozy: bool) -> Duration {
    if cozy {
        INSERT_COZY
    } else {
        INSERT
    }
}

/// How long the launch fade waits after A while the launch animation plays: until the cartridge or disc is in.
pub fn fade_delay(cozy: bool) -> Duration {
    if cozy {
        IN_AT_COZY
    } else {
        IN_AT
    }
}

pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (2.0 - 2.0 * t).powi(3) / 2.0
    }
}

/// How far `now` is through `dur` from `at`, 0-1. Settings > Look > Motion: Reduced and Off make the cases
/// move at once (`motion::span`); the start of a game (`insert_at`) keeps its own clock.
fn progress(at: Instant, now: Instant, dur: Duration) -> f32 {
    let dur = crate::motion::span(crate::motion::Kind::Move, dur);
    if dur.is_zero() {
        return 1.0;
    }
    (now.saturating_duration_since(at).as_secs_f32() / dur.as_secs_f32()).min(1.0)
}

/// A value that glides to where it is sent; sent elsewhere halfway, it carries on from where it is.
#[derive(Clone, Copy, Debug)]
pub struct Tween {
    from: f32,
    to: f32,
    at: Instant,
    dur: Duration,
}

impl Tween {
    pub fn new(v: f32, dur: Duration) -> Tween {
        Tween {
            from: v,
            to: v,
            at: Instant::now(),
            dur,
        }
    }

    pub fn value(&self, now: Instant) -> f32 {
        let t = progress(self.at, now, self.dur);
        if t >= 1.0 {
            return self.to;
        }
        self.from + (self.to - self.from) * ease_out(t)
    }

    pub fn target(&self) -> f32 {
        self.to
    }

    pub fn set(&mut self, to: f32, now: Instant) {
        if to != self.to {
            self.from = self.value(now);
            self.to = to;
            self.at = now;
        }
    }

    /// Jumps there without gliding.
    pub fn jump(&mut self, to: f32) {
        (self.from, self.to) = (to, to);
    }

    pub fn moving(&self, now: Instant) -> bool {
        self.from != self.to && progress(self.at, now, self.dur) < 1.0
    }

    /// As if it had been sent `by` earlier (for snapshots in the middle of a move).
    pub fn age(&mut self, by: Duration) {
        self.at = self.at.checked_sub(by).unwrap_or(self.at);
    }
}

/// The selected case slides out; the one selected before goes back from wherever it had got to.
/// Each is (case, how far out it was when it started, when it started).
#[derive(Clone, Copy, Debug, Default)]
pub struct Pull {
    sel: Option<(Item, f32, Instant)>,
    prev: Option<(Item, f32, Instant)>,
}

impl Pull {
    /// The selection is `item`, chosen at `at`.
    pub fn follow(&mut self, item: Item, at: Instant) {
        if self.sel.is_some_and(|(s, _, _)| s == item) {
            return;
        }
        // Back to the one that is still going back: it comes out again from where it is.
        let start = self.openness(item, at);
        self.prev = self.sel.map(|(s, _, _)| (s, self.openness(s, at), at));
        self.sel = Some((item, start, at));
    }

    /// 0 = on the shelf, 1 = out and turned.
    pub fn openness(&self, item: Item, now: Instant) -> f32 {
        match (self.sel, self.prev) {
            (Some((s, start, at)), _) if s == item => start + (1.0 - start) * ease_out(progress(at, now, PULL)),
            (_, Some((p, start, at))) if p == item => start * (1.0 - ease_out(progress(at, now, PUSH))),
            _ => 0.0,
        }
    }

    /// The case going back, while it is on its way.
    pub fn going_back(&self, now: Instant) -> Option<Item> {
        self.prev
            .filter(|(_, start, at)| *start > 0.0 && progress(*at, now, PUSH) < 1.0)
            .map(|(p, _, _)| p)
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.sel.is_some_and(|(_, start, at)| start < 1.0 && progress(at, now, PULL) < 1.0) || self.going_back(now).is_some()
    }
}

/// Where the start of a game is, `elapsed` after A: the console comes up (`rise`), the case travels to it
/// (`carry`), then the cartridge or disc goes in (`sink`) and the slot lights up for a moment (`glow`, 0 until it is
/// in, up to 1 and down again); `dim` darkens the cabinet behind.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Insert {
    pub rise: f32,
    pub carry: f32,
    pub sink: f32,
    pub glow: f32,
    pub dim: f32,
}

pub fn insert_at(elapsed: Duration, cozy: bool) -> Insert {
    let t = (elapsed.as_secs_f32() / insert_len(cozy).as_secs_f32()).min(1.0);
    let phase = |a: f32, b: f32| ((t - a) / (b - a)).clamp(0.0, 1.0);
    let rise = ease_out(phase(0.0, 0.3));
    // In at IN_AT (0.6 of the way): the glow comes up fast and fades slower.
    let glow_up = phase(0.52, 0.6);
    let glow_down = phase(0.6, 1.0);
    Insert {
        rise,
        carry: ease_in_out(phase(0.06, 0.45)),
        sink: phase(0.45, 0.6).powi(2),
        glow: if t >= 1.0 { 0.0 } else { glow_up * (1.0 - glow_down * glow_down) },
        dim: 0.6 * rise,
    }
}

/// The spine colour a cover gives: its most common clear colour (a white or black border doesn't count
/// when there is colour inside); None for an empty picture.
pub fn spine_color(px: &[u32]) -> Option<u32> {
    if px.is_empty() {
        return None;
    }
    // 8 levels per channel; every pixel of a small picture, every n-th of a big one.
    let mut count = vec![0u32; 512];
    let mut sum = vec![[0u32; 3]; 512];
    let every = (px.len() / 40_000).max(1);
    for &p in px.iter().step_by(every) {
        let (r, g, b) = (p >> 16 & 0xff, p >> 8 & 0xff, p & 0xff);
        let k = ((r >> 5) << 6 | (g >> 5) << 3 | b >> 5) as usize;
        count[k] += 1;
        sum[k][0] += r;
        sum[k][1] += g;
        sum[k][2] += b;
    }
    let mean = |k: usize| {
        let n = count[k].max(1);
        (sum[k][0] / n, sum[k][1] / n, sum[k][2] / n)
    };
    // Clear colours count for more than greys; paper white and black borders for much less.
    let weight = |k: usize| {
        let (r, g, b) = mean(k);
        let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
        if lo > 215 || hi < 36 {
            0.15
        } else {
            0.4 + (hi - lo) as f32 / hi.max(1) as f32
        }
    };
    let best = (0..512)
        .filter(|&k| count[k] > 0)
        .max_by(|&a, &b| (count[a] as f32 * weight(a)).total_cmp(&(count[b] as f32 * weight(b))))?;
    let (r, g, b) = mean(best);
    let c = r << 16 | g << 8 | b;
    // A black spine is still a spine: lifted just enough not to vanish into the cabinet.
    let lift = 28u32.saturating_sub(luma(c));
    Some(if lift == 0 {
        c
    } else {
        (r + lift).min(255) << 16 | (g + lift).min(255) << 8 | (b + lift).min(255)
    })
}

/// Ink for a title on `bg`: dark on light spines, light on dark ones.
pub fn ink_on(bg: u32) -> u32 {
    if luma(bg) > 150 {
        0x1a1714
    } else {
        0xf2eee6
    }
}

/// Perceived brightness, 0-255.
pub fn luma(c: u32) -> u32 {
    ((c >> 16 & 0xff) * 299 + (c >> 8 & 0xff) * 587 + (c & 0xff) * 114) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(s: usize, g: usize, ff: FormFactor) -> Case {
        Case { item: Item::Game(s, g), ff, medium: ff.typical() }
    }

    fn shelf_of(ff: FormFactor, n: usize) -> Shelf {
        Shelf::new(ShelfKey::System(0), (0..n).map(|g| case(0, g, ff)).collect())
    }

    #[test]
    fn each_system_gets_the_case_its_games_came_in() {
        use FormFactor::*;
        for (system, want) in [
            ("gb", HandheldCart),
            ("gbc", HandheldCart),
            ("gba", HandheldCart),
            ("gamegear", HandheldCart),
            ("pico-8", HandheldCart),
            ("nes", ConsoleCart),
            ("snes", ConsoleCart),
            ("sfc", ConsoleCart),
            ("n64", ConsoleCart),
            ("megadrive", ConsoleCart),
            ("genesis", ConsoleCart),
            ("psx", JewelCase),
            ("saturn", JewelCase),
            ("dreamcast", JewelCase),
            ("ps2", DvdCase),
            ("gamecube", DvdCase),
            ("wii", DvdCase),
            ("psp", UmdCase),
            ("nds", CardBox),
            ("3ds", CardBox),
            ("psvita", CardBox),
            ("something-new", JewelCase),
        ] {
            assert_eq!(FormFactor::of(system), want, "{system}");
        }
    }

    #[test]
    fn cases_keep_their_real_proportions_and_room_for_a_title() {
        use FormFactor::*;
        let h = |f: FormFactor| f.size().h;
        assert!(
            h(DvdCase) > h(UmdCase) && h(UmdCase) > h(CardBox) && h(CardBox) > h(JewelCase),
            "tallest to shortest"
        );
        assert!(h(HandheldCart) < h(ConsoleCart), "a Game Boy cart is the smallest");
        assert!(ConsoleCart.size().spine > DvdCase.size().spine, "a cartridge is thicker than a DVD case");
        assert!(JewelCase.size().front > JewelCase.size().h, "a jewel case is wider than tall");
        assert!(
            DvdCase.size().front < DvdCase.size().h && UmdCase.size().front < CardBox.size().front,
            "DVD and UMD cases are tall and narrow"
        );
        for f in FormFactor::ALL {
            // The PlayStation's jewel case is the thin one: 26 px, a 20 px title.
            assert!(f.size().spine >= 26, "{f:?}: a title fits on the spine");
            assert!(f.size().front > f.size().spine * 3, "{f:?}");
        }
        assert!(HandheldCart.is_cart() && ConsoleCart.is_cart() && !JewelCase.is_cart() && !CardBox.is_cart());
        assert_eq!(JewelCase.media(), Media::Disc);
        assert_eq!(DvdCase.media(), Media::Disc);
        assert_eq!(UmdCase.media(), Media::Umd);
        assert_eq!(CardBox.media(), Media::Card);
        assert_eq!(ConsoleCart.media(), Media::Cart);
    }

    #[test]
    fn shelves_go_recent_favorites_then_systems_and_empty_ones_are_left_out() {
        let recent = vec![case(1, 0, FormFactor::ConsoleCart)];
        let favorites = vec![case(0, 2, FormFactor::HandheldCart)];
        let systems = vec![
            (0, vec![case(0, 0, FormFactor::HandheldCart)]),
            (1, Vec::new()),
            (3, vec![case(3, 0, FormFactor::DvdCase)]),
        ];
        let cab = Cabinet::build(recent, favorites, Vec::new(), systems);
        let keys: Vec<ShelfKey> = cab.shelves.iter().map(|s| s.key).collect();
        assert_eq!(keys, vec![ShelfKey::Continue, ShelfKey::Favorites, ShelfKey::System(0), ShelfKey::System(3)]);
        let cab = Cabinet::build(Vec::new(), Vec::new(), Vec::new(), vec![(2, vec![case(2, 0, FormFactor::JewelCase)])]);
        assert_eq!(
            cab.shelves.iter().map(|s| s.key).collect::<Vec<_>>(),
            vec![ShelfKey::System(2)],
            "no recent games, no favourites"
        );
        assert!(Cabinet::build(Vec::new(), Vec::new(), Vec::new(), Vec::new()).shelves.is_empty());
    }

    #[test]
    fn curator_s_shelves_stand_between_the_favourites_and_the_systems() {
        let boxes = vec![Case { item: Item::Lens, ff: FormFactor::DvdCase, medium: Medium::Keep }, Case { item: Item::SmartShelf(0), ff: FormFactor::DvdCase, medium: Medium::Keep }];
        let curator = vec![
            (ShelfKey::Curator, boxes),
            (ShelfKey::SmartShelf(0), vec![case(1, 1, FormFactor::ConsoleCart)]),
            (ShelfKey::Sticker(7), Vec::new()),
            (ShelfKey::Sticker(3), vec![case(0, 1, FormFactor::HandheldCart)]),
        ];
        let cab = Cabinet::build(vec![case(1, 0, FormFactor::ConsoleCart)], Vec::new(), curator, vec![(0, vec![case(0, 0, FormFactor::HandheldCart)])]);
        let keys: Vec<ShelfKey> = cab.shelves.iter().map(|s| s.key).collect();
        assert_eq!(
            keys,
            vec![ShelfKey::Continue, ShelfKey::Curator, ShelfKey::SmartShelf(0), ShelfKey::Sticker(3), ShelfKey::System(0)],
            "in the order given, an empty sticker left out"
        );
        assert_eq!(cab.find(ShelfKey::Curator, Item::SmartShelf(0)), Some((1, 1)));
        assert_eq!(cab.find(ShelfKey::SmartShelf(0), Item::Game(1, 1)), Some((2, 0)), "a game on a smart shelf and on its system's");
        // Up from the systems reaches the stickers, the shelves and the boxes on the way.
        let none = |_: usize| 0.0;
        let initial = |_: Item| 'A';
        assert_eq!(step(&cab, (4, 0), Button::Up, &none, &initial), Some((3, 0)));
        assert_eq!(step(&cab, (2, 0), Button::Up, &none, &initial).map(|a| a.0), Some(1));
    }

    #[test]
    fn favourites_stand_grouped_by_case_in_their_own_order_recent_games_stay_in_playing_order() {
        use FormFactor::*;
        let mixed = vec![
            case(0, 0, DvdCase),
            case(1, 0, HandheldCart),
            case(2, 0, DvdCase),
            case(1, 1, HandheldCart),
            case(3, 0, JewelCase),
        ];
        let cab = Cabinet::build(mixed.clone(), mixed, Vec::new(), Vec::new());
        let fav: Vec<Case> = cab.shelves[1].cases.clone();
        assert_eq!(
            fav,
            vec![
                case(1, 0, HandheldCart),
                case(1, 1, HandheldCart),
                case(3, 0, JewelCase),
                case(0, 0, DvdCase),
                case(2, 0, DvdCase)
            ]
        );
        let recent: Vec<Item> = cab.shelves[0].cases.iter().map(|c| c.item).collect();
        assert_eq!(
            recent,
            vec![Item::Game(0, 0), Item::Game(1, 0), Item::Game(2, 0), Item::Game(1, 1), Item::Game(3, 0)],
            "last played first"
        );
    }

    #[test]
    fn cases_stand_side_by_side_and_the_shelf_is_as_tall_as_its_tallest() {
        use FormFactor::*;
        let s = Shelf::new(ShelfKey::Continue, vec![case(0, 0, HandheldCart), case(1, 0, DvdCase), case(2, 0, JewelCase)]);
        assert_eq!(s.x(0), INSET);
        assert_eq!(s.x(1), INSET + HandheldCart.size().spine + GAP);
        assert_eq!(s.x(2), s.x(1) + DvdCase.size().spine + GAP);
        assert_eq!(s.width, s.x(2) + JewelCase.size().spine + INSET);
        assert_eq!(s.height, DvdCase.size().h + HEADROOM);
        assert_eq!(
            shelf_of(HandheldCart, 3).height,
            HandheldCart.size().h + HEADROOM,
            "a Game Boy shelf is a low one"
        );
    }

    #[test]
    fn shelves_stack_by_their_own_heights() {
        use FormFactor::*;
        let cab = Cabinet::build(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![
                (0, vec![case(0, 0, DvdCase)]),
                (1, vec![case(1, 0, HandheldCart)]),
                (2, vec![case(2, 0, JewelCase)]),
            ],
        );
        assert_eq!(cab.y(0), 0);
        assert_eq!(cab.pitch(0), DvdCase.size().h + HEADROOM + BOARD);
        assert_eq!(cab.y(1), cab.pitch(0));
        assert_eq!(cab.y(2), cab.pitch(0) + cab.pitch(1));
        assert!(cab.pitch(1) < cab.pitch(2) && cab.pitch(2) < cab.pitch(0));
        assert_eq!(cab.find(ShelfKey::System(1), Item::Game(1, 0)), Some((1, 0)));
        assert_eq!(cab.find(ShelfKey::System(1), Item::Game(2, 0)), None);
        assert_eq!(cab.find(ShelfKey::Favorites, Item::Game(1, 0)), None);
    }

    #[test]
    fn a_tap_finds_the_case_under_the_finger() {
        let s = shelf_of(FormFactor::DvdCase, 4);
        let w = FormFactor::DvdCase.size().spine;
        assert_eq!(s.at(s.x(0)), Some(0));
        assert_eq!(s.at(s.x(2) + w / 2), Some(2));
        assert_eq!(s.at(s.x(3) + w - 1), Some(3));
        assert_eq!(s.at(s.x(3) + w + 1), None, "past the last case");
        assert_eq!(s.at(0), None, "the inset before the first");
        assert_eq!(s.nearest(-500), 0);
        assert_eq!(s.nearest(s.x(2) + w / 2 + 1), 2);
        assert_eq!(s.nearest(99_999), 3);
    }

    #[test]
    fn the_shelf_scrolls_only_as_far_as_needed_and_never_past_its_ends() {
        let s = shelf_of(FormFactor::DvdCase, 60);
        let view = 1000;
        assert_eq!(s.scroll_to_show(0, view, 0.0), 0.0);
        assert_eq!(s.scroll_to_show(5, view, 0.0), 0.0, "already in view: no move");
        let far = s.scroll_to_show(40, view, 0.0);
        assert!(far > 0.0);
        let right = s.x(40) as f32 + FormFactor::DvdCase.size().spine as f32 - far;
        assert!(
            right <= view as f32 && right > view as f32 - 260.0,
            "comes in near the right edge, with room for the next ones: {right}"
        );
        assert_eq!(
            s.scroll_to_show(59, view, 0.0),
            (s.width - view) as f32,
            "the last case: the shelf's end at the view's end"
        );
        let back = s.scroll_to_show(39, view, far);
        assert_eq!(back, far, "one to the left is still in view");
        assert_eq!(s.scroll_to_show(0, view, far), 0.0);
        let short = shelf_of(FormFactor::DvdCase, 5);
        assert_eq!(short.scroll_to_show(4, view, 30.0), 0.0, "a shelf that fits doesn't scroll");
    }

    fn cab3() -> Cabinet {
        use FormFactor::*;
        Cabinet::build(
            vec![case(9, 0, DvdCase), case(9, 1, DvdCase)],
            Vec::new(),
            Vec::new(),
            vec![
                (0, (0..30).map(|g| case(0, g, HandheldCart)).collect()),
                (1, (0..8).map(|g| case(1, g, JewelCase)).collect()),
            ],
        )
    }

    #[test]
    fn left_and_right_move_along_the_shelf_and_stop_at_its_ends() {
        let cab = cab3();
        let none = |_: usize| 0.0;
        let a = |_: Item| 'A';
        assert_eq!(step(&cab, (1, 3), Button::Right, &none, &a), Some((1, 4)));
        assert_eq!(step(&cab, (1, 3), Button::Left, &none, &a), Some((1, 2)));
        assert_eq!(step(&cab, (1, 0), Button::Left, &none, &a), None);
        assert_eq!(step(&cab, (1, 29), Button::Right, &none, &a), None);
        assert_eq!(step(&cab, (0, 0), Button::Up, &none, &a), None, "the top shelf");
        assert_eq!(step(&cab, (2, 0), Button::Down, &none, &a), None, "the bottom shelf");
        assert_eq!(step(&cab, (1, 0), Button::South, &none, &a), None, "not a direction");
    }

    #[test]
    fn up_and_down_pick_the_case_standing_nearest_on_screen() {
        let cab = cab3();
        let a = |_: Item| 'A';
        // The Game Boy shelf scrolled 300 px: its case 20 stands where the jewel case shelf's case ...
        let scroll = |s: usize| if s == 1 { 300.0 } else { 0.0 };
        let (s, i) = step(&cab, (1, 20), Button::Down, &scroll, &a).unwrap();
        assert_eq!(s, 2);
        let x_on_screen = cab.shelves[1].x(20) - 300 + FormFactor::HandheldCart.size().spine / 2;
        assert_eq!(i, cab.shelves[2].nearest(x_on_screen));
        assert_eq!(step(&cab, (1, 0), Button::Up, &scroll, &a), Some((0, 0)));
        // From far right on a long shelf to a short one: its last case.
        assert_eq!(step(&cab, (1, 29), Button::Down, &|_| 0.0, &a), Some((2, 7)));
    }

    #[test]
    fn l1_and_r1_jump_to_the_next_and_previous_letter() {
        let cab = cab3();
        let none = |_: usize| 0.0;
        // Games 0-4 "A…", 5-11 "B…", 12-29 "D…".
        let letter = |item: Item| match item {
            Item::Game(_, g) if g < 5 => 'A',
            Item::Game(_, g) if g < 12 => 'B',
            _ => 'D',
        };
        assert_eq!(step(&cab, (1, 2), Button::R1, &none, &letter), Some((1, 5)));
        assert_eq!(step(&cab, (1, 5), Button::R1, &none, &letter), Some((1, 12)));
        assert_eq!(step(&cab, (1, 12), Button::R1, &none, &letter), Some((1, 29)), "the last letter: to the end");
        assert_eq!(step(&cab, (1, 29), Button::R1, &none, &letter), None);
        assert_eq!(
            step(&cab, (1, 14), Button::L1, &none, &letter),
            Some((1, 12)),
            "to the start of its letter first"
        );
        assert_eq!(step(&cab, (1, 12), Button::L1, &none, &letter), Some((1, 5)), "then the letter before");
        assert_eq!(step(&cab, (1, 0), Button::L1, &none, &letter), None);
        assert_eq!(initial("Legend of Zelda, The"), 'L');
        assert_eq!(initial("super Metroid"), 'S');
        assert_eq!(initial("1942"), '#');
        assert_eq!(initial("'98 Koshien"), '#');
        assert_eq!(initial(""), '#');
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_tween_glides_and_carries_on_from_where_it_is() {
        let t0 = Instant::now();
        let mut t = Tween::new(0.0, ms(200));
        assert_eq!(t.value(t0), 0.0);
        t.set(100.0, t0);
        assert!(t.moving(t0 + ms(100)));
        let mid = t.value(t0 + ms(100));
        assert!(mid > 50.0 && mid < 100.0, "eases out: past halfway at half the time, {mid}");
        assert_eq!(t.value(t0 + ms(200)), 100.0);
        assert!(!t.moving(t0 + ms(200)));
        t.set(0.0, t0 + ms(100));
        assert!((t.value(t0 + ms(100)) - mid).abs() < 0.01, "no jump when sent back halfway");
        assert_eq!(t.target(), 0.0);
        t.jump(42.0);
        assert_eq!(t.value(t0), 42.0);
        assert!(!t.moving(t0));
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert_eq!(ease_in_out(0.5), 0.5);
    }

    #[test]
    fn the_selected_case_slides_out_and_the_last_one_goes_back() {
        let t0 = Instant::now();
        let (a, b, c) = (Item::Game(0, 0), Item::Game(0, 1), Item::Game(0, 2));
        let mut p = Pull::default();
        p.follow(a, t0);
        assert_eq!(p.openness(a, t0), 0.0);
        assert_eq!(p.openness(a, t0 + PULL), 1.0);
        assert!(p.animating(t0 + PULL / 2));
        assert!(!p.animating(t0 + PULL));
        // Following the same selection again changes nothing.
        p.follow(a, t0 + PULL);
        assert_eq!(p.openness(a, t0 + PULL), 1.0);
        let t1 = t0 + PULL * 2;
        p.follow(b, t1);
        assert_eq!(p.openness(a, t1), 1.0, "goes back from all the way out");
        assert!(p.openness(a, t1 + PUSH / 2) < 1.0);
        assert_eq!(p.openness(a, t1 + PUSH), 0.0);
        assert_eq!(p.openness(c, t1), 0.0, "everything else stays in");
        // Moving on before b is out: b goes back from where it had got to, never jumping out first.
        let t2 = t1 + PULL / 3;
        let part = p.openness(b, t2);
        assert!(part > 0.0 && part < 1.0);
        p.follow(c, t2);
        assert!((p.openness(b, t2) - part).abs() < 0.001);
        assert!(p.openness(b, t2 + PUSH / 4) < part);
        assert_eq!(p.openness(a, t2), 0.0, "only the last one is still going back");
        assert!(!p.animating(t2 + PULL.max(PUSH)));
    }

    #[test]
    fn a_game_start_brings_the_console_carries_the_case_then_puts_it_in() {
        let start = insert_at(Duration::ZERO, false);
        assert_eq!((start.rise, start.carry, start.sink, start.glow), (0.0, 0.0, 0.0, 0.0));
        let end = insert_at(INSERT, false);
        assert_eq!((end.rise, end.carry, end.sink, end.glow), (1.0, 1.0, 1.0, 0.0));
        assert_eq!(insert_at(INSERT * 3, false), end, "stays put after");
        let mut last = start;
        for k in 1..=52 {
            let s = insert_at(INSERT * k / 52, false);
            assert!(
                s.rise >= last.rise && s.carry >= last.carry && s.sink >= last.sink && s.dim >= last.dim,
                "never goes back at step {k}"
            );
            last = s;
        }
        let early = insert_at(INSERT / 5, false);
        assert!(early.rise > 0.0 && early.sink == 0.0 && early.glow == 0.0, "the console comes first");
        let late = insert_at(INSERT / 2, false);
        assert!(late.carry == 1.0 && late.sink > 0.0, "it goes in once it is there");
        assert!(end.dim > 0.3 && end.dim < 0.9, "the cabinet stays faintly there under the fade");
    }

    #[test]
    fn the_cartridge_is_in_and_glows_while_a_game_still_loads_and_the_fade_waits_for_it() {
        // The game starts at A; the animation has to say its part within a typical start (150-300 ms on a Mini V2).
        assert!(INSERT <= Duration::from_millis(300), "{INSERT:?}");
        let at = insert_at(IN_AT, false);
        assert!(at.carry == 1.0 && at.sink > 0.98, "in at IN_AT: {at:?}");
        assert!(at.glow > 0.8, "and the slot lights up: {at:?}");
        let before = insert_at(IN_AT - Duration::from_millis(40), false);
        assert!(before.glow == 0.0, "no glow before it is in: {before:?}");
        assert!(insert_at(INSERT - Duration::from_millis(30), false).glow < at.glow, "the glow fades");
        assert_eq!(fade_delay(false), IN_AT, "the fade starts once it is in");
    }

    #[test]
    fn cozy_is_the_same_animation_over_about_a_second_with_the_fade_waiting_for_the_cartridge() {
        assert!(INSERT_COZY >= Duration::from_millis(1000) && INSERT_COZY <= Duration::from_millis(1500), "{INSERT_COZY:?}");
        // The game's window is held until the fade has run (`launch_hold`): that is what a cozy start costs at most.
        assert!(fade_delay(true) + Duration::from_millis(250) <= Duration::from_millis(1100));
        assert!((IN_AT_COZY.as_secs_f32() / INSERT_COZY.as_secs_f32() - 0.6).abs() < 0.01, "the cartridge is in at 0.6 of the way, as in Quick");
        let at = insert_at(IN_AT_COZY, true);
        assert!(at.carry == 1.0 && at.sink > 0.98 && at.glow > 0.8, "in at IN_AT_COZY: {at:?}");
        assert!(insert_at(IN_AT_COZY - Duration::from_millis(150), true).glow == 0.0, "no glow before it is in");
        let mid = insert_at(INSERT_COZY / 4, true);
        assert!(mid.rise > 0.9 && mid.carry < 1.0, "slow enough to see the case travel: {mid:?}");
        let end = insert_at(INSERT_COZY, true);
        assert_eq!((end.rise, end.carry, end.sink, end.glow), (1.0, 1.0, 1.0, 0.0));
    }

    fn picture(parts: &[(u32, usize)]) -> Vec<u32> {
        parts.iter().flat_map(|(c, n)| std::iter::repeat(*c).take(*n)).collect()
    }

    fn close(a: u32, b: u32) -> bool {
        let d = |s: u32| ((a >> s & 0xff) as i32 - (b >> s & 0xff) as i32).abs();
        d(16) < 24 && d(8) < 24 && d(0) < 24
    }

    #[test]
    fn the_spine_takes_the_covers_main_colour() {
        let red = 0xc0_2020;
        let c = spine_color(&picture(&[(red, 900), (0x2040c0, 100)])).unwrap();
        assert!(close(c, red), "{c:06x}");
        // A white frame around a blue picture: the blue.
        let blue = 0x2050b0;
        let c = spine_color(&picture(&[(0xffffff, 500), (blue, 400), (0x101010, 100)])).unwrap();
        assert!(close(c, blue), "{c:06x}");
        // A grey cover stays grey; a black one is kept dark but not lost against the cabinet.
        let c = spine_color(&picture(&[(0x808080, 1000)])).unwrap();
        assert!(close(c, 0x808080), "{c:06x}");
        let c = spine_color(&picture(&[(0x000000, 1000)])).unwrap();
        assert!(luma(c) >= 20 && luma(c) < 60, "{c:06x}");
        assert_eq!(spine_color(&[]), None);
        // Noise around one colour still finds it.
        let noisy: Vec<u32> = (0..2000u32)
            .map(|i| if i % 3 == 0 { i.wrapping_mul(2654435761) & 0xffffff } else { 0x3a8a3a + (i % 7) })
            .collect();
        assert!(close(spine_color(&noisy).unwrap(), 0x3a8a3a));
    }

    #[test]
    fn titles_are_dark_on_light_spines_and_light_on_dark_ones() {
        assert!(luma(ink_on(0xf0e8d8)) < 60);
        assert!(luma(ink_on(0xffd000)) < 60, "yellow is light");
        assert!(luma(ink_on(0x202020)) > 180);
        assert!(luma(ink_on(0x2050b0)) > 180, "blue is dark");
    }
}
