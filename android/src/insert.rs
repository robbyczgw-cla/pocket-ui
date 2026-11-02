//! Starting a game on a phone: the case goes into its console over the page it stood on (`pocket_ui::insert_scene`, the
//! handhelds' animation at the phone's own pixels), the player waits until the fade has run and comes up on black.
//! Cozy holds `PlayerActivity` back as Linux holds pocket-core's window (`insert_scene::hold`); a tap or any button skips.
use pocket_ui::{
    cabinet::FormFactor,
    cabinet_art::{self as art, Look, Pic},
    document_library::Game,
    gfx::{mix, Canvas},
    image::Image,
    insert_rig as ir,
    insert_scene::{self, HandOver, Scene, Stage},
    media::{self, Medium},
    palette::pal,
    system_art::{self, Sprite},
    text::{Face, Text},
    ui_sound,
};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

/// A frame handed to the screen reaches the glass about this much later (the compositor's vsync and the panel's), while a
/// sound's moment is AAudio's presentation time. scripts/android/insert-sync.py measures both up to that point: without
/// this the sound is 22-30 ms before the frame is handed over, and that frame is seen a vsync or two after.
const PICTURE_LATE: Duration = Duration::from_millis(30);
/// After the fade, this long on black before the screen says why (a large game still being copied, a slow folder).
const SAY_STARTING_AFTER: Duration = Duration::from_millis(1200);
/// The player never came: the shelf comes back rather than a black screen for good.
const GIVE_UP_AFTER: Duration = Duration::from_secs(20);

pub struct Launch {
    pub at: Instant,
    scene: Scene,
    stage: Stage,
    sprite: Option<Sprite>,
    /// The page the case stood on, without the case, at the screen's pixels.
    backdrop: Vec<u32>,
    pub size: (u32, u32),
    /// The window's size when it started: turned, the start's picture ends.
    pub window: (u32, u32),
    hand_over: HandOver,
    /// When the player was told to come.
    released_at: Option<Instant>,
    /// Black is on screen after the end: nothing more to paint until it says "Starting".
    pub settled: bool,
    pub trace: bool,
    /// Test only: no game comes, the page comes back at the end (`insertRehearsal`).
    pub rehearsal: bool,
}

/// The console a game goes into: Android keeps Wii and GameCube on one shelf (`document_library`), their consoles differ.
fn rig_system(game: &Game) -> &str {
    let relative = game.relative.to_ascii_lowercase();
    if game.system_id == "gamecube" && (relative.ends_with(".wbfs") || relative.ends_with(".wad") || relative.split('/').any(|d| d == "wii")) {
        "wii"
    } else {
        game.system_id
    }
}

/// A PNG's size from its header.
fn png_size(png: &[u8]) -> Option<(i32, i32)> {
    let dim = |at: usize| Some(i32::from_be_bytes(png.get(at..at + 4)?.try_into().ok()?));
    Some((dim(16)?, dim(20)?))
}

/// The sounds of the start, queued now with their waits.
pub fn play_cues(cozy: bool, game: &Game) {
    let system = rig_system(game);
    let kind = media::Game::of(system, &game.relative);
    let disc_rig = cozy && pocket_ui::playful::on() && ir::takes_disc(system, kind);
    let ctx = ui_sound::Ctx { config: Default::default(), in_game: false, pause_menu: false, asleep: false };
    for cue in insert_scene::cues(cozy, Medium::of(system), disc_rig) {
        let after = if cue.after.is_zero() { cue.after } else { cue.after + PICTURE_LATE };
        ui_sound::play_after(cue.sound, &ctx, after, cue.db);
    }
}

impl Launch {
    /// `from`: where the case stands on the page, at the screen's pixels; `backdrop`: the page without it.
    #[allow(clippy::too_many_arguments)]
    pub fn new(game: &Game, front: Pic, from: (i32, i32, i32, i32), backdrop: Vec<u32>, size: (u32, u32), window: (u32, u32), cover: Option<&Image>, printed: bool, cozy: bool, text: &mut Text) -> Launch {
        let system = rig_system(game);
        let kind = media::Game::of(system, &game.relative);
        let medium = Medium::of(system);
        let look = Look { color: game.color, band: game.color, plastic: art::plastic(system) };
        let (w, h) = (size.0 as i32, size.1 as i32);
        let rig_files = (cozy && pocket_ui::playful::on()).then(|| ir::files(system, kind)).flatten();
        let rig = rig_files.as_ref().and_then(|f| Some((f.rig, png_size(&f.base)?)));
        // A rig's own measures, or the plain console's room.
        let base = rig.map_or((300, 200), |(_, b)| b);
        let blocks = insert_scene::phone_blocks(rig.as_ref().map(|(r, _)| r), base, (w, h));
        let k = blocks as f32 / ir::SCALE;
        let sprite = system_art::get(system).or_else(|| system_art::get(insert_scene::STAND_IN)).map(|s| s.fitted((560.0 * k) as i32, (330.0 * k) as i32));
        let floor = if rig.is_some() {
            insert_scene::phone_floor(rig.as_ref().map(|(r, _)| r), base, blocks, h, k)
        } else {
            let sh = sprite.as_ref().map_or((120.0 * k) as i32, |s| s.h as i32);
            (h + sh) / 2 + (10.0 * k) as i32
        };
        let mut scene = Scene {
            system: system.to_string(),
            ff: FormFactor::of(system),
            medium,
            look,
            title: game.name.clone(),
            kind,
            has_cover: cover.is_some(),
            front: Rc::new(front),
            from,
            game: None,
            cozy,
            at_drive: None,
            rig: None,
            rig_asked: false,
        };
        scene.load_rig(blocks);
        let seated = scene.rig.as_ref().map_or(0, |r| r.rig.seated_h(r.scale));
        let console = system_art::get(system);
        let beside = kind.size_beside(medium, from.3).1;
        scene.game = if kind.is_round() {
            let sw = sprite.as_ref().map_or(360.0 * k, |s| s.w as f32);
            let disc_h = beside.max(2 * insert_scene::over_drive(kind, sw, k) as i32).max(seated);
            Some(if printed {
                media::disc_pic(text, media::DiscKind::of(system), disc_h, look, &game.name, cover)
            } else {
                media::disc_pic(text, media::DiscKind::of(system), disc_h, look, "", None)
            })
        } else {
            media::game_pic(text, kind, beside.max(seated), look, &game.name, cover, console)
        }
        .map(Rc::new);
        // No medium to show on the rig: the plain animation still has its console.
        if scene.game.is_none() {
            scene.rig = None;
        }
        Launch {
            at: Instant::now(),
            scene,
            stage: Stage { floor, k },
            sprite,
            backdrop,
            size,
            window,
            hand_over: HandOver::new(cozy),
            released_at: None,
            settled: false,
            trace: false,
            rehearsal: false,
        }
    }

    /// Time to tell the player to come (once): Cozy's hold has run, or at once the quick way.
    pub fn take_release(&mut self, now: Instant) -> bool {
        let due = self.hand_over.take_release(now.saturating_duration_since(self.at));
        if due {
            self.released_at = Some(now);
        }
        due
    }

    pub fn released(&self) -> bool {
        self.hand_over.released()
    }

    /// A tap or a button: the picture ends, its sounds still waiting aren't played, the game comes now. True the first time.
    pub fn skip(&mut self) -> bool {
        if !self.hand_over.skip() {
            return false;
        }
        self.settled = false;
        ui_sound::forget_waiting();
        true
    }

    /// Turned while it played: only black is left to paint, at the new size.
    pub fn drop_backdrop(&mut self) {
        self.backdrop = Vec::new();
    }

    pub fn gave_up(&self, now: Instant) -> bool {
        self.released_at.is_some_and(|at| now.saturating_duration_since(at) > GIVE_UP_AFTER)
    }

    /// Whether the picture is at its black end, so a frame is only needed for the words on it.
    pub fn at_end(&self, now: Instant) -> bool {
        self.hand_over.skipped() || self.scene.fade_at(now.saturating_duration_since(self.at)) >= 1.0
    }

    pub fn says_starting(&self, now: Instant) -> bool {
        self.released_at.is_some_and(|at| now.saturating_duration_since(at) > SAY_STARTING_AFTER) && self.at_end(now)
    }

    /// How long since the start, for the sync measurement's log.
    pub fn elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.at)
    }

    /// One frame on the screen's pixels: the page, the scene over it, the launch fade, black at the end.
    pub fn paint(&mut self, buf: &mut [u32], text: &mut Text, now: Instant) {
        let (w, h) = self.size;
        let mut c = Canvas::new(buf, w, h);
        let elapsed = now.saturating_duration_since(self.at);
        if self.at_end(now) {
            c.clear(0);
            if self.says_starting(now) {
                let word = crate::ui::tr("Starting");
                let size = (self.stage.k * 28.0) as u32;
                let tw = text.width(Face::Body, size, &word);
                text.draw(&mut c, Face::Body, size, (w as i32 - tw) / 2, h as i32 / 2, pal().dust, &word);
            }
            return;
        }
        c.buf.copy_from_slice(&self.backdrop);
        self.scene.paint(&mut c, elapsed, self.sprite.as_ref(), self.stage);
        c.fade((self.scene.fade_at(elapsed) * 255.0) as u8);
    }

    /// The colour around the safe area now (the cutout's row, the gesture bar): it goes dark with the page.
    pub fn margin(&self, now: Instant) -> u32 {
        let elapsed = now.saturating_duration_since(self.at);
        if self.at_end(now) {
            return 0;
        }
        let st = pocket_ui::cabinet::insert_at(elapsed, self.scene.cozy);
        let dark = 1.0 - (1.0 - st.dim) * (1.0 - self.scene.fade_at(elapsed));
        mix(0, pal().void, ((1.0 - dark) * 256.0) as u32)
    }
}
