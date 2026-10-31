//! Android player picture policy. Geometry is independent of the density-scaled menu canvas.
use pocket_ui::{core_options::{FileOption, FileValue}, gfx::Canvas, touch::Area};
use std::path::{Path, PathBuf};

pub const ROWS: usize = 4;
pub struct Settings {
    pub scaling: usize,
    pub frame: usize,
    pub effect: usize,
    game: bool,
    system_path: PathBuf,
    game_path: PathBuf,
}
impl Settings {
    pub fn load(data: &Path, system: &str, key: &str) -> Self {
        let system_path = data.join("picture/systems").join(format!("{system}.json"));
        let game_path = data.join("picture/games").join(format!("{key}.json"));
        let read = |p: &Path| pocket_ui::fsutil::read_json_or_backup::<serde_json::Value>(p);
        let per_game = read(&game_path);
        let game = per_game.is_some();
        let value = per_game.or_else(|| read(&system_path)).unwrap_or_default();
        let get = |key: &str, max: usize| value[key].as_u64().unwrap_or(0).min(max as u64) as usize;
        Self { scaling: get("scaling", 2), frame: get("frame", 2), effect: get("effect", 3), game, system_path, game_path }
    }
    pub fn options(&self) -> Vec<FileOption> {
        [("Scaling", &["Fit / aspect", "Integer", "Stretch / fullscreen"][..], self.scaling),
         ("Screen frame", &["None", "Console", "CRT TV"][..], self.frame),
         ("Picture effect", &["Sharp", "Smooth", "CRT", "Scanlines"][..], self.effect),
         ("Save settings for", &["This system", "This game"][..], usize::from(self.game))]
        .into_iter().map(|(label, values, current)| FileOption {
            key: format!("picture.{label}"), label: label.into(),
            current: values[current].into(), default: values[0].into(),
            values: values.iter().map(|s| FileValue { value: (*s).into(), label: (*s).into() }).collect(),
            ..FileOption::default()
        }).collect()
    }
    pub fn change(&mut self, row: usize, delta: i32) -> Result<(), String> {
        let (at, count) = match row {
            0 => (&mut self.scaling, 3), 1 => (&mut self.frame, 3),
            2 => (&mut self.effect, 4), 3 => { self.game = !self.game; return self.save(); },
            _ => return Ok(()),
        };
        *at = (*at as i32 + delta).rem_euclid(count) as usize;
        self.save()
    }
    fn save(&self) -> Result<(), String> {
        let path = if self.game { &self.game_path } else { &self.system_path };
        let bytes = serde_json::to_vec(&serde_json::json!({"scaling":self.scaling,"frame":self.frame,"effect":self.effect})).map_err(|e| e.to_string())?;
        pocket_ui::fsutil::write_atomic(path, &bytes).map_err(|e| e.to_string())?;
        // Selecting the system scope explicitly returns this game to the system policy.
        if !self.game && self.game_path.exists() {
            std::fs::remove_file(&self.game_path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn rect(&self, size: (i32, i32), physical: (i32, i32), native: (u32, u32), reported: f64, touch: bool) -> Area {
        let (w, h) = size;
        let portrait = w < h;
        let room_h = if portrait && touch { (h - 510).max(h / 2) } else { h };
        let margin = if self.frame == 0 { 0 } else { 24 };
        let (rw, rh) = ((w - margin * 2).max(1), (room_h - margin * 2).max(1));
        let aspect = if reported.is_finite() && (0.05..20.0).contains(&reported) { reported } else { native.0 as f64 / native.1.max(1) as f64 };
        let (iw, ih) = if self.scaling == 2 { (rw, rh) } else {
            let mut ih = (rh as f64).min(rw as f64 / aspect);
            if self.scaling == 1 {
                // Quantize on physical scanlines, not density-independent menu pixels. Retain
                // display aspect for non-square-pixel cores; small panels fall back to Fit.
                let py = physical.1 as f64 / h.max(1) as f64;
                let step = native.1.max(1) as f64;
                let multiple = (ih * py / step).floor();
                if multiple >= 1.0 { ih = multiple * step / py; }
            }
            ((ih * aspect).round().max(1.0) as i32, ih.round().max(1.0) as i32)
        };
        Area::new((w - iw) / 2, if portrait { margin } else { (room_h - ih) / 2 }, iw, ih)
    }
    pub fn paint_frame(&self, c: &mut Canvas, rect: Area, system: &str) {
        if self.frame == 0 { return; }
        let tint = match system { "gba" => 0x63528a, "snes" => 0x726684, "psp" => 0x35353d, "n64" => 0x494d54, "gb" | "gbc" => 0x65745c, _ => 0x5f626a };
        let (x,y,w,h) = (rect.x,rect.y,rect.w,rect.h);
        let outer = if self.frame == 2 { 0x725238 } else { tint };
        c.round_rect(x-24,y-24,w+48,h+48,18,outer);
        c.round_rect(x-13,y-13,w+26,h+26,9,0x343438);
        c.rect(x-4,y-4,w+8,h+8,0x101014);
        c.rect(x,y,w,h,0);
        c.rect(x+w-20,y+h+15,6,3,0x72ac78);
    }
}
