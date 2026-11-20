//! File boundaries of the Android handheld Activity, shared with the host rule.
//! Activity/JNI callbacks still own when these functions run; this module owns
//! their actual file reads and the picture handed to the existing eject scene.
use std::path::{Path, PathBuf};

pub fn launcher_json(data: &Path) -> String {
    std::fs::read_to_string(data.join("launcher.json")).unwrap_or_default()
}

pub fn channel_url(json: &str, built_in: &str) -> String {
    if built_in.is_empty() {
        serde_json::from_str::<serde_json::Value>(json).ok().and_then(|v| v["core_catalog_url"].as_str().map(str::to_string)).unwrap_or_default()
    } else {
        built_in.to_string()
    }
}

pub fn system_list(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// The generated list is replaced only when different, preserving its library
/// cache on an ordinary Activity return. `true` means a replacement was written.
pub fn keep_system_list(path: &Path, systems: &str) -> std::io::Result<bool> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(systems) {
        return Ok(false);
    }
    std::fs::write(path, systems)?;
    Ok(true)
}

/// The interrupted process's note is consumed even when unreadable, exactly as
/// the Activity previously did before deciding whether it still owns a session.
pub fn take_playing_note(path: &Path) -> Option<serde_json::Value> {
    let note = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok());
    let _ = std::fs::remove_file(path);
    note
}

#[derive(Debug, PartialEq)]
pub struct ResumedGame {
    pub path: PathBuf,
    pub system: String,
    pub name: String,
    pub secs: u64,
}

/// The same defaults used by the Activity when its in-memory session was lost.
pub fn resumed_game(note: &serde_json::Value, now: u64) -> ResumedGame {
    let path = PathBuf::from(note["path"].as_str().unwrap_or_default());
    let system = note["system"].as_str().unwrap_or_default().to_string();
    let secs = now.saturating_sub(note["at"].as_u64().unwrap_or(now));
    let name = note["name"].as_str().map(str::to_string).unwrap_or_else(|| crate::library::display_name(path.file_stem().and_then(|s| s.to_str()).unwrap_or_default()));
    ResumedGame { path, system, name, secs }
}

/// Restore the player's last picture into its old window. A missing/damaged
/// PNG returns None, so begin_eject takes its existing shelf-without-picture path.
pub fn return_frame(note: &crate::eject::LastPicture, window: Option<(u32, u32)>) -> Option<crate::image::Image> {
    if !note.saved { return None; }
    let picture = crate::image::load_png(note.picture.as_deref()?)?;
    let (ww, wh) = window.filter(|s| *s == note.window).unwrap_or(note.window);
    let (x, y, w, h) = note.rect.filter(|r| r.2 > 0 && r.3 > 0).unwrap_or((0, 0, ww as i32, wh as i32));
    let mut screen = crate::image::Image { w: ww, h: wh, px: vec![0; (ww * wh) as usize] };
    let scaled = crate::launch_picture::scaled(&picture, w as u32, h as u32);
    for row in 0..scaled.h as i32 {
        let sy = y + row;
        if sy < 0 || sy >= wh as i32 { continue; }
        for col in 0..scaled.w as i32 {
            let sx = x + col;
            if sx >= 0 && sx < ww as i32 {
                screen.px[(sy as u32 * ww + sx as u32) as usize] = scaled.px[(row as u32 * scaled.w + col as u32) as usize];
            }
        }
    }
    Some(screen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const NOW: u64 = 1794139290;
    const ROM: &str = "/storage/emulated/0/ROMs/gb/Reader Rule Game (World).gb";
    const DOCUMENT: &str = "com.android.externalstorage.documents:primary:ROMs/gb/Reader Rule Game (World).gb";
    const SYSTEMS: &str = "<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>/storage/emulated/0/ROMs/gb</path><extension>.gb</extension><command>true</command></system></systemList>";

    fn put(path: &Path, bytes: Option<Vec<u8>>) {
        let _ = std::fs::remove_file(path);
        if let Some(bytes) = bytes { std::fs::write(path, bytes).unwrap(); }
    }

    fn launcher_seed() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "gameboy_gambatte":true,"core_catalog_url":"https://example.invalid/reader-cores.json",
            "played":{(DOCUMENT):NOW-90},"play_secs":{(DOCUMENT):90}
        })).unwrap()
    }

    fn history_words(data: &Path, launcher: &str, systems: &str) -> (usize, String) {
        let mut recents = crate::recents::Recents::load(data);
        let changed = crate::android_handheld::take_mobile_history(&mut recents,data,launcher,systems);
        let words = crate::recents::played_line(recents.list.first());
        (changed, words)
    }

    #[test]
    fn file_source_rule_handheld_activity_files() {
        let d = TestDir::new("handheld-activity-file-rule");
        let launcher = d.join("launcher.json");
        for (label,bytes) in crate::file_faults::faults(&launcher_seed()) {
            let data = d.join(format!("launcher-{label}"));std::fs::create_dir_all(&data).unwrap();
            put(&launcher,bytes);
            let json = launcher_json(&d);
            let channel = channel_url(&json,"");
            let (changed,words) = history_words(&data,&json,SYSTEMS);
            assert!(!words.trim().is_empty(),"{label}: history has no words");
            if label == "valid" {assert!(crate::play_route::gambatte_from_json(&json));assert_eq!(channel,"https://example.invalid/reader-cores.json");assert_eq!(changed,1);assert!(words.starts_with("Last played"));}
            assert_eq!(channel_url(&json,"https://example.invalid/bundled.json"),"https://example.invalid/bundled.json");
        }
        let es = d.join("es_systems.cfg");
        let json = String::from_utf8(launcher_seed()).unwrap();
        for (label,bytes) in crate::file_faults::faults(SYSTEMS.as_bytes()) {
            let data = d.join(format!("systems-{label}"));std::fs::create_dir_all(&data).unwrap();
            put(&es,bytes);
            let text = system_list(&es);
            let (changed,words) = history_words(&data,&json,&text);
            assert!(!words.trim().is_empty());
            if label == "valid" {assert_eq!(changed,1);assert_eq!(text,SYSTEMS);}
            let replaced = keep_system_list(&es,SYSTEMS).unwrap();
            if label == "valid" {assert!(!replaced,"a recognised generated list must retain its cache stamp");}
            assert_eq!(system_list(&es),SYSTEMS,"{label}: generated system list must be readable again");
        }
        let playing = d.join("android-playing.json");
        for with_name in [true,false] {
            let mut seed = serde_json::json!({"path":ROM,"system":"gb","at":NOW-90});
            if with_name {seed["name"] = "Handheld Reader Game".into();}
            for (label,bytes) in crate::file_faults::faults(&serde_json::to_vec(&seed).unwrap()) {
                put(&playing,bytes);
                let note = take_playing_note(&playing);
                assert!(!playing.exists(),"{label}: interrupted session is consumed once");
                let data = d.join(format!("playing-{with_name}-{label}"));std::fs::create_dir_all(&data).unwrap();
                let mut recents = crate::recents::Recents::load(&data);
                if let Some(note) = note {
                    let game = resumed_game(&note,NOW);
                    if label == "valid" {assert_eq!(game.path,Path::new(ROM));assert_eq!(game.system,"gb");assert_eq!(game.secs,90);assert_eq!(game.name,if with_name {"Handheld Reader Game"}else{"Reader Rule Game"});}
                    recents.note(&game.system,&game.name,&game.path,game.secs);
                } else if label == "valid" {panic!("valid playing note was not read");}
                assert!(!crate::recents::played_line(recents.list.first()).trim().is_empty(),"{label}: returning shelf history has no words");
            }
        }
    }

    #[test]
    fn file_source_rule_handheld_return_picture() {
        let d = TestDir::new("handheld-return-picture-file-rule");let png = d.join("return.png");
        let mut seed = Vec::new();
        {let mut enc=png::Encoder::new(&mut seed,2,2);enc.set_color(png::ColorType::Rgb);enc.write_header().unwrap().write_image_data(&[40,80,120].repeat(4)).unwrap();}
        let note = crate::eject::LastPicture {key:"reader-game".into(),picture:Some(png.clone()),rect:Some((1,1,2,2)),window:(6,4),saved:true,at_ms:NOW*1000};
        let json = serde_json::to_vec(&note).unwrap();let note_path=crate::eject::LastPicture::path(&d);
        for mutate_picture in [false,true] {
            for (label,bytes) in crate::file_faults::faults(if mutate_picture {&seed}else{&json}) {
                std::fs::write(&png,&seed).unwrap();std::fs::write(&note_path,&json).unwrap();
                put(if mutate_picture {&png}else{&note_path},bytes);
                let got = crate::eject::LastPicture::take(&d,NOW*1000+1).and_then(|n|return_frame(&n,Some((6,4))));
                if label == "valid" {let frame=got.as_ref().expect("valid retained picture reaches the return compositor");assert_eq!((frame.w,frame.h),(6,4));assert_eq!(frame.px.iter().filter(|&&p|p==0x285078).count(),4);assert_eq!(frame.px[7],0x285078);}
                if let Some(frame)=got {assert_eq!(frame.px.len(),(frame.w*frame.h)as usize);}
                // No picture takes the existing shelf scene, never an absent scene.
                let way=crate::eject::Way::new(crate::eject::Screen::Lcd,false,false);
                assert!(matches!(way.moment(std::time::Duration::ZERO),crate::eject::Moment::Shelf{..}));
            }
        }
    }
}
