//! What the Shelf and the Cabinet stand on their boards after the cases: the plant at its stage, the framed "100 %" with
//! its words, and which decorations a shelf shows (`room_deco` says what was earned). Shared by the Linux app
//! (`app_room_extras`) and the Android app; pure apart from the picture cache the caller keeps.

use std::collections::HashMap;
use std::rc::Rc;

use crate::cabinet_art::Pic;
use crate::gfx::Canvas;
use crate::media::{self, Prop};
use crate::room_deco::Earned;
use crate::text::{Face, Text};

/// The picture cache of [`prop_pic`]: per (prop, scale x 100).
pub type PropPics = HashMap<(Prop, i32), Rc<Pic>>;

/// The picture of `prop` at scale `f` (screen pixels a template pixel) with what was earned: the plant at its stage;
/// the framed "100 %" with its words. Made once per prop and scale.
pub fn prop_pic(pics: &mut PropPics, text: &mut Text, prop: Prop, earned: &Earned, f: f32) -> Rc<Pic> {
    let prop = prop.grown(earned.plant);
    let key = (prop, (f * 100.0) as i32);
    if let Some(p) = pics.get(&key) {
        return p.clone();
    }
    let mut pic = if (f - media::SHELF_SCALE).abs() < 0.01 { prop.pic().clone() } else { media::rescaled(prop.pic(), f / media::SHELF_SCALE) };
    if prop == Prop::Hundred {
        hundred_words(text, &mut pic);
    }
    let pic = Rc::new(pic);
    if pics.len() > 40 {
        pics.clear();
    }
    pics.insert(key, pic.clone());
    pic
}

/// "100 %" painted on the framed paper's empty middle.
pub fn hundred_words(text: &mut Text, pic: &mut Pic) {
    let (px, py, pw, ph, png_w) = media::FRAME_PAPER;
    let k = pic.w as f32 / png_w as f32;
    let (x0, y0, w, h) = ((px as f32 * k) as i32, (py as f32 * k) as i32, (pw as f32 * k) as i32, (ph as f32 * k) as i32);
    if w > 4 && h > 4 && x0 + w <= pic.w && y0 + h <= pic.h {
        let paper = pic.px[((y0 + h / 2) * pic.w + x0 + w / 2) as usize] & 0xff_ffff;
        let mut buf = vec![paper; (w * h) as usize];
        {
            let mut c = Canvas::new(&mut buf, w as u32, h as u32);
            let _fixed = crate::type_scale::fixed();
            let words = "100 %";
            let mut size = (h as f32 * 0.8) as u32;
            while size > 7 && text.width(Face::Title, size, words) > w - 2 {
                size -= 1;
            }
            let tw = text.width(Face::Title, size, words);
            text.draw(&mut c, Face::Title, size, (w - tw) / 2, h / 2 + size as i32 * 7 / 20, 0x5a3a1e, words);
        }
        for y in 0..h {
            for x in 0..w {
                pic.px[((y0 + y) * pic.w + x0 + x) as usize] = 0xff00_0000 | buf[(y * w + x) as usize];
            }
        }
    }
}

/// The decorations a shelf of `system` (None: not a system's shelf) shows after its bookend, most important first:
/// its figurine, then the trophy and the framed "100 %" where `awards` (the Shelf look's single row; the Cabinet's
/// first shelf with room for them).
pub fn deco_for(earned: &Earned, system: Option<&str>, awards: bool) -> Vec<Prop> {
    let mut out = Vec::new();
    if let Some(f) = system.and_then(|s| earned.figure(s)) {
        out.push(Prop::Figure(f));
    }
    if awards && earned.trophy {
        out.push(Prop::Trophy);
    }
    if awards && earned.hundred {
        out.push(Prop::Hundred);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::room_deco::Figure;

    #[test]
    fn awards_go_only_where_asked_and_the_figurine_first() {
        let e = Earned { plant: 1, trophy: true, hundred: true, figures: vec!["snes".into()] };
        assert_eq!(deco_for(&e, Some("snes"), true), vec![Prop::Figure(Figure::of("snes")), Prop::Trophy, Prop::Hundred]);
        assert_eq!(deco_for(&e, Some("gba"), false), vec![]);
        assert_eq!(deco_for(&e, None, true), vec![Prop::Trophy, Prop::Hundred]);
        assert_eq!(deco_for(&Earned { plant: 2, ..Default::default() }, Some("snes"), true), vec![]);
    }
}
