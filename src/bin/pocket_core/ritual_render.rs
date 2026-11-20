//! Evidence render, only with $POCKET_RITUAL_SHOTS set (else it returns at once): the Brick's software picture over the switch-on's first 500 ms at 1024x768, as `SoftScreen::show`
//! paints it (blit, then the switch-on's quads), written as PPM frames into $POCKET_RITUAL_SHOTS.
use super::*;

#[test]
fn ritual_render_series() {
    let Some(out) = std::env::var_os("POCKET_RITUAL_SHOTS") else { return };
    let out = std::path::PathBuf::from(out);
    save_safety::child("ritual_render::ritual_render_series", |root| {
        let _ = &root;
        let out = out.clone();
        std::fs::create_dir_all(&out).unwrap();
        let mut app = save_safety::app(root);
        app.crt_on = true;
        app.crt_screen = pocket_ui::crt_on::Screen::Tv;
        // A game picture: colour bars, 256x224 (a SNES frame).
        let (fw, fh) = (256u32, 224u32);
        let bars = [0xc0c0c0u32, 0xc0c000, 0x00c0c0, 0x00c000, 0xc000c0, 0xc00000, 0x0000c0, 0x202020];
        let mut data = vec![0u8; (fw * fh * 4) as usize];
        for y in 0..fh { for x in 0..fw { let c = bars[(x * 8 / fw) as usize]; let c = if y > fh * 3 / 4 { 0x101010 + ((x * 255 / fw) * 0x010101) } else { c }; let i = ((y * fw + x) * 4) as usize; data[i..i + 4].copy_from_slice(&c.to_le_bytes()); } }
        let frame = Frame { width: fw, height: fh, pitch: fw as usize * 4, format: pocket_ui::libretro::PIXEL_XRGB8888, data };
        app.last_frame = Some(frame.clone());
        let (w, h) = (1024usize, 768usize);
        app.picture_out(DrawPath::Soft);
        let first = app.switch_on_at();
        let start = app.shown_at;
        for i in 0..31u32 {
            let t = Duration::from_millis(i as u64 * 1000 / 60);
            if let Some(s) = start { app.shown_at = Some(s - t); }
            let at = if i == 0 { first } else { { app.picture_out(DrawPath::Soft); app.switch_on_at() } };
            let mut buf = vec![0u32; w * h];
            pocket_ui::soft_screen::blit(&frame, &mut buf, w, h);
            let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
            for p in &buf { ppm.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]); }
            std::fs::write(out.join(format!("plain{i:02}.ppm")), ppm).unwrap();
            if let Some((since, screen)) = at.filter(|(since, screen)| *since < pocket_ui::lcd_on::switch_on_len(*screen)) {
                let (x, y, pw, ph) = pocket_ui::soft_screen::placement((fw, fh), (w as u32, h as u32));
                let quads = pocket_ui::lcd_on::switch_on(screen, since, (x as f32, y as f32, pw as f32, ph as f32));
                pocket_ui::soft_screen::fill_quads(&mut buf, w, h, &quads);
            }
            let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
            for p in &buf { ppm.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]); }
            std::fs::write(out.join(format!("{}{i:02}.ppm", if at.is_some() { "crt" } else { "none" })), ppm).unwrap();
        }
    });
}
