//! Pixel-art pictures of the consoles for the system cards on the shelf (assets/systems, sources and
//! licences in assets/systems/SOURCES.md). They are compiled into the binary, so every install has them.

/// A picture with binary transparency: 0xAARRGGBB, alpha 0 or 255.
pub struct Sprite {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u32>,
}

/// The systems with a picture of their own (assets/systems/<id>.png), by ROCKNIX system name.
const ART: &[(&str, &[u8])] = &[
    ("3do", include_bytes!("../assets/systems/3do.png")),
    ("3ds", include_bytes!("../assets/systems/3ds.png")),
    ("atari2600", include_bytes!("../assets/systems/atari2600.png")),
    ("atari7800", include_bytes!("../assets/systems/atari7800.png")),
    ("atarijaguar", include_bytes!("../assets/systems/atarijaguar.png")),
    ("atarilynx", include_bytes!("../assets/systems/atarilynx.png")),
    ("colecovision", include_bytes!("../assets/systems/colecovision.png")),
    ("dreamcast", include_bytes!("../assets/systems/dreamcast.png")),
    ("fds", include_bytes!("../assets/systems/fds.png")),
    ("gamecube", include_bytes!("../assets/systems/gamecube.png")),
    ("gamegear", include_bytes!("../assets/systems/gamegear.png")),
    ("gb", include_bytes!("../assets/systems/gb.png")),
    ("gba", include_bytes!("../assets/systems/gba.png")),
    ("gbc", include_bytes!("../assets/systems/gbc.png")),
    ("intellivision", include_bytes!("../assets/systems/intellivision.png")),
    ("mastersystem", include_bytes!("../assets/systems/mastersystem.png")),
    ("megadrive", include_bytes!("../assets/systems/megadrive.png")),
    ("n64", include_bytes!("../assets/systems/n64.png")),
    ("nds", include_bytes!("../assets/systems/nds.png")),
    ("neocd", include_bytes!("../assets/systems/neocd.png")),
    ("neogeo", include_bytes!("../assets/systems/neogeo.png")),
    ("nes", include_bytes!("../assets/systems/nes.png")),
    ("ngpc", include_bytes!("../assets/systems/ngpc.png")),
    ("pcengine", include_bytes!("../assets/systems/pcengine.png")),
    ("pcenginecd", include_bytes!("../assets/systems/pcenginecd.png")),
    ("pokemini", include_bytes!("../assets/systems/pokemini.png")),
    ("pico-8", include_bytes!("../assets/systems/pico-8.png")),
    ("pc", include_bytes!("../assets/systems/pc.png")),
    ("ports", include_bytes!("../assets/systems/ports.png")),
    ("ps2", include_bytes!("../assets/systems/ps2.png")),
    ("psp", include_bytes!("../assets/systems/psp.png")),
    ("psvita", include_bytes!("../assets/systems/psvita.png")),
    ("psx", include_bytes!("../assets/systems/psx.png")),
    ("scummvm", include_bytes!("../assets/systems/scummvm.png")),
    ("sega32x", include_bytes!("../assets/systems/sega32x.png")),
    ("segacd", include_bytes!("../assets/systems/segacd.png")),
    ("sg-1000", include_bytes!("../assets/systems/sg-1000.png")),
    ("snes", include_bytes!("../assets/systems/snes.png")),
    ("supergrafx", include_bytes!("../assets/systems/supergrafx.png")),
    // Settings > Look > Systems on the shelf: the row of "PC streaming" (an app, not a system with games).
    ("streaming", include_bytes!("../assets/systems/streaming.png")),
    ("tic-80", include_bytes!("../assets/systems/tic-80.png")),
    ("vectrex", include_bytes!("../assets/systems/vectrex.png")),
    ("virtualboy", include_bytes!("../assets/systems/virtualboy.png")),
    ("wii", include_bytes!("../assets/systems/wii.png")),
    ("wonderswan", include_bytes!("../assets/systems/wonderswan.png")),
    ("xbox", include_bytes!("../assets/systems/xbox.png")),
];

/// Other ROCKNIX names for the same hardware.
const ALIASES: &[(&str, &str)] = &[
    ("famicom", "nes"),
    ("nesh", "nes"),
    ("sfc", "snes"),
    ("snesh", "snes"),
    ("gbh", "gb"),
    ("gbch", "gbc"),
    ("gbah", "gba"),
    ("genesis", "megadrive"),
    ("megadrive-japan", "megadrive"),
    ("megadriveh", "megadrive"),
    ("genh", "megadrive"),
    ("ggh", "gamegear"),
    ("megacd", "segacd"),
    ("tg16", "pcengine"),
    ("tg16cd", "pcenginecd"),
    ("ngp", "ngpc"),
    ("wonderswancolor", "wonderswan"),
    ("pico8", "pico-8"),
    ("wiiware", "wii"),
    ("n3ds", "3ds"),
];

fn slot(system: &str) -> Option<usize> {
    let id = ALIASES.iter().find(|(alias, _)| *alias == system).map_or(system, |(_, id)| *id);
    ART.iter().position(|(name, _)| *name == id)
}

/// The embedded picture for a system id, aliases included; None means the card stays plain.
pub fn png(system: &str) -> Option<&'static [u8]> {
    slot(system).map(|i| ART[i].1)
}

/// The decoded picture for a system, decoded on first use and kept.
pub fn get(system: &str) -> Option<&'static Sprite> {
    static DECODED: [std::sync::OnceLock<Option<Sprite>>; ART.len()] = [const { std::sync::OnceLock::new() }; ART.len()];
    let i = slot(system)?;
    DECODED[i].get_or_init(|| decode(ART[i].1)).as_ref()
}

/// PNG (palette with tRNS, RGB or RGBA) to a sprite; partial alpha counts as solid from half on.
pub fn decode(data: &[u8]) -> Option<Sprite> {
    let mut reader = crate::image::png_reader(std::io::Cursor::new(data))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let argb = |r: u8, g: u8, b: u8, a: u8| {
        if a < 128 {
            0
        } else {
            0xff00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
        }
    };
    let px: Vec<u32> = match info.color_type {
        png::ColorType::Rgba => data.chunks_exact(4).map(|p| argb(p[0], p[1], p[2], p[3])).collect(),
        png::ColorType::Rgb => data.chunks_exact(3).map(|p| argb(p[0], p[1], p[2], 255)).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).map(|p| argb(p[0], p[0], p[0], p[1])).collect(),
        png::ColorType::Grayscale => data.iter().map(|g| argb(*g, *g, *g, 255)).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(Sprite {
        w: info.width,
        h: info.height,
        px,
    })
}

impl Sprite {
    /// The largest copy that fits `max_w` x `max_h`: whole-number upscales so pixel art stays crisp (a
    /// nearest-neighbour fractional one when the whole number would leave it below three quarters of the
    /// box), area-averaged downscales (with transparency taken into account) when the box is smaller.
    pub fn fitted(&self, max_w: i32, max_h: i32) -> Sprite {
        if self.w == 0 || self.h == 0 || max_w <= 0 || max_h <= 0 {
            return Sprite { w: 0, h: 0, px: Vec::new() };
        }
        let scale = (max_w as f32 / self.w as f32).min(max_h as f32 / self.h as f32);
        if scale >= 1.0 {
            let k = if scale.floor() / scale >= 0.75 { scale.floor() } else { scale };
            let (w, h) = ((self.w as f32 * k) as u32, (self.h as f32 * k) as u32);
            let (sw, sh) = (self.w, self.h);
            let px = (0..h)
                .flat_map(|y| (0..w).map(move |x| ((x * sw / w).min(sw - 1), (y * sh / h).min(sh - 1))))
                .map(|(x, y)| self.px[(y * self.w + x) as usize])
                .collect();
            return Sprite { w, h, px };
        }
        let (w, h) = (((self.w as f32 * scale) as u32).max(1), ((self.h as f32 * scale) as u32).max(1));
        let mut px = Vec::with_capacity((w * h) as usize);
        for ty in 0..h {
            let (y0, y1) = (ty * self.h / h, ((ty + 1) * self.h / h).max(ty * self.h / h + 1));
            for tx in 0..w {
                let (x0, x1) = (tx * self.w / w, ((tx + 1) * self.w / w).max(tx * self.w / w + 1));
                let (mut r, mut g, mut b, mut solid, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
                for sy in y0..y1.min(self.h) {
                    for sx in x0..x1.min(self.w) {
                        let p = self.px[(sy * self.w + sx) as usize];
                        n += 1;
                        if p >> 24 != 0 {
                            r += p >> 16 & 0xff;
                            g += p >> 8 & 0xff;
                            b += p & 0xff;
                            solid += 1;
                        }
                    }
                }
                // Solid where at least half the area is; the colour only from the solid part, so no dark fringe.
                px.push(if solid * 2 >= n.max(1) {
                    0xff00_0000 | (r / solid) << 16 | (g / solid) << 8 | b / solid
                } else {
                    0
                });
            }
        }
        Sprite { w, h, px }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::Canvas;

    /// Every system the shelf shows a card for (src/main.rs `stripe`, src/hostsys.rs, pocket-core's own).
    const SHELF: &[&str] = &[
        "gb",
        "gbc",
        "gba",
        "nes",
        "snes",
        "megadrive",
        "mastersystem",
        "gamegear",
        "sg-1000",
        "pcengine",
        "segacd",
        "sega32x",
        "pcenginecd",
        "supergrafx",
        "neogeo",
        "neocd",
        "atarilynx",
        "atari7800",
        "ngpc",
        "virtualboy",
        "atarijaguar",
        "3do",
        "fds",
        "colecovision",
        "intellivision",
        "vectrex",
        "wonderswan",
        "xbox",
        "atari2600",
        "pokemini",
        "psx",
        "ps2",
        "n64",
        "psp",
        "psvita",
        "dreamcast",
        "nds",
        "gamecube",
        "wii",
        "3ds",
        "pico-8",
        "ports",
        "pc",
        "scummvm",
        "tic-80",
        "streaming",
    ];

    #[test]
    fn every_shelf_system_has_an_embedded_picture() {
        let missing: Vec<&str> = SHELF.iter().copied().filter(|s| png(s).is_none()).collect();
        assert!(missing.is_empty(), "no picture for {missing:?}");
    }

    #[test]
    fn every_picture_decodes_transparent_and_about_256_wide() {
        for s in SHELF {
            let sprite = get(s).unwrap_or_else(|| panic!("{s}: does not decode"));
            assert!((150..=256).contains(&sprite.w.max(sprite.h)), "{s}: {}x{}", sprite.w, sprite.h);
            assert_eq!(sprite.px.len(), (sprite.w * sprite.h) as usize, "{s}");
            let clear = sprite.px.iter().filter(|p| *p >> 24 == 0).count();
            let solid = sprite.px.iter().filter(|p| *p >> 24 == 0xff).count();
            assert_eq!(clear + solid, sprite.px.len(), "{s}: only fully clear or fully solid pixels");
            assert!(
                clear > 0 && solid > sprite.px.len() / 3,
                "{s}: background cut out, console kept ({clear} clear, {solid} solid)"
            );
        }
    }

    /// Luminance 0..255 of an opaque 0xAARRGGBB pixel.
    fn luma(p: u32) -> f32 {
        0.2126 * ((p >> 16) & 0xff) as f32 + 0.7152 * ((p >> 8) & 0xff) as f32 + 0.0722 * (p & 0xff) as f32
    }

    #[test]
    fn a_picture_that_is_dark_all_over_has_a_light_rim_so_it_shows_on_the_dark_spine() {
        // The spine is #1F1F22 (luminance 31); the tiles of Add games and the switcher are as dark or black. A console that
        // is black or navy all over (Mega Drive, PS2) vanished there. Its outline must stand out; lighter pictures are as drawn.
        let mut dark = Vec::new();
        for s in SHELF {
            let sprite = get(s).unwrap();
            let solid = |x: i64, y: i64| x >= 0 && y >= 0 && x < sprite.w as i64 && y < sprite.h as i64 && sprite.px[(y as usize) * sprite.w as usize + x as usize] >> 24 != 0;
            let (mut near_spine, mut total, mut edge, mut edge_lit) = (0, 0, 0, 0);
            for y in 0..sprite.h as i64 {
                for x in 0..sprite.w as i64 {
                    if !solid(x, y) {
                        continue;
                    }
                    let l = luma(sprite.px[(y as usize) * sprite.w as usize + x as usize]);
                    total += 1;
                    near_spine += (l < 53.0) as u32;
                    if !(solid(x - 1, y) && solid(x + 1, y) && solid(x, y - 1) && solid(x, y + 1)) {
                        edge += 1;
                        edge_lit += (l >= 80.0) as u32;
                    }
                }
            }
            if near_spine as f32 / total as f32 >= 0.85 {
                dark.push(*s);
                assert!(edge_lit as f32 / edge as f32 >= 0.6, "{s}: dark all over and only {edge_lit} of {edge} outline pixels show on the spine");
            }
        }
        // The rule is meant for these (the list changes when a picture is made lighter or a darker one comes).
        for s in ["megadrive", "ps2"] {
            assert!(dark.contains(&s), "{s} counts as dark all over: {dark:?}");
        }
    }

    #[test]
    fn rocknix_alias_names_share_the_picture() {
        for (alias, id) in [
            ("genesis", "megadrive"),
            ("megadrive-japan", "megadrive"),
            ("famicom", "nes"),
            ("sfc", "snes"),
            ("tg16", "pcengine"),
            ("wonderswancolor", "wonderswan"),
            ("pico8", "pico-8"),
            ("wiiware", "wii"),
            ("n3ds", "3ds"),
            ("megacd", "segacd"),
            ("tg16cd", "pcenginecd"),
            ("ngp", "ngpc"),
        ] {
            assert_eq!(png(alias).map(|b| b.as_ptr()), png(id).map(|b| b.as_ptr()), "{alias} -> {id}");
            assert!(png(alias).is_some(), "{alias}");
        }
    }

    #[test]
    fn unknown_systems_and_the_special_cards_stay_plain() {
        for s in ["favorites", "settings", "no-such-system", ""] {
            assert!(png(s).is_none() && get(s).is_none(), "{s}");
        }
    }

    #[test]
    fn fitting_scales_up_by_whole_pixels_and_down_to_the_box() {
        let s = Sprite {
            w: 100,
            h: 50,
            px: vec![0xff00_ff00; 5000],
        };
        let up = s.fitted(310, 400);
        assert_eq!((up.w, up.h), (300, 150), "3x, the largest whole factor");
        let odd = s.fitted(175, 400);
        assert_eq!((odd.w, odd.h), (175, 87), "1x would fill only 57% of the box: 1.75x instead");
        let down = s.fitted(80, 80);
        assert!(down.w <= 80 && down.h <= 80 && down.w >= 78, "{}x{}", down.w, down.h);
        assert!(down.px.iter().all(|p| *p == 0xff00_ff00));
    }

    #[test]
    fn transparent_pixels_leave_the_card_showing() {
        let s = Sprite {
            w: 2,
            h: 1,
            px: vec![0x0000_0000, 0xffff_0000],
        };
        let mut buf = vec![0x0012_3456u32; 4];
        let mut c = Canvas::new(&mut buf, 2, 2);
        c.blit_sprite(&s, 0, 0, 0);
        assert_eq!(buf[..2], [0x0012_3456, 0x00ff_0000]);
    }
}
