//! Cozy's way back from a game on a phone, as on the handhelds (`pocket_ui::eject`): after Save & quit the game's last
//! picture switches off (an old TV's line and dot, a handheld's display going dark), then the start's own scene
//! (`insert::Launch`, the same `insert_scene`) runs backwards over the page: the medium comes out of its console and goes
//! back into its case, which stands in its place on the shelf; what is inside then comes a little out of its side on its
//! own clock (`Ui::ajar`). Only Cozy with Motion Full, from a page that shows the case (as the start). Nothing waits for
//! it: the page under it is the shelf after Quit, and any button or touch ends it and does what it does there.
//!
//! The player is another process: it leaves the last picture and where it stood (`eject::LastPicture`).
use super::insert::{self, Launch};
use pocket_ui::{
    cabinet::FormFactor,
    cabinet_art::Pic,
    document_library::Game,
    eject::{self, LastPicture, Moment, Screen, Way},
    gfx::Canvas,
    image::Image,
    media,
    motion::Motion,
    text::Text,
    ui_sound,
};
use std::time::{Duration, Instant};

pub struct Eject {
    /// When the launcher was back: the way back's clock.
    pub at: Instant,
    pub way: Way,
    /// The game's last picture (`LastPicture::picture`).
    frame: Option<Image>,
    /// Where it stood on the canvas.
    area: (i32, i32, i32, i32),
    /// The start's scene of this game from the case's place on the page, played backwards. Made after the first frame
    /// is on screen (`attach`): the picture switching off needs none of it, and the way back is not slower to come.
    scene: Option<Launch>,
    medium: media::Medium,
    /// How a handheld's display switches off (Handheld switch-on, `lcd_on`); `Plain` simply goes dark.
    panel: pocket_ui::lcd_on::Panel,
    /// The canvas and the window it was made for: turned meanwhile, it ends.
    size: (u32, u32),
    window: (u32, u32),
    /// The game's key on the shelf (`Game::key`).
    pub key: String,
}

/// Why the way back doesn't show (said in the log), or None when it does: Cozy with Motion Full, a page with the case on
/// it, and the game just played selected there.
pub fn why_not(cozy: bool, motion: Motion, on_shelf: bool, on_the_game: bool) -> Option<&'static str> {
    if !cozy || motion != Motion::Full {
        Some("not Cozy with Motion Full")
    } else if !on_shelf {
        Some("the shelf is not in front")
    } else if !on_the_game {
        Some("the shelf is not on the game")
    } else {
        None
    }
}

/// The way back of `game`: its last picture switches off first when there is one (a TV's only with the old-TV
/// switch-on, as on the handhelds), and its console opens a lid or tray first when it has one.
pub fn way(game: &Game, has_picture: bool) -> Way {
    let system = insert::rig_system(game);
    let screen = Screen::of(FormFactor::of(system));
    // The Mobile view has the one old-TV row (switch-on); the handheld view's switch-off row is read as well.
    let crt = pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::CrtOn) && pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::CrtOff);
    let kind = media::Game::of(system, &game.relative);
    Way::new(screen, has_picture && (screen == Screen::Lcd || crt), pocket_ui::playful::on() && pocket_ui::insert_rig::takes_disc(system, kind))
}

/// The page the way back plays over, as the launcher painted it (`Ui::back_from`, then a paint with `Ui::launching` and
/// `Ui::returning`): its pixels at the screen's size `size`, the window `window`, where the canvas starts in the window
/// (`offset`, the safe area) and its pixels per page pixel (`scale`, the game card's canvas is smaller).
pub struct Page {
    pub pixels: Vec<u32>,
    pub size: (u32, u32),
    pub window: (u32, u32),
    pub offset: (i32, i32),
    pub scale: (f32, f32),
    /// What the page leaves out round the case's place, canvas pixels (`Ui::page_marks`): it comes with the case.
    pub marks: Vec<pocket_ui::insert_scene::Mark>,
}

/// The start's own scene of `game` (Cozy, as on the handhelds also for a game that was frozen) from where its case
/// stood on the page (`Ui::front_at`).
pub fn scene(game: &Game, ((x, y, w, h), mut front): ((i32, i32, i32, i32), Pic), page: Page, cover: Option<&Image>, printed: bool, text: &mut Text) -> Launch {
    let (sx, sy) = page.scale;
    let from = ((x as f32 * sx) as i32, (y as f32 * sy) as i32, (w as f32 * sx) as i32, (h as f32 * sy) as i32);
    if (from.2, from.3) != (front.w, front.h) {
        front = front.scaled_smooth(from.2, from.3);
    }
    let mut launch = Launch::new(game, front, from, page.pixels, page.size, page.window, cover, printed, true, text);
    launch.marks = page.marks.into_iter().map(|m| m.scaled(page.scale)).collect();
    launch
}

/// The way back of `game` with the player's note, its scene from the page at once (the tests; the app makes the scene
/// after the first frame).
#[allow(clippy::too_many_arguments)]
pub fn begin(game: &Game, case: ((i32, i32, i32, i32), Pic), page: Page, note: &LastPicture, cover: Option<&Image>, printed: bool, text: &mut Text, at: Instant) -> Eject {
    let frame = note.picture.as_deref().and_then(pocket_ui::image::load_png);
    let area = note.area_in(page.window, page.offset);
    let (size, window) = (page.size, page.window);
    let mut ej = Eject::new(game, frame, area, size, window, at);
    ej.attach(scene(game, case, page, cover, printed, text));
    ej
}

impl Eject {
    /// The way back of `game` from its last picture `frame` (None: from black), which stood at `area` on a canvas `size`
    /// big in a window `window` big (None: all of the canvas); its scene comes later (`attach`).
    pub fn new(game: &Game, frame: Option<Image>, area: Option<(i32, i32, i32, i32)>, size: (u32, u32), window: (u32, u32), at: Instant) -> Eject {
        let way = way(game, frame.is_some());
        let area = area.unwrap_or((0, 0, size.0 as i32, size.1 as i32));
        let panel = if pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::HandheldOn) { pocket_ui::lcd_on::Panel::of(insert::rig_system(game)) } else { pocket_ui::lcd_on::Panel::Plain };
        Eject { at, way, frame, area, scene: None, medium: media::Medium::of(insert::rig_system(game)), panel, size, window, key: game.key.clone() }
    }

    /// Its scene, made from the page (`scene`).
    pub fn attach(&mut self, scene: Launch) {
        self.scene = Some(scene);
    }

    /// The scene is still to be made.
    pub fn needs_scene(&self) -> bool {
        self.scene.is_none()
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }
    pub fn window(&self) -> (u32, u32) {
        self.window
    }

    pub fn moment(&self, now: Instant) -> Moment {
        self.way.moment(now.saturating_duration_since(self.at))
    }

    pub fn done(&self, now: Instant) -> bool {
        self.moment(now) == Moment::Done
    }

    /// The way back's sounds (`eject::Way::sounds`, as on Linux: the TV or the handheld's switch going off, a lid, the game
    /// coming out, the case), queued now, each a frame's way to the glass later (as the start's).
    pub fn play_cues(&self) {
        let ctx = ui_sound::Ctx { config: Default::default(), in_game: false, pause_menu: false, asleep: false };
        for (sound, at) in self.way.sounds(self.medium) {
            ui_sound::play_after(sound, &ctx, at.saturating_sub(ui_sound::hit_at(sound)) + insert::PICTURE_LATE, 0.0);
        }
    }

    /// One frame on the canvas's pixels. Without its scene yet, the page's part is black.
    pub fn paint(&mut self, buf: &mut [u32], now: Instant) {
        let (w, h) = self.size;
        match (self.moment(now), self.scene.as_mut()) {
            (Moment::Off(p), _) => {
                let mut c = Canvas::new(buf, w, h);
                match self.way.screen {
                    Screen::Tv => eject::paint_tv_off(&mut c, self.frame.as_ref(), None, self.area, p),
                    Screen::Lcd => eject::paint_lcd_off_as(&mut c, self.frame.as_ref(), None, self.area, p, self.panel),
                }
            }
            (Moment::Shelf { black, scene }, Some(s)) => {
                s.paint_scene(buf, scene);
                eject::paint_black(&mut Canvas::new(buf, w, h), black);
            }
            (Moment::Done, Some(s)) => s.paint_scene(buf, Duration::ZERO),
            (_, None) => Canvas::new(buf, w, h).clear(0),
        }
    }

    /// The colour around the safe area now (the cutout's row, the gesture bar): black, then with the page.
    pub fn margin(&self, now: Instant) -> u32 {
        match (self.moment(now), self.scene.as_ref()) {
            (Moment::Shelf { black, scene }, Some(s)) => s.margin_at(scene, black),
            (Moment::Done, Some(s)) => s.margin_at(Duration::ZERO, 0.0),
            _ => 0,
        }
    }
}
