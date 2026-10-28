//! The picture between A and the game's first frame: where the game was left, as the switcher shows it, so starting
//! a game looks like a console waking from standby instead of a black moment. pocket-ui fades the shelf out and this
//! picture in where the game is going to draw it; pocket-core's first frame is then the same picture, and it fades its
//! frame (the screen border) in around it (`overlay::Overlay::draw_veil`).
//!
//! Only a picture of the moment the game continues from is shown: the preview of the auto-resume state (pocket-core's
//! `<game>.pstate.auto.png`, RetroArch's `<game>.state.auto.png`), else the frame pocket-ui kept when the game was
//! quit (`lastframe`, the standalone emulators, which resume from their own state written at that Quit). A manual
//! save's preview or the box art would show something else than what comes next, so a game with neither starts as
//! before, from the fading shelf.
//!
//! Reading and scaling happen on a thread (`prepare`), after the game's process has been started; painting is a copy
//! with one mix per pixel while the launch fade runs, and nothing at all once the game covers pocket-ui.

use std::path::{Path, PathBuf};

use crate::gfx::{mix, Canvas};
use crate::image::{self, Image};
use crate::video::{self, Rect, Scaling};

/// What a picture is, which says where it goes on the screen.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A raw core frame (auto-resume preview): placed the way pocket-core places the game.
    Resume(PathBuf),
    /// Cut from the screen at Quit: already the shape it was shown in, fitted to the screen.
    Kept(PathBuf),
}

/// The picture for a game whose states start with `base` (`slots::base`), with `kept` the frame `lastframe` keeps.
pub fn source_at(base: &Path, kept: &Path) -> Option<Source> {
    let with = |suffix: &str| {
        let mut p = base.as_os_str().to_owned();
        p.push(suffix);
        PathBuf::from(p)
    };
    let resume = [crate::slots::preview_path(&crate::slots::auto_path(base)), with(".state.auto.png")];
    if let Some(p) = resume.into_iter().find(|p| p.is_file()) {
        return Some(Source::Resume(p));
    }
    kept.is_file().then(|| Source::Kept(kept.to_path_buf()))
}

/// The picture for `rom` of `system`, if it has one.
pub fn source(data_dir: &Path, system: &str, rom: &Path) -> Option<Source> {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    source_at(&crate::slots::base(system, stem), &crate::lastframe::frame_path(data_dir, system, rom))
}

/// How wide a raw core frame of `w` x `h` appears: the switcher's rule (handhelds square pixels, TV systems 4:3).
pub fn display_aspect(system: &str, w: u32, h: u32) -> f32 {
    crate::switcher::display_aspect(system, w, h)
}

/// Where a picture of `w` x `h` goes on a `view_w` x `view_h` screen. `hosted` with `(scaling, turns)`: the game runs
/// in pocket-core with that Picture setting, and a resume preview lands where the game's first frame will, turned
/// `turns` quarter turns as Screen orientation turns the game (`video::turned_shape`; `w` x `h` are the frame's own).
/// Anything else (RetroArch, the standalone emulators, a kept frame) is fitted to the screen at its shape.
pub fn placement(source: &Source, system: &str, w: u32, h: u32, hosted: Option<(Scaling, u32)>, view_w: u32, view_h: u32) -> Rect {
    match (source, hosted) {
        (Source::Resume(_), Some((scaling, turns))) => {
            let (dw, dh, aspect) = video::turned_shape(w, h, display_aspect(system, w, h), 0, turns);
            video::dest_rect(scaling, view_w, view_h, dw, dh, aspect)
        }
        (Source::Resume(_), None) => video::dest_rect(Scaling::Aspect, view_w, view_h, w, h, display_aspect(system, w, h)),
        (Source::Kept(_), _) => video::dest_rect(Scaling::Aspect, view_w, view_h, w, h, 0.0),
    }
}

/// A picture ready to paint: scaled to its place on the screen.
pub struct Prepared {
    pub rom: PathBuf,
    pub x: i32,
    pub y: i32,
    pub img: Image,
}

impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Prepared({}, {}x{} at {},{})", self.rom.display(), self.img.w, self.img.h, self.x, self.y)
    }
}

/// Reads the picture of `rom` and scales it to its place (nearest neighbour, as pocket-core's default Sharp shows
/// pixels). `hosted`: the game's Picture scaling and Screen orientation's quarter turns when pocket-core plays it. None
/// when the game has no picture.
pub fn prepare(data_dir: &Path, system: &str, rom: &Path, hosted: Option<(Scaling, u32)>, view_w: u32, view_h: u32) -> Option<Prepared> {
    let src = source(data_dir, system, rom)?;
    let file = match &src {
        Source::Resume(p) | Source::Kept(p) => p,
    };
    let mut img = image::load_png(file)?;
    let r = placement(&src, system, img.w, img.h, hosted, view_w, view_h);
    // A resume preview is the core's frame; the game turned by Screen orientation draws it turned (a kept frame was
    // cut from the screen, turned already).
    let turns = hosted.map_or(0, |(_, t)| t & 3);
    if matches!(src, Source::Resume(_)) && turns != 0 {
        let (w, h, px) = video::turn_pixels(img.w, img.h, &img.px, turns);
        img = Image { w, h, px };
    }
    Some(Prepared { rom: rom.to_path_buf(), x: r.x as i32, y: r.y as i32, img: scaled(&img, r.w, r.h) })
}

/// `img` at `w` x `h`, nearest neighbour.
fn scaled(img: &Image, w: u32, h: u32) -> Image {
    let (w, h) = (w.max(1), h.max(1));
    let mut px = Vec::with_capacity((w * h) as usize);
    for ty in 0..h {
        let sy = (ty as u64 * img.h as u64 / h as u64) as u32;
        let row = &img.px[(sy * img.w) as usize..((sy + 1) * img.w) as usize];
        px.extend((0..w).map(|tx| row[(tx as u64 * img.w as u64 / w as u64) as usize]));
    }
    Image { w, h, px }
}

/// Paints the picture over what is on the canvas, `alpha` 0 (not at all) to 255 (as it is).
pub fn paint(c: &mut Canvas, p: &Prepared, alpha: u8) {
    if alpha == 0 {
        return;
    }
    let (w, h) = (p.img.w as i32, p.img.h as i32);
    let (x0, x1) = (p.x.max(0), (p.x + w).min(c.w));
    for row in p.y.max(0)..(p.y + h).min(c.h) {
        let src = &p.img.px[((row - p.y) * w) as usize..((row - p.y + 1) * w) as usize];
        let dst = &mut c.buf[(row * c.w) as usize..((row + 1) * c.w) as usize];
        for x in x0..x1 {
            let s = src[(x - p.x) as usize];
            dst[x as usize] = if alpha == 255 { s } else { mix(dst[x as usize], s, alpha as u32) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("launchpic-{name}"))
    }

    #[test]
    fn the_resume_preview_comes_first_then_the_kept_frame_and_nothing_else() {
        let d = dir("source");
        let base = d.join("uCity");
        let kept = d.join("frames/gbc/uCity.png");
        assert_eq!(source_at(&base, &kept), None, "nothing yet");
        // A manual save's preview is not what the game continues from.
        std::fs::write(d.join("uCity.pstate.1.png"), b"x").unwrap();
        assert_eq!(source_at(&base, &kept), None);
        std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
        std::fs::write(&kept, b"x").unwrap();
        assert_eq!(source_at(&base, &kept), Some(Source::Kept(kept.clone())));
        std::fs::write(d.join("uCity.state.auto.png"), b"x").unwrap();
        assert_eq!(source_at(&base, &kept), Some(Source::Resume(d.join("uCity.state.auto.png"))), "RetroArch's");
        std::fs::write(d.join("uCity.pstate.auto.png"), b"x").unwrap();
        assert_eq!(source_at(&base, &kept), Some(Source::Resume(d.join("uCity.pstate.auto.png"))), "pocket-core's first");
    }

    #[test]
    fn a_resume_preview_lands_where_pocket_core_draws_the_game() {
        let src = Source::Resume(PathBuf::from("x.png"));
        // Game Boy Color, Native: the largest whole multiple of 144 lines, 7x, centred.
        let r = placement(&src, "gbc", 160, 144, Some((Scaling::Native, 0)), 1240, 1080);
        assert_eq!(r, video::dest_rect(Scaling::Native, 1240, 1080, 160, 144, 160.0 / 144.0));
        assert_eq!((r.w, r.h), (1120, 1008));
        // SNES at 4:3 whatever its 256x224 frame says, like the switcher.
        let r = placement(&src, "snes", 256, 224, Some((Scaling::Aspect, 0)), 1240, 1080);
        assert_eq!((r.w, r.h), (1240, 930));
        // Not pocket-core: fitted to the screen.
        let r = placement(&src, "gbc", 160, 144, None, 1240, 1080);
        assert_eq!(r.h, 1080);
        let kept = Source::Kept(PathBuf::from("k.png"));
        let r = placement(&kept, "ps2", 640, 448, Some((Scaling::Native, 0)), 1240, 1080);
        assert_eq!((r.w, r.h), (1240, 868), "a kept frame keeps its own shape");
        // Screen orientation: turned as pocket-core turns the game, a Game Boy picture 6x of its 144x160 turned shape.
        let r = placement(&src, "gbc", 160, 144, Some((Scaling::Native, 1)), 1240, 1080);
        assert_eq!(r, video::dest_rect(Scaling::Native, 1240, 1080, 144, 160, 144.0 / 160.0));
        assert_eq!((r.w, r.h), (864, 960));
        assert_eq!(placement(&kept, "ps2", 640, 448, Some((Scaling::Native, 1)), 1240, 1080).w, 1240, "a kept frame is turned already");
    }

    #[test]
    fn painting_mixes_in_the_picture_and_stays_on_the_canvas() {
        let img = Image { w: 2, h: 2, px: vec![0xffffff; 4] };
        let p = Prepared { rom: PathBuf::new(), x: 3, y: -1, img: scaled(&img, 4, 3) };
        assert_eq!((p.img.w, p.img.h), (4, 3));
        let mut buf = vec![0u32; 5 * 4];
        let mut c = Canvas::new(&mut buf, 5, 4);
        paint(&mut c, &p, 255);
        // Columns 3 and 4, rows 0 and 1 (the picture starts one row above the canvas and runs off its right edge).
        let lit: Vec<usize> = buf.iter().enumerate().filter(|(_, v)| **v == 0xffffff).map(|(i, _)| i).collect();
        assert_eq!(lit, vec![3, 4, 8, 9]);
        let mut buf = vec![0u32; 5 * 4];
        let mut c = Canvas::new(&mut buf, 5, 4);
        paint(&mut c, &p, 128);
        assert_eq!(c.buf[3], mix(0, 0xffffff, 128));
        paint(&mut c, &p, 0);
        assert_eq!(c.buf[3], mix(0, 0xffffff, 128), "alpha 0 paints nothing");
    }
}
