//! Part of the pocket-ui binary: the pause menu's Save and Load slots (`pocket_ui::save_media`). Under the picture a
//! slim row of eight small blocks in the game's save medium's plastic (`save_media::paint_row`); with "Memory card in
//! the pause menu" on ([`pause_memory_card`]) the medium itself beside its blocks, as on the info page.
//!
//! It takes the place of the dots under the picture and changes nothing else: the same slots, the same ring on the
//! slot the row is on (none on Auto and Before load), the same note under it. A slot with a save shows the game's
//! cover, which the picture loader fits on its thread (`App::picture`; until it is there, the console's picture).

use pocket_ui::control;
use pocket_ui::gfx::Canvas;
use pocket_ui::image::Image;
use pocket_ui::save_media::{self, Icon, Kind, Layout, Slots};

use super::App;

/// How many slots the strip is laid out for: the eight of every emulator with slots, so the window keeps its size.
const LAID_OUT_FOR: usize = 8;

/// The slim row's blocks and the room between them (screen pixels at the menu's 1240 px).
const ROW_BLOCK: i32 = 30;
const ROW_GAP: i32 = 12;

/// The snapshots' "slot-…:<system>:card": the full medium in the pause menu whatever the setting says.
static SNAPSHOT_CARD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// "Memory card in the pause menu" (off by default): the full medium beside its blocks instead of the slim row.
/// Only "slot-…:<system>:card" snapshots request the full medium here; players use the slim row.
pub(crate) fn pause_memory_card() -> bool {
    SNAPSHOT_CARD.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether this menu's Save is the one that can be shown: pocket-core's own slots (not RetroArch's one save, not a
/// standalone emulator, whose game must run to save).
pub(crate) fn fills(menu: &super::QuickMenu) -> bool {
    menu.hosted && !menu.retroarch && menu.states.is_some() && menu.saves == pocket_ui::pause_saves::Saves::Slots && menu.filled.len() > 1 && !pocket_ui::standalone::active()
}

impl App {
    /// Animations on Cozy and Motion on Full: the only way Save is shown (no setting of its own).
    pub(crate) fn save_fill_on(&self) -> bool {
        self.cozy() && pocket_ui::motion::level() == pocket_ui::motion::Motion::Full
    }

    /// A is pressed on Save: the state is written now (the command goes to pocket-core at the press, as it did before),
    /// the saved picture flies from the preview into its slot, a quiet click comes when it lands,
    /// and the game goes on after `save_fill::LEN` at the latest, or at once with any button (`skip_save_fill`).
    pub(crate) fn begin_save_fill(&mut self) {
        let now = pocket_ui::frame_clock::now();
        let Some(menu) = self.menu.as_mut() else { return };
        if menu.saving.is_some() {
            return;
        }
        let slot = menu.slot;
        // Not written: nothing is shown as saved and the menu stays, as the error sound says.
        // (POCKETUI_MOCK, the snapshots and the tests have no pocket-core to send to.)
        if !pocket_ui::proc::mock() && !control::pocket_core(&format!("save {}", slot + 1)) {
            pocket_ui::ui_sound::refuse();
            return;
        }
        // The preview shows the new picture, which flies into its slot; the slot holds a save from now on.
        menu.saved = Some((menu.frame.as_ref().map(|f| Image { w: f.w, h: f.h, px: f.px.clone() }), std::time::SystemTime::now()));
        if let Some(f) = menu.filled.get_mut(slot) {
            *f = true;
        }
        menu.saving = Some(pocket_ui::save_fill::Saving::new(slot, now));
        // The press's own sound is the confirm; the click comes with the full block.
        pocket_ui::ui_sound::played();
        self.play_sound(pocket_ui::ui_sound::Sound::Confirm);
        let cue = pocket_ui::save_fill::cue();
        self.play_sound_after(cue.sound, cue.after, cue.db);
        self.request_redraw();
    }

    /// Whether the picture of a Save is playing (the frames keep coming).
    pub(crate) fn save_filling(&self) -> bool {
        self.menu.as_ref().is_some_and(|m| m.saving.is_some())
    }

    /// Any button or touch while it plays: it ends, the click that was still waiting is dropped, the game comes back.
    /// True when there was one (the press is then used up).
    pub(crate) fn skip_save_fill(&mut self) -> bool {
        if !self.save_filling() {
            return false;
        }
        pocket_ui::ui_sound::forget_waiting();
        self.end_save_fill();
        true
    }

    /// Before the loop waits: the picture has run its length, the game goes on.
    pub(crate) fn save_fill_tick(&mut self) {
        let now = pocket_ui::frame_clock::now();
        if self.menu.as_ref().and_then(|m| m.saving.as_ref()).is_some_and(|s| s.moment(now).is_none()) {
            self.end_save_fill();
        }
    }

    /// The game goes on: the save was written at the press, so this is only the way back (`resume_game`).
    fn end_save_fill(&mut self) {
        if let Some(menu) = self.menu.as_mut() {
            menu.saving = None;
        }
        self.resume_game(None);
        self.request_redraw();
    }

    /// Over the preview and the slot's row while Save plays (`save_fill::paint`): the saved picture flying from the preview
    /// into its slot, then the slot lit. `preview`: where the menu painted the picture (None: there was none, nothing flies);
    /// `block`: where the row or the card puts that slot's block.
    fn paint_save_fill_over(&mut self, c: &mut Canvas, preview: Option<(i32, i32, i32, i32)>, block: impl FnOnce(usize) -> (i32, i32, i32)) {
        let Some(menu) = self.menu.as_ref() else { return };
        let Some(saving) = menu.saving else { return };
        let Some(m) = saving.moment(pocket_ui::frame_clock::now()) else { return };
        // The picture that was saved; without one (the game gave no picture) the system's colour, so a slot is still seen to fill.
        let stripe = self.running.as_ref().map_or(0x444444, |r| super::stripe(&r.system));
        let fallback;
        let block = block(saving.slot);
        let (picture, preview) = match (menu.saved.as_ref().and_then(|(img, _)| img.as_ref()), preview) {
            (Some(img), Some(p)) => (img, p),
            (img, _) => {
                fallback = pocket_ui::image::Image { w: 2, h: 2, px: vec![stripe; 4] };
                // Nothing in the preview to fly out of: it lands from the middle of the row, small.
                (img.unwrap_or(&fallback), (block.0, block.1, block.2, block.2))
            }
        };
        pocket_ui::save_fill::paint(c, picture, preview, block, m);
    }

    /// How much height the slots take under the picture: the slim row as much as the old dots did (28), the full
    /// medium its own.
    pub(crate) fn save_strip_h(&self, w: i32) -> i32 {
        if pause_memory_card() {
            self.save_strip(w).1.h
        } else {
            28
        }
    }

    /// The slim row centred on (cx, cy): `filled` one per slot, `ring` the slot the row is on.
    pub(crate) fn paint_save_row(&mut self, c: &mut Canvas, cx: i32, cy: i32, filled: &[bool], ring: Option<usize>, preview: Option<(i32, i32, i32, i32)>) {
        let Some(run) = self.running.as_ref() else { return };
        let (system, path) = (run.system.clone(), run.path.clone());
        let kind = Kind::of(&system);
        let inner = ROW_BLOCK - 2 * (ROW_BLOCK / 12).max(2);
        let cover = self.picture(&pocket_ui::covers::cover_path(&self.data_dir, &system, &path), inner, inner);
        let console = self
            .art
            .entry((system.clone(), inner, inner))
            .or_insert_with(|| pocket_ui::system_art::get(&system).map(|a| std::rc::Rc::new(a.fitted(inner, inner))))
            .clone();
        let icon = match (&cover, &console) {
            (Some(img), _) => Icon::Cover(img),
            (None, Some(sp)) => Icon::Console(sp),
            _ => Icon::None,
        };
        save_media::paint_row(c, kind, cx, cy, ROW_BLOCK, ROW_GAP, filled, ring, icon);
        let n = filled.len();
        self.paint_save_fill_over(c, preview, |slot| pocket_ui::save_fill::row_block(cx, cy, ROW_BLOCK, ROW_GAP, n, slot));
    }

    /// The running game's save medium and its strip `w` wide.
    pub(crate) fn save_strip(&self, w: i32) -> (Kind, Layout) {
        let kind = self.running.as_ref().map_or(Kind::List, |r| Kind::of(&r.system));
        (kind, Layout::new(kind, LAID_OUT_FOR, w))
    }

    /// Paints the medium with its slots in the middle of a room `w` wide from (x, y): `filled` one per slot, `ring`
    /// the slot the row is on.
    pub(crate) fn paint_save_strip(&mut self, c: &mut Canvas, x: i32, y: i32, w: i32, filled: &[bool], ring: Option<usize>, preview: Option<(i32, i32, i32, i32)>) {
        let Some(run) = self.running.as_ref() else { return };
        let (system, path) = (run.system.clone(), run.path.clone());
        let (kind, _) = self.save_strip(w);
        let l = Layout::new(kind, filled.len(), w);
        // The cover fitted to a cartridge's sticker or a slot, from the loader thread; the console's picture to a slot.
        let side = match kind {
            Kind::Cartridge(..) => l.h,
            _ => l.block,
        };
        let cover = self.picture(&pocket_ui::covers::cover_path(&self.data_dir, &system, &path), side, side);
        let console = self
            .art
            .entry((system.clone(), l.block, l.block))
            .or_insert_with(|| pocket_ui::system_art::get(&system).map(|a| std::rc::Rc::new(a.fitted(l.block, l.block))))
            .clone();
        let icon = match (&cover, &console) {
            (Some(img), _) => Icon::Cover(img),
            (None, Some(sp)) => Icon::Console(sp),
            _ => Icon::None,
        };
        let slots = Slots { filled, ring, icon, label: None, stripe: super::stripe(&system), numbers: true };
        save_media::paint(c, &mut self.text, kind, x + (w - l.w) / 2, y, &l, &slots);
        self.paint_save_fill_over(c, preview, |slot| {
            let (bx, by) = l.slot(slot);
            (x + (w - l.w) / 2 + bx, y + by, l.block)
        });
    }
}

impl App {
    /// Snapshots: "slot-save:<system>" and the other slot pages run a made-up game of that system (`main.rs`
    /// `snapshot`); `cover`: with a made-up cover in the data folder, written as a downloaded one would be; `card`:
    /// the full medium ("…:card", as with Memory card in the pause menu on).
    pub(crate) fn save_media_snapshot(&mut self, system: &str, cover: bool, card: bool) {
        SNAPSHOT_CARD.store(card, std::sync::atomic::Ordering::Relaxed);
        let (name, path) = match system {
            "psx" => ("Vagrant Story", "/storage/roms/psx/Vagrant Story (USA).chd"),
            "ps2" => ("Okage: Shadow King", "/storage/roms/ps2/Okage - Shadow King (USA).chd"),
            "gamecube" => ("Pikmin", "/storage/roms/gamecube/Pikmin (USA).rvz"),
            "dreamcast" => ("Jet Set Radio", "/storage/roms/dreamcast/Jet Set Radio (USA).chd"),
            "xbox" => ("Jade Empire", "/storage/roms/xbox/Jade Empire (USA, Europe).iso"),
            "psp" => ("Patapon", "/storage/roms/psp/Patapon (USA).cso"),
            "snes" => ("Super Metroid", "/storage/roms/snes/Super Metroid (Japan, USA).sfc"),
            "gba" => ("Advance Wars", "/storage/roms/gba/Advance Wars (USA).gba"),
            "megadrive" => ("Streets of Rage 2", "/storage/roms/megadrive/Streets of Rage 2 (USA).md"),
            "nds" => ("Professor Layton", "/storage/roms/nds/Professor Layton and the Curious Village (USA).nds"),
            _ => ("uCity", "/storage/roms/gbc/uCity.gbc"),
        };
        let path = std::path::PathBuf::from(path);
        if let Some(run) = self.running.as_mut() {
            (run.system, run.name, run.path) = (system.to_string(), name.to_string(), path.clone());
        }
        if !cover {
            return;
        }
        // A made-up cover: the system's stripe, a light disc and a dark band, 2:3 like box art.
        let (w, h) = (120u32, 168u32);
        let stripe = super::stripe(system);
        let mut px = Vec::with_capacity((w * h) as usize);
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let (dx, dy) = (x - 60, y - 70);
                px.push(match () {
                    _ if y > 132 => 0x1a1714,
                    _ if dx * dx + dy * dy < 38 * 38 => pocket_ui::gfx::mix(stripe, 0xf2eee6, 170),
                    _ => pocket_ui::gfx::mix(stripe, 0x000000, (y as u32 * 90 / h).min(255)),
                });
            }
        }
        let out = pocket_ui::covers::cover_path(&self.data_dir, system, &path);
        let _ = std::fs::create_dir_all(out.parent().unwrap_or(&self.data_dir));
        let rgb: Vec<u8> = px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
        if let Ok(file) = std::fs::File::create(&out) {
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let _ = enc.write_header().and_then(|mut wr| wr.write_image_data(&rgb));
        }
    }
}

impl App {
    /// ":saves" on a card: its save slots with 1, 3 and 6 filled, the newest two days old; ":saves=none" all empty
    /// (the snapshot has no `/storage`, so a card finds none by itself); ":continue" the "Continue" line under them (":noslots": none), from a made-up read of the shelf; ":end" with it: a box's back at its end.
    pub(crate) fn card_saves_snapshot(&mut self, state: &str) {
        // ":continue": a moment waiting, written two hours ago (the card's "Continue" line, `frozen::moment_time`).
        // The shelf's read stands in as a made-up result (the snapshot has no moment on a card); the card's own lookup
        // (`frozen_moment_time`) does the rest, as when it opens.
        if let Some(path) = self.card.as_ref().filter(|_| state.split(':').any(|p| p == "continue")).map(|o| o.path.clone()) {
            self.moment_reads.shelf_got(vec![(path, Some(pocket_ui::recents::now_secs() - 2 * 3600 - 60))]);
        }
        // ":noslots": a game without save slots (RetroArch's, a standalone emulator's).
        if let Some(open) = self.card.as_mut().filter(|_| state.split(':').any(|p| p == "noslots")) {
            open.card.data.saves = None;
        }
        let Some(case) = state.split(':').find_map(|p| p.strip_prefix("saves").map(|r| r.trim_start_matches('='))) else { return };
        let Some(open) = self.card.as_mut() else { return };
        let system = open.system.clone();
        let filled = (0..pocket_ui::slots::SLOTS).map(|i| case != "none" && [0, 2, 5].contains(&i)).collect();
        let newest = (case != "none").then(|| pocket_ui::recents::now_secs() - 2 * 86_400 - 600);
        open.card.data.saves = Some(pocket_ui::game_card::CardSaves { system: system.clone(), stripe: super::stripe(&system), filled, newest });
        // ":end": the box's back scrolled to its end, where "Your game" is (the first paint stops it at the end).
        if let Some(bx) = open.card.boxed.as_mut().filter(|_| state.split(':').any(|p| p == "end")) {
            bx.back = true;
            bx.scroll = i32::MAX / 2;
        }
    }
}

/// Snapshots: "save-card:<card>" paints that memory card alone, as tall as the screen leaves, and beside it at the
/// menu's size, on grey; "save-card:<system>"
/// of a cartridge system its cartridge (with a plain label).
pub(crate) fn save_card_snapshot(c: &mut Canvas, text: &mut pocket_ui::text::Text, name: &str) {
    use pocket_ui::save_media::Card;
    let card = match name {
        "ps1" => Some(Card::Ps1),
        "ps2" => Some(Card::Ps2),
        "gamecube" => Some(Card::GameCube),
        "vmu" => Some(Card::Vmu),
        "xbox" => Some(Card::Xbox),
        "stick" => Some(Card::MemoryStick),
        _ => None,
    };
    c.clear(0x808080);
    let u = (c.h * 3 / 4) / 32;
    let h = 32 * u;
    if let Some(card) = card {
        // Large (the picture at whole multiples of its pixels) and at the pause menu's 128 px.
        card.paint(c, 40, (c.h - h) / 2, h);
        card.paint(c, 40 + card.size(h).0 + 60, (c.h - 128) / 2, 128);
        return;
    }
    let Kind::Cartridge(game, plastic) = Kind::of(name) else { return };
    let stripe = super::stripe(name);
    let look = pocket_ui::cabinet_art::Look { color: pocket_ui::gfx::mix(stripe, 0x101010, 70), band: stripe, plastic, tape: false };
    let mut x = 40;
    for size in [h, 128] {
        if let Some(p) = pocket_ui::media::game_pic(text, game, size, look, "", None, None) {
            pocket_ui::cabinet_art::blit(c, &p, x, (c.h - size) / 2, 256, (0, c.h));
            x += p.w + 60;
        }
    }
}
