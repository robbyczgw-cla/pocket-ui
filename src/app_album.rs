//! Part of the pocket-ui binary: the App's side of the photo album in a game's case (`pocket_ui::album`,
//! `album_paint`). The card reads the album when it opens (`card_album`: one small file); the case's photos are
//! decoded on a thread of their own when the case opens, each answer a `UserEvent::AlbumPicture`, so nothing is
//! decoded on the UI thread; X then A takes one out of the album (`album_delete`).

use std::path::{Path, PathBuf};

use pocket_ui::album::{self, AlbumData};
use pocket_ui::image::Image;

use super::{App, UserEvent};

impl App {
    /// The album of a game for its card: its photos, whether its save is one pocket-ui reads, the switch.
    pub(crate) fn card_album(&self, system: &str, path: &Path) -> AlbumData {
        let reads = self.case_progress(system, path).is_some();
        let saves = pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::ProgressFromSaves);
        AlbumData::read(&self.data_dir, system, path, reads, self.moment_photos, saves)
    }

    /// The photos to slide into the case on the way back from `rom` (`app_cabinet::begin_eject`): those its camera kept
    /// this session (`album_since`, set when the game ended); in a snapshot, those not seen yet. The newest three, decoded
    /// here (a few small JPEGs, once at a game's end).
    pub(crate) fn album_tucked(&mut self, system: &str, rom: &Path) -> Vec<Image> {
        if !self.moment_photos {
            return Vec::new();
        }
        let dir = album::dir(&self.data_dir, system, rom);
        let since = self.album_since.take().unwrap_or_else(|| album::seen(&dir) + 1);
        let photos: Vec<_> = album::list(&dir).into_iter().filter(|p| p.at >= since).collect();
        photos.iter().rev().take(3).rev().filter_map(|p| pocket_ui::image::load_jpeg(&p.file)).collect()
    }

    /// The case was opened: its photos are seen (their "New" marks go from the next opening on).
    pub(crate) fn album_opened(&mut self) {
        let Some(dir) = self.card.as_ref().and_then(|o| o.card.data.album.as_ref()).map(|a| a.dir.clone()) else { return };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        if let Err(e) = album::mark_seen(&dir, now) {
            eprintln!("album: seen not kept: {e}");
        }
    }

    /// Each frame the open card is painted: what the case still needs decoded is asked for once, on a thread.
    pub(crate) fn album_pictures(&mut self) {
        let Some(open) = self.card.as_mut() else { return };
        let Some(bx) = open.card.boxed.as_mut() else { return };
        if bx.album.is_none() {
            return;
        }
        let Some(data) = open.card.data.album.as_ref() else { return };
        let missing: Vec<PathBuf> = data.photos.iter().map(|p| p.file.clone()).filter(|f| !bx.album_pics.contains_key(f)).collect();
        if missing.is_empty() {
            return;
        }
        // Asked for: a place each, so they are asked once (None until the answer, and for a picture that can't be read).
        for f in &missing {
            bx.album_pics.insert(f.clone(), None);
        }
        // Snapshots decode at once, as the cabinet's covers do (`cabinet.inline`).
        if self.cabinet.inline {
            for f in missing {
                let img = pocket_ui::image::load_jpeg(&f).map(std::rc::Rc::new);
                if let Some(bx) = self.card.as_mut().and_then(|o| o.card.boxed.as_mut()) {
                    bx.album_pics.insert(f, img);
                }
            }
            return;
        }
        let proxy = self.proxy.clone();
        let _ = std::thread::Builder::new().name("album".into()).spawn(move || {
            #[cfg(target_os = "linux")]
            pocket_ui::web::lower_priority();
            for f in missing {
                let img = pocket_ui::image::load_jpeg(&f);
                if proxy.send_event(UserEvent::AlbumPicture(f, img)).is_err() {
                    break;
                }
            }
        });
    }

    /// A photo's picture is decoded (None: it couldn't be read; the case shows its grey window then).
    pub(crate) fn album_picture(&mut self, file: PathBuf, img: Option<Image>) {
        if let Some(bx) = self.card.as_mut().and_then(|o| o.card.boxed.as_mut()) {
            if bx.album_pics.contains_key(&file) {
                bx.album_pics.insert(file, img.map(std::rc::Rc::new));
            }
        }
    }

    /// The second A of a delete: the photo leaves the album and the case.
    pub(crate) fn album_delete(&mut self, file: &Path) {
        let Some(open) = self.card.as_mut() else { return };
        let Some(data) = open.card.data.album.as_mut() else { return };
        match album::delete(&data.dir, file) {
            Ok(()) => {
                data.photos.retain(|p| p.file != file);
                if let Some(bx) = open.card.boxed.as_mut() {
                    bx.album_pics.remove(file);
                }
                self.toast = Some(("Photo deleted".to_string(), std::time::Instant::now()));
            }
            Err(e) => {
                eprintln!("album: delete {}: {e}", file.display());
                self.warn("Couldn't delete the photo: is the SD card read-only?");
            }
        }
    }
}
