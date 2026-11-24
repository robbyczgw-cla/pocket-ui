//! Over the game without a GPU: the FPS line, the badge, the toast or note and the volume/brightness bar that
//! `overlay::Overlay::draw` draws with GL, put by the CPU into the picture `soft_screen` copies (dArkOS's pocket-fb runs
//! without `--gl`; a Brick whose GPU probe failed). The pills are the same pixels (`overlay::piece_raster`)
//! at the same places (`overlay::piece_at`), drawn at the overlay's opacity.
//!
//! Cost: nothing while nothing is shown (four empty strings). A piece is rasterised only when its text or the screen
//! width changes (the FPS line twice a second, a toast once); every frame then only blends the pieces' own rectangles
//! into the picture, which the next frame's copy overwrites anyway (no restore, no full-screen pass).
//!
//! The level bar is 680 pixels wide; on a panel narrower than that plus its margins (640x480, 720x480) it is drawn at the
//! size pocket-ui's shelf shows it there: painted full size and reduced 2:1 (`scaled_layout`'s 1280x960 canvas).

use crate::overlay;
#[cfg(test)]
use crate::osd;
use crate::text::Text;

/// A piece ready to blend: straight colour in the low 24 bits, coverage (0 = nothing) in the top 8.
struct Piece {
    key: String,
    width: u32,
    w: i32,
    h: i32,
    px: Vec<u32>,
}

/// The HUD's CPU side; one per pocket-core window.
pub struct SoftHud {
    text: Text,
    pieces: [Option<Piece>; 4],
    /// Rasterisations so far (tests: a steady text is painted once).
    pub painted: u32,
}

impl Default for SoftHud {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftHud {
    pub fn new() -> Self {
        SoftHud { text: Text::new(), pieces: [None, None, None, None], painted: 0 }
    }

    /// Blends `texts` (`Hud::texts`: FPS, badge, toast, level bar wire text) into `dst`, 0RGB `dw` by `dh`. Returns the
    /// rectangles drawn (x, y, w, h), clipped to the screen; empty when nothing is shown.
    pub fn paint(&mut self, texts: &[String; 4], dst: &mut [u32], dw: usize, dh: usize) -> Vec<(i32, i32, i32, i32)> {
        let mut drawn = Vec::new();
        if texts.iter().all(String::is_empty) || dw == 0 || dh == 0 || dst.len() < dw * dh {
            return drawn;
        }
        let alpha = (overlay::ALPHA * 256.0) as u32;
        for (i, t) in texts.iter().enumerate() {
            if t.is_empty() {
                continue;
            }
            let Some(p) = self.piece(i, t, dw as u32) else { continue };
            let (x, y) = overlay::piece_at(i, (p.w, p.h), (dw as u32, dh as u32));
            if let Some(r) = blend(dst, dw, dh, p, x, y, alpha) {
                drawn.push(r);
            }
        }
        drawn
    }

    fn piece(&mut self, i: usize, t: &str, view_w: u32) -> Option<&Piece> {
        if self.pieces[i].as_ref().is_none_or(|p| p.key != t || p.width != view_w) {
            let (w, h, px) = overlay::piece_raster(&mut self.text, i, t, view_w)?;
            self.painted += 1;
            self.pieces[i] = Some(Piece { key: t.to_string(), width: view_w, w, h, px });
        }
        self.pieces[i].as_ref()
    }
}

/// The piece over `dst` at (x, y), clipped; `alpha` is the overall opacity in 1/256.
fn blend(dst: &mut [u32], dw: usize, dh: usize, p: &Piece, x: i32, y: i32, alpha: u32) -> Option<(i32, i32, i32, i32)> {
    let (x0, y0) = (x.max(0), y.max(0));
    let (x1, y1) = ((x + p.w).min(dw as i32), (y + p.h).min(dh as i32));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    for sy in y0..y1 {
        let srow = ((sy - y) * p.w) as usize;
        let drow = sy as usize * dw;
        for sx in x0..x1 {
            let s = p.px[srow + (sx - x) as usize];
            let cov = s >> 24;
            if cov == 0 {
                continue;
            }
            let a = cov * alpha / 255;
            let d = &mut dst[drow + sx as usize];
            let mix = |sh: u32| ((s >> sh & 0xff) * a + (*d >> sh & 0xff) * (256 - a)) >> 8;
            *d = mix(16) << 16 | mix(8) << 8 | mix(0);
        }
    }
    Some((x0, y0, x1 - x0, y1 - y0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(fps: &str, badge: &str, toast: &str, bar: &str) -> [String; 4] {
        [fps.into(), badge.into(), toast.into(), bar.into()]
    }

    fn bar() -> String {
        osd::Data::percent("Volume", 40).to_wire()
    }

    #[test]
    fn nothing_shown_touches_no_pixel() {
        let mut hud = SoftHud::new();
        let mut buf = vec![0x123456; 640 * 480];
        assert!(hud.paint(&texts("", "", "", ""), &mut buf, 640, 480).is_empty());
        assert!(buf.iter().all(|&p| p == 0x123456));
        assert_eq!(hud.painted, 0);
    }

    #[test]
    fn the_volume_bar_sits_at_the_top_centre_of_a_640_screen_and_fits_on_it() {
        let mut hud = SoftHud::new();
        let mut buf = vec![0x00ff00; 640 * 480];
        let drawn = hud.paint(&texts("", "", "", &bar()), &mut buf, 640, 480);
        assert_eq!(drawn.len(), 1);
        let (x, y, w, h) = drawn[0];
        assert_eq!((w, h), (osd::W / 2, osd::H / 2), "reduced 2:1 as pocket-ui's shelf shows it on 640x480");
        assert_eq!(x, (640 - w) / 2, "centred");
        assert!(y + h < 480 / 4, "kept away from the game's centre: {y}+{h}");
        // The bar really darkened the picture there, and nothing outside its rectangle changed.
        let changed: Vec<usize> = (0..buf.len()).filter(|&i| buf[i] != 0x00ff00).collect();
        assert!(changed.len() > (w * h) as usize / 2, "{} of {}", changed.len(), w * h);
        for i in changed {
            let (px, py) = ((i % 640) as i32, (i / 640) as i32);
            assert!(px >= x && px < x + w && py >= y && py < y + h, "pixel {px},{py} outside the bar");
        }
    }

    #[test]
    fn the_brick_screen_gets_the_full_size_bar_where_the_gpu_draws_it() {
        let mut hud = SoftHud::new();
        let mut buf = vec![0; 1024 * 768];
        let drawn = hud.paint(&texts("", "", "", &bar()), &mut buf, 1024, 768);
        assert_eq!(drawn, vec![((1024 - osd::W) / 2, osd::TOP, osd::W, osd::H)]);
    }

    #[test]
    fn the_bar_follows_the_level() {
        let lit = |pct: u8| {
            let mut hud = SoftHud::new();
            let mut buf = vec![0; 640 * 480];
            hud.paint(&texts("", "", "", &osd::Data::percent("Volume", pct).to_wire()), &mut buf, 640, 480);
            buf.iter().filter(|&&p| (p >> 8 & 0xff) > 0x60).count()
        };
        assert!(lit(90) > lit(20), "a louder level fills more of the bar");
    }

    #[test]
    fn toast_fps_and_badge_stay_on_a_small_screen_in_their_corners() {
        let mut hud = SoftHud::new();
        let mut buf = vec![0; 640 * 480];
        let drawn = hud.paint(&texts("60 FPS", "2x", "Battery low: 5%", ""), &mut buf, 640, 480);
        assert_eq!(drawn.len(), 3);
        let (fps, badge, toast) = (drawn[0], drawn[1], drawn[2]);
        assert_eq!((fps.0, fps.1), (overlay::MARGIN, overlay::MARGIN));
        assert_eq!(badge.0 + badge.2, 640 - overlay::MARGIN);
        assert_eq!(toast.1 + toast.3, 480 - overlay::MARGIN, "the toast stands on the bottom margin");
        for (x, y, w, h) in drawn {
            assert!(x >= 0 && y >= 0 && x + w <= 640 && y + h <= 480);
        }
    }

    #[test]
    fn the_cpu_copy_draws_every_piece_where_the_gl_overlay_does_and_whole() {
        let mut text = Text::new();
        for (vw, vh) in [(640, 480), (720, 480), (720, 720), (960, 544), (1024, 768), (1280, 720)] {
            let mut hud = SoftHud::new();
            let mut buf = vec![0; (vw * vh) as usize];
            let all = texts("60 FPS   CPU 23%", "2x", crate::first_game::NOTE, &bar());
            let drawn = hud.paint(&all, &mut buf, vw as usize, vh as usize);
            for (i, r) in drawn.into_iter().enumerate() {
                let (w, h, _) = overlay::piece_raster(&mut text, i, &all[i], vw).unwrap();
                let (x, y) = overlay::piece_at(i, (w, h), (vw, vh));
                assert_eq!(r, (x, y, w, h), "{vw}x{vh} piece {i}: the GL place, not clipped");
            }
        }
    }

    #[test]
    fn a_steady_text_is_painted_once_and_a_new_one_again() {
        let mut hud = SoftHud::new();
        let mut buf = vec![0; 640 * 480];
        for _ in 0..30 {
            hud.paint(&texts("60 FPS", "", "", &bar()), &mut buf, 640, 480);
        }
        assert_eq!(hud.painted, 2, "one raster per piece for 30 frames");
        hud.paint(&texts("59 FPS", "", "", &bar()), &mut buf, 640, 480);
        assert_eq!(hud.painted, 3);
    }

    #[test]
    fn a_long_german_note_wraps_inside_a_640_screen() {
        let mut hud = SoftHud::new();
        let mut buf = vec![0; 640 * 480];
        let msg = "Die Spielstände kommen nicht auf die Karte. Spiel speichern und beenden, dann die Karte prüfen.";
        let drawn = hud.paint(&texts("", "", msg, ""), &mut buf, 640, 480);
        let (x, _, w, _) = drawn[0];
        assert!(x >= overlay::MARGIN && x + w <= 640 - overlay::MARGIN, "{x}+{w}");
    }

    /// The Brick without its GPU (and dArkOS's pocket-fb) puts pocket-core's notes on the picture by the CPU: a Korean
    /// note there is hangul from the shipped fonts in every style, never the Latin letters a font without hangul made of it.
    #[test]
    fn a_korean_note_is_hangul_on_the_cpu_path_in_every_style() {
        for (style, letters) in crate::font_style::ROW {
            let _style = crate::font_style::for_this_thread(style, letters);
            let paint = |msg: &str| {
                let mut hud = SoftHud::new();
                let mut buf = vec![0; 1024 * 768];
                hud.paint(&texts("", "", msg, ""), &mut buf, 1024, 768);
                buf
            };
            let (korean, latin) = (paint("저장했습니다. 게임을 계속하세요"), paint("Jeojanghaetseumnida. Geimeul gyesokhaseyo"));
            assert!(korean != latin, "{style:?} {letters:?}: hangul, not its Latin letters");
            assert!(korean.iter().filter(|&&p| p != 0).count() > 2000, "{style:?}: the note has ink");
        }
    }

    #[test]
    fn junk_bar_text_draws_nothing() {
        let mut hud = SoftHud::new();
        let mut buf = vec![7; 640 * 480];
        assert!(hud.paint(&texts("", "", "", "nonsense"), &mut buf, 640, 480).is_empty());
        assert!(buf.iter().all(|&p| p == 7));
    }
}
