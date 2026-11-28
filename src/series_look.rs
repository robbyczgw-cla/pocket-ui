//! The shelf by series' own pictures (docs/SERIES-SHELF.md "Its cover and its spine"): a series' front is a small stack
//! of the cases of its games that have box art, and its spine carries the series' logo where ScreenScraper has one.
//! Pure pictures and choices; the App picks the games and covers (`app_cabinet`, `App::series_front`), the reader
//! thread turns the logo (`upright_logo`), and both are made once and kept.
//!
//! - **Which games** (`ranked`): the series' key games first, as `series_facts` lists them, then the rest in shelf
//!   order (release year); one title on two systems once. The cover shows the first [`COLLAGE_MAX`] of them with box
//!   art; the logo comes from the first of them ScreenScraper has one for.
//! - **The cover** (`collage`): none with box art, the series' plain front as before; one, that game's case alone;
//!   two to four, their cases leaning a little, one over the other, the first in front.
//! - **The spine** (`logo_spine`): the logo whole across the label, as large as the spine's width lets it, never
//!   stretched or cut, on a ground it reads on (`logo_ground`).

use crate::cabinet::luma;
use crate::cabinet_art::{Look, Pic};
use crate::connections::{key, Kind};
use crate::media::Medium;
use crate::text::Text;

/// Whether a series' spine carries its logo and ScreenScraper's run fetches logos (`wheel`). Off in 0.8 (Alex,
/// 27.11.2026, after Neon's look: the series' spines stay their name; the logos come in 0.8.1): no logo is fetched, none
/// drawn, logos already on the card stay unused.
pub const SERIES_LOGOS: bool = false;

/// The most cases on a series' cover.
pub const COLLAGE_MAX: usize = 4;

/// The widest a case of a stack of two or more is, a share of the cover's width: the ones behind still show.
pub const CASE_SHARE: f32 = 0.52;

/// How many of a series' ranked games are looked at for a logo, on the card and to fetch (`screenscraper::series_logo_games`).
pub const LOGO_LOOK: usize = 8;

/// The longest side a logo is kept at once read: as `media::SPINE_PHOTO_EDGE`, the largest spine needs no more.
pub const LOGO_EDGE: u32 = crate::media::SPINE_PHOTO_EDGE;

/// A series' games in the order its cover and logo take them: its key games (`key_games`, as the facts list them)
/// first, every edition of each, then the others in shelf order. `names`: the games in shelf order. A title on two systems
/// is told apart where the cover is made (`title_key`): the first edition with box art counts.
pub fn ranked(names: &[&str], key_games: &[&str]) -> Vec<usize> {
    let keys: Vec<String> = names.iter().map(|n| key(Kind::Series, n)).collect();
    let mut out: Vec<usize> = Vec::with_capacity(names.len());
    let mut taken = vec![false; names.len()];
    for k in key_games {
        let k = key(Kind::Series, k);
        for (i, g) in keys.iter().enumerate() {
            if !taken[i] && *g == k {
                taken[i] = true;
                out.push(i);
            }
        }
    }
    out.extend((0..names.len()).filter(|&i| !taken[i]));
    out
}

/// A game's title as a cover's cases tell them apart: one title on two systems is one case.
pub fn title_key(name: &str) -> String {
    key(Kind::Series, name)
}

/// The order a series' logo is looked for in (the lead's look, 27.11.2026: "Castlevania", not "Super Castlevania IV"):
/// 1. the games named as the series itself (`series`: its name and aliases; "Metroid" says the series where "Metroid
///    Zero Mission" names one game);
/// 2. games whose title starts with the series' name and has no number ("Castlevania - Dracula X");
/// 3. the rest; each tier in `ranked`'s order.
pub fn logo_order(ranked: &[usize], names: &[&str], series: &[&str]) -> Vec<usize> {
    let series: Vec<String> = series.iter().map(|s| key(Kind::Series, s)).filter(|k| !k.is_empty()).collect();
    let numbered = |k: &str| k.split(' ').any(|w| !w.is_empty() && (w.chars().all(|c| c.is_ascii_digit()) || matches!(w, "ii" | "iii" | "iv" | "v" | "vi" | "vii" | "viii" | "ix")));
    let tier = |i: usize| -> u8 {
        let k = names.get(i).map(|n| key(Kind::Series, n)).unwrap_or_default();
        if series.contains(&k) {
            0
        } else if series.iter().any(|s| k.starts_with(&format!("{s} "))) && !numbered(&k) {
            1
        } else {
            2
        }
    };
    let mut out: Vec<usize> = ranked.to_vec();
    out.sort_by_key(|&i| tier(i));
    out
}

/// The games a series' cover shows: the first [`COLLAGE_MAX`] of `ranked` that have box art (`has_cover`, asked in
/// that order and no further than needed), one per title (`title`: a title's second edition with box art is not a second
/// case; its first edition without box art leaves room for it).
pub fn cover_games(ranked: &[usize], title: impl Fn(usize) -> String, mut has_cover: impl FnMut(usize) -> bool) -> Vec<usize> {
    let mut out = Vec::new();
    let mut titles: Vec<String> = Vec::new();
    for &i in ranked {
        if out.len() == COLLAGE_MAX {
            break;
        }
        let t = title(i);
        if titles.contains(&t) || !has_cover(i) {
            continue;
        }
        titles.push(t);
        out.push(i);
    }
    out
}

/// How each case of a cover of `n` lies: its middle across the cover (a share of the width), how much higher than the
/// lowest it stands (a share of the height) and its angle in degrees. The first is in front, on the left.
fn poses(n: usize) -> &'static [(f32, f32, f32)] {
    match n {
        0 | 1 => &[(0.5, 0.0, 0.0)],
        2 => &[(0.36, 0.0, -3.5), (0.66, 0.05, 4.0)],
        3 => &[(0.31, 0.0, -4.0), (0.52, 0.06, 1.0), (0.71, 0.02, 5.0)],
        _ => &[(0.28, 0.0, -4.5), (0.43, 0.07, -1.0), (0.59, 0.03, 2.5), (0.74, 0.08, 5.5)],
    }
}

/// A series' cover `w` x `h`: the cases (`fronts`, each a game's front with its cover, in [`ranked`] order) stacked as
/// they would lie on a table, leaning a little and overlapping, the first in front, each with a soft shadow; see-through
/// round them. One case stands straight in the middle, as large as fits. Made once per series and size (the App keeps it).
pub fn collage(fronts: &[&Pic], w: i32, h: i32) -> Pic {
    let mut out = Pic::new(w.max(1), h.max(1), 0);
    let n = fronts.len().min(COLLAGE_MAX);
    if n == 0 {
        return out;
    }
    let poses = poses(n);
    // Each case as large as lets the stack keep inside the cover: its height a share of the cover's, its width at most
    // the room the fan leaves.
    let (tall, wide) = if n == 1 { (1.0, 1.0) } else { (0.80, CASE_SHARE) };
    let mut placed = Vec::with_capacity(n);
    for (k, front) in fronts.iter().take(n).enumerate() {
        let (cx, up, deg) = poses[k];
        let s = (h as f32 * tall / front.h.max(1) as f32).min(w as f32 * wide / front.w.max(1) as f32);
        let (cw, ch) = (front.w as f32 * s, front.h as f32 * s);
        // Its foot on the cover's bottom, a little higher for the ones behind; room for the turn's corners.
        let margin = if n == 1 { 0.0 } else { h as f32 * 0.04 };
        let cy = h as f32 - margin - ch / 2.0 - up * h as f32;
        let (lo, hi) = (cw / 2.0 + margin, w as f32 - cw / 2.0 - margin);
        let cx = if lo <= hi { (cx * w as f32).clamp(lo, hi) } else { w as f32 / 2.0 };
        placed.push((front, cx, cy, s, deg.to_radians()));
    }
    // The last behind, the first in front.
    for &(front, cx, cy, s, a) in placed.iter().rev() {
        // Its shadow (one case too: the series' front casts no case's shadow of its own, `App::series_stacked`).
        let s_shadow = if n == 1 { s * 0.97 } else { s };
        draw_turned(&mut out, front, (cx + h as f32 * 0.012, cy + h as f32 * 0.018 * if n == 1 { 0.5 } else { 1.0 }), s_shadow, a, Some(0x60));
        draw_turned(&mut out, front, (cx, cy), s, a, None);
    }
    out
}

/// `p` scaled by `s` and turned by `a` (radians, clockwise) round (cx, cy), laid over `out` (straight alpha, sampled
/// between its pixels). `shadow`: only its outline in black at that strength, for the case's shadow.
fn draw_turned(out: &mut Pic, p: &Pic, (cx, cy): (f32, f32), s: f32, a: f32, shadow: Option<u32>) {
    if p.w <= 0 || p.h <= 0 || s <= 0.0 {
        return;
    }
    let (sin, cos) = a.sin_cos();
    let (hw, hh) = (p.w as f32 * s / 2.0, p.h as f32 * s / 2.0);
    let ext_x = hw * cos.abs() + hh * sin.abs();
    let ext_y = hw * sin.abs() + hh * cos.abs();
    let (x0, x1) = (((cx - ext_x).floor() as i32).max(0), ((cx + ext_x).ceil() as i32).min(out.w));
    let (y0, y1) = (((cy - ext_y).floor() as i32).max(0), ((cy + ext_y).ceil() as i32).min(out.h));
    out.changed();
    for y in y0..y1 {
        for x in x0..x1 {
            // Back into the picture's pixels: turned the other way, then unscaled.
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let u = (dx * cos + dy * sin) / s + p.w as f32 / 2.0 - 0.5;
            let v = (-dx * sin + dy * cos) / s + p.h as f32 / 2.0 - 0.5;
            if u < -1.0 || v < -1.0 || u > p.w as f32 || v > p.h as f32 {
                continue;
            }
            let src = sample(p, u, v);
            let src = match shadow {
                Some(k) => ((src >> 24) * k / 255) << 24,
                None => src,
            };
            let i = (y * out.w + x) as usize;
            out.px[i] = over(out.px[i], src);
        }
    }
}

/// `p` at (u, v), between its pixels (bilinear, straight alpha weighted by coverage); see-through outside it.
fn sample(p: &Pic, u: f32, v: f32) -> u32 {
    let (x0, y0) = (u.floor() as i32, v.floor() as i32);
    let (fx, fy) = (u - x0 as f32, v - y0 as f32);
    let px = |x: i32, y: i32| if x < 0 || y < 0 || x >= p.w || y >= p.h { 0 } else { p.px[(y * p.w + x) as usize] };
    let mut acc = [0f32; 4];
    for (x, y, wt) in [(x0, y0, (1.0 - fx) * (1.0 - fy)), (x0 + 1, y0, fx * (1.0 - fy)), (x0, y0 + 1, (1.0 - fx) * fy), (x0 + 1, y0 + 1, fx * fy)] {
        let c = px(x, y);
        let a = (c >> 24) as f32 * wt;
        acc[0] += a;
        acc[1] += (c >> 16 & 0xff) as f32 * a;
        acc[2] += (c >> 8 & 0xff) as f32 * a;
        acc[3] += (c & 0xff) as f32 * a;
    }
    if acc[0] < 0.5 {
        return 0;
    }
    let ch = |v: f32| ((v / acc[0]).round() as u32).min(255);
    (acc[0].round() as u32).min(255) << 24 | ch(acc[1]) << 16 | ch(acc[2]) << 8 | ch(acc[3])
}

/// `src` over `dst`, both straight alpha.
fn over(dst: u32, src: u32) -> u32 {
    let sa = src >> 24;
    if sa == 0 {
        return dst;
    }
    if sa == 255 {
        return src;
    }
    let da = dst >> 24;
    let oa = sa + da * (255 - sa) / 255;
    if oa == 0 {
        return 0;
    }
    let ch = |s: u32| {
        let (sc, dc) = (src >> s & 0xff, dst >> s & 0xff);
        ((sc * sa + dc * da * (255 - sa) / 255) / oa).min(255)
    };
    oa << 24 | ch(16) << 16 | ch(8) << 8 | ch(0)
}

/// A logo (ScreenScraper's `wheel`, 0xAARRGGBB) as it stands on a spine: its see-through margin cut off, a wide one
/// turned a quarter clockwise (read top to bottom, as the spines' titles), box-filtered to at most `edge` long. None for
/// an empty or wholly see-through picture. Done once on the reader thread, never in a frame.
pub fn upright_logo(w: u32, h: u32, px: &[u32], edge: u32) -> Option<Pic> {
    if w == 0 || h == 0 || px.len() as u64 != w as u64 * h as u64 {
        return None;
    }
    // Its drawn part: where it is more than faintly there.
    let solid = |x: u32, y: u32| px[(y * w + x) as usize] >> 24 > 16;
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if solid(x, y) {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (cw, ch) = (x1 - x0, y1 - y0);
    let cut = |x: u32, y: u32| px[((y0 + y) * w + x0 + x) as usize];
    // Wide: a quarter clockwise, its left end on top and its top facing right.
    let (tw, th, turned): (u32, u32, Vec<u32>) = if cw > ch {
        let mut t = vec![0u32; (cw * ch) as usize];
        for y in 0..cw {
            for x in 0..ch {
                t[(y * ch + x) as usize] = cut(y, ch - 1 - x);
            }
        }
        (ch, cw, t)
    } else {
        (cw, ch, (0..ch).flat_map(|y| (0..cw).map(move |x| (x, y))).map(|(x, y)| cut(x, y)).collect())
    };
    let long = tw.max(th);
    let (ow, oh) = if long > edge { (((tw as u64 * edge as u64) / long as u64).max(1) as u32, ((th as u64 * edge as u64) / long as u64).max(1) as u32) } else { (tw, th) };
    Some(Pic::of(ow as i32, oh as i32, box_alpha(&turned, tw, th, ow, oh)))
}

/// `px` (`w` x `h`, straight alpha) box-filtered to `tw` x `th`: each pixel the coverage-weighted mean of its area, so
/// a see-through edge never darkens the colour beside it. Larger than the source, each pixel takes the nearest one.
fn box_alpha(px: &[u32], w: u32, h: u32, tw: u32, th: u32) -> Vec<u32> {
    let mut out = Vec::with_capacity((tw * th) as usize);
    for ty in 0..th {
        let (sy0, sy1) = (ty * h / th, ((ty + 1) * h / th).max(ty * h / th + 1).min(h));
        for tx in 0..tw {
            let (sx0, sx1) = (tx * w / tw, ((tx + 1) * w / tw).max(tx * w / tw + 1).min(w));
            let (mut a, mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for y in sy0..sy1 {
                for &c in &px[(y * w + sx0) as usize..(y * w + sx1) as usize] {
                    let ca = (c >> 24) as u64;
                    a += ca;
                    r += (c >> 16 & 0xff) as u64 * ca;
                    g += (c >> 8 & 0xff) as u64 * ca;
                    b += (c & 0xff) as u64 * ca;
                    n += 1;
                }
            }
            if a == 0 {
                out.push(0);
            } else {
                out.push(((a / n.max(1)) as u32) << 24 | ((r / a) as u32) << 16 | ((g / a) as u32) << 8 | (b / a) as u32);
            }
        }
    }
    out
}

/// The brightness of a logo's drawn part (coverage-weighted luma, 0-255; 0 for an empty one).
fn logo_luma(logo: &Pic) -> u32 {
    let (mut sum, mut n) = (0u64, 0u64);
    for &c in &logo.px {
        let a = (c >> 24) as u64;
        sum += luma(c & 0xffffff) as u64 * a;
        n += a;
    }
    if n == 0 { 0 } else { (sum / n) as u32 }
}

/// The colour of the label a logo stands on: a near-black, as the logos are made for (their outlines and shadows read on
/// dark), so every series' spine with a logo is of one kind on the shelf; a light one only under a dark logo. The
/// spine's own colour (`spine`) where it is that dark already.
pub fn logo_ground(logo: &Pic, spine: u32) -> u32 {
    let l = logo_luma(logo);
    if l < 60 {
        LIGHT_GROUND
    } else if luma(spine) + 90 <= l && luma(spine) < 40 {
        spine
    } else {
        DARK_GROUND
    }
}

/// A warm anthracite: dark enough for the logos, apart from a black page (Neon's look, 27.11.2026).
const DARK_GROUND: u32 = 0x2c2a29;
const LIGHT_GROUND: u32 = 0xeae6de;

/// A logo darker than this on the dark ground gets a thin light edge round it (deep blue letters, Neon's look).
const EDGE_BELOW: u32 = 110;

/// A series' spine with its logo (`logo`, from [`upright_logo`]): the painted spine of `m` at `k` times its shelf size
/// without a title, its label in [`logo_ground`], and the logo whole in the label, as large as fits with its shape kept,
/// centred (`media::spine_label_rect`). Nothing else is written on it: the logo is the name.
pub fn logo_spine(text: &mut Text, m: Medium, k: f32, look: Look, logo: &Pic, want: f32) -> Pic {
    let ground = logo_ground(logo, look.color);
    let mut p = crate::media::spine_with(text, m, k, Look { color: ground, ..look }, "", want);
    let (lx, ly, lw, lh) = crate::media::spine_label_rect(m, k);
    // A little paper round it, as little as reads: the spine's width is all the logo has.
    let pad = ((lw as f32) * 0.06).round().max(2.0) as i32;
    let (rw, rh) = (lw - 2 * pad, lh - 2 * pad);
    if rw <= 0 || rh <= 0 || logo.w <= 0 || logo.h <= 0 {
        return p;
    }
    let s = (rw as f32 / logo.w as f32).min(rh as f32 / logo.h as f32);
    let (fw, fh) = (((logo.w as f32 * s).round() as u32).max(1), ((logo.h as f32 * s).round() as u32).max(1));
    let fitted = box_alpha(&logo.px, logo.w as u32, logo.h as u32, fw, fh);
    let (ox, oy) = (lx + pad + (rw - fw as i32) / 2, ly + pad + (rh - fh as i32) / 2);
    p.changed();
    // A dark logo on the dark ground: a thin light edge round its letters first, so they stand off it.
    if ground == DARK_GROUND && logo_luma(logo) < EDGE_BELOW {
        let solid = |x: i32, y: i32| x >= 0 && y >= 0 && x < fw as i32 && y < fh as i32 && fitted[(y as u32 * fw + x as u32) as usize] >> 24 > 96;
        for y in -1..=fh as i32 {
            for x in -1..=fw as i32 {
                if solid(x, y) || !(-1..=1).any(|dy| (-1..=1).any(|dx| solid(x + dx, y + dy))) {
                    continue;
                }
                let (dx, dy) = (ox + x, oy + y);
                if dx >= 0 && dy >= 0 && dx < p.w && dy < p.h {
                    let i = (dy * p.w + dx) as usize;
                    p.px[i] = over(p.px[i], 0xa0d8d2c8);
                }
            }
        }
    }
    for y in 0..fh as i32 {
        for x in 0..fw as i32 {
            let (dx, dy) = (ox + x, oy + y);
            if dx < 0 || dy < 0 || dx >= p.w || dy >= p.h {
                continue;
            }
            let i = (dy * p.w + dx) as usize;
            p.px[i] = over(p.px[i], fitted[(y as u32 * fw + x as u32) as usize]);
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(w: i32, h: i32, c: u32) -> Pic {
        Pic::new(w, h, 0xff00_0000 | c)
    }

    #[test]
    fn key_games_come_first_then_the_shelf_order_and_one_title_on_two_systems_once() {
        let names = ["The Legend of Zelda", "Zelda II - The Adventure of Link", "The Legend of Zelda - A Link to the Past", "The Legend of Zelda - Ocarina of Time", "The Legend of Zelda - A Link to the Past"];
        let key_games = ["The Legend of Zelda: Ocarina of Time", "The Legend of Zelda: A Link to the Past", "The Legend of Zelda: Breath of the Wild"];
        assert_eq!(ranked(&names, &key_games), [3, 2, 4, 0, 1], "Ocarina, A Link to the Past (both editions), then by year");
        assert_eq!(ranked(&names, &[]), [0, 1, 2, 3, 4], "no facts: shelf order (release year)");
        assert_eq!(title_key(names[2]), title_key(names[4]), "one title on two systems: one case");
        // The logo: the game named as the series first.
        let order = ranked(&names, &key_games);
        assert_eq!(logo_order(&order, &names, &["The Legend of Zelda", "Zelda"])[0], 0);
        assert_eq!(logo_order(&order, &names, &["Metroid"]), order, "none named so: the rank's order");
        // Castlevania: a title that starts with the series' name and has no number before a numbered one or another.
        let cv = ["Super Castlevania IV", "Castlevania II - Simon's Quest", "Castlevania - Dracula X", "Castlevania - Symphony of the Night"];
        assert_eq!(logo_order(&[0, 1, 2, 3], &cv, &["Castlevania"]), [2, 3, 0, 1]);
        let cv1 = ["Super Castlevania IV", "Castlevania"];
        assert_eq!(logo_order(&[0, 1], &cv1, &["Castlevania"])[0], 1, "the first game, named as the series");
        assert_eq!(ranked(&[], &key_games), Vec::<usize>::new());
    }

    #[test]
    fn the_cover_takes_at_most_four_games_with_box_art_in_rank_order() {
        let r = [5, 0, 1, 2, 3, 4, 6];
        let own = |i: usize| i.to_string();
        assert_eq!(cover_games(&r, own, |i| i != 0), [5, 1, 2, 3]);
        assert_eq!(cover_games(&r, own, |i| i == 4), [4], "one: that one");
        assert!(cover_games(&r, own, |_| false).is_empty(), "none: the plain front");
        // 5 and 1 are one title: 1 is no second case; 0 (the same title as 5, no box art) leaves room.
        let same = |i: usize| if i == 1 || i == 0 { "5".to_string() } else { i.to_string() };
        assert_eq!(cover_games(&r, same, |i| i != 5), [0, 2, 3, 4], "the title's first edition with box art");
        // Asked no further than needed.
        let mut asked = Vec::new();
        cover_games(&r, own, |i| { asked.push(i); true });
        assert_eq!(asked, [5, 0, 1, 2]);
    }

    /// 0, 1 and 4+ covers: nothing but see-through; one case straight and whole in the middle; four stacked, the first
    /// on top, every one of them showing and nothing past the cover's edge.
    #[test]
    fn a_collage_of_none_one_and_many_cases() {
        let (w, h) = (300, 400);
        let empty = collage(&[], w, h);
        assert!(empty.px.iter().all(|&p| p >> 24 == 0), "no case: nothing drawn (the App keeps the plain front)");

        let one = case(150, 210, 0xcc2222);
        let single = collage(&[&one], w, h);
        let solid = |p: &Pic, c: u32| p.px.iter().filter(|&&px| px >> 24 == 255 && px & 0xffffff == c).count();
        // As large as fits: 400 high, 286 wide, straight (so every pixel of its rectangle is the case's own colour).
        let n = solid(&single, 0xcc2222);
        assert!(n >= 284 * 398 && n <= 288 * 400, "{n}");

        let colours = [0xcc2222, 0x22cc22, 0x2222cc, 0xcccc22, 0x22cccc];
        let cases: Vec<Pic> = colours.iter().map(|&c| case(150, 210, c)).collect();
        let refs: Vec<&Pic> = cases.iter().collect();
        let stack = collage(&refs, w, h);
        let counts: Vec<usize> = colours.iter().map(|&c| solid(&stack, c)).collect();
        assert_eq!(counts[4], 0, "the fifth is left out: {counts:?}");
        assert!(counts[..4].iter().all(|&n| n > 300), "every one of four shows: {counts:?}");
        assert!(counts[0] > counts[1] && counts[0] > counts[2] && counts[0] > counts[3], "the first is in front, whole: {counts:?}");
        // The first's middle is its own, not one behind it.
        let first_mid = stack.px[((h / 2) * w + (w as f32 * 0.30) as i32) as usize] & 0xffffff;
        assert_eq!(first_mid, 0xcc2222);
        // Leaning: a case's top-left corner is not where a straight one's would be (its rows shift).
        assert!(stack.px.iter().any(|&p| p >> 24 > 0 && p >> 24 < 255), "soft edges and shadows, not hard steps");
        // Never squeezed: the drawn part of the first keeps the case's shape (no case is stretched to the cover).
        let (mut x0, mut x1, mut y0, mut y1) = (w, 0, h, 0);
        for y in 0..h {
            for x in 0..w {
                if stack.px[(y * w + x) as usize] & 0xffffff == 0xcc2222 && stack.px[(y * w + x) as usize] >> 24 == 255 {
                    (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
                }
            }
        }
        let ratio = (x1 - x0) as f32 / (y1 - y0) as f32;
        assert!((ratio - 150.0 / 210.0).abs() < 0.12, "{ratio}");
    }

    #[test]
    fn a_wide_logo_stands_up_reading_top_to_bottom_without_its_empty_margin() {
        // 40 x 10 with a see-through margin of 5 round a 30 x 4 drawn part, its left end red.
        let (w, h) = (40u32, 14u32);
        let mut px = vec![0u32; (w * h) as usize];
        for y in 5..9 {
            for x in 5..35 {
                px[(y * w + x) as usize] = if x < 10 { 0xffff0000 } else { 0xff0000ff };
            }
        }
        let up = upright_logo(w, h, &px, LOGO_EDGE).expect("a logo");
        assert_eq!((up.w, up.h), (4, 30), "margin cut, turned");
        assert_eq!(up.px[0] & 0xffffff, 0xff0000, "its left end on top");
        assert_eq!(up.px[(29 * 4) as usize] & 0xffffff, 0x0000ff);
        // Kept small.
        let big = vec![0xff808080u32; 1000 * 300];
        let up = upright_logo(1000, 300, &big, LOGO_EDGE).unwrap();
        assert_eq!((up.w as u32, up.h as u32), (120, LOGO_EDGE));
        assert!(upright_logo(10, 10, &vec![0; 100], LOGO_EDGE).is_none(), "all see-through: no logo");
        assert!(upright_logo(0, 0, &[], LOGO_EDGE).is_none());
    }

    #[test]
    fn a_logo_reads_on_its_ground() {
        let white = Pic::new(10, 40, 0xfff0f0f0);
        let black = Pic::new(10, 40, 0xff101010);
        let colourful = Pic::new(10, 40, 0xffc83c28);
        assert_eq!(logo_ground(&white, 0x202020), 0x202020, "white on a dark spine: the spine's own colour");
        assert_eq!(logo_ground(&white, 0xe0e0e0), DARK_GROUND, "white on a light spine: a dark ground");
        assert_eq!(logo_ground(&colourful, 0x2a4a3a), DARK_GROUND, "a colourful logo on a mid spine: dark, as logos are made for");
        assert_eq!(logo_ground(&black, 0x303030), LIGHT_GROUND, "dark on a dark spine: a light ground");
        for (logo, spine) in [(&white, 0xd0d0d0u32), (&black, 0x101010), (&white, 0x777777), (&black, 0x777777), (&colourful, 0x777777)] {
            let ground = logo_ground(logo, spine);
            // Apart from its ground, or (a mid-dark logo on the dark ground) with its light edge (`logo_spine`).
            let apart = (luma(ground) as i32 - logo_luma(logo) as i32).abs() >= 70;
            assert!(apart || (ground == DARK_GROUND && logo_luma(logo) < EDGE_BELOW), "{spine:06x}: {ground:06x}");
        }
    }

    /// The logo whole on the spine: as wide as the label lets it, its shape kept (not stretched to the label's length),
    /// in the label's middle, and the spine otherwise as painted.
    #[test]
    fn a_logo_spine_shows_the_logo_whole_never_stretched() {
        let mut text = Text::builtin();
        let look = Look { color: 0x2a4a3a, band: 0x2a4a3a, plastic: 0x2e2e33, tape: false };
        let logo = Pic::new(30, 90, 0xfff5d020);
        for m in [Medium::Keep, Medium::Snes, Medium::Gba] {
            let p = logo_spine(&mut text, m, 1.0, look, &logo, 24.0);
            let plain = crate::media::spine_with(&mut text, m, 1.0, look, "", 24.0);
            assert_eq!((p.w, p.h), (plain.w, plain.h), "{m:?}: the spine's own size");
            let (lx, ly, lw, lh) = crate::media::spine_label_rect(m, 1.0);
            let yellow: Vec<(i32, i32)> = (0..p.h).flat_map(|y| (0..p.w).map(move |x| (x, y))).filter(|&(x, y)| p.px[(y * p.w + x) as usize] & 0xffffff == 0xf5d020).collect();
            assert!(!yellow.is_empty(), "{m:?}: the logo shows");
            let (x0, x1) = (yellow.iter().map(|p| p.0).min().unwrap(), yellow.iter().map(|p| p.0).max().unwrap());
            let (y0, y1) = (yellow.iter().map(|p| p.1).min().unwrap(), yellow.iter().map(|p| p.1).max().unwrap());
            assert!(x0 >= lx && x1 < lx + lw && y0 >= ly && y1 < ly + lh, "{m:?}: inside the label");
            let ratio = (y1 - y0 + 1) as f32 / (x1 - x0 + 1) as f32;
            assert!((ratio - 3.0).abs() < 0.35, "{m:?}: its shape kept, {ratio}");
        }
        // A deep blue logo on the dark ground: a light edge round it; a light one: none.
        let blue = Pic::new(30, 90, 0xff2050c0);
        assert_eq!(logo_ground(&blue, look.color), DARK_GROUND);
        let edge = |p: &Pic| p.px.iter().filter(|&&c| c & 0xffffff == over(DARK_GROUND | 0xff00_0000, 0xa0d8d2c8) & 0xffffff).count();
        let p = logo_spine(&mut text, Medium::Keep, 1.0, look, &blue, 24.0);
        assert!(edge(&p) > 20, "a light edge round it: {}", edge(&p));
        let plain = logo_spine(&mut text, Medium::Keep, 1.0, look, &logo, 24.0);
        assert_eq!(edge(&plain), 0, "a light logo needs none");
    }
}
