//! Pictures decoded and scaled on a thread of their own, so that the shelf never waits for a PNG: the app asks
//! (`Loader::request`), draws what it has, and the picture arrives as an event, to be put in the image cache
//! (`app_pictures.rs`). Used for box art, in-game screenshots and title screens. One thread, one picture at a
//! time, started on the first request; the same decoder and box filter as the synchronous snapshots.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};

use crate::image::{self, Image};

/// A picture of a file fitted into a box: the file, the box's width and height.
pub type Key = (PathBuf, i32, i32);

/// The size a `w` x `h` picture is drawn at in a box (what `Image::fit` makes of it), known from the file's header
/// before it is decoded.
pub fn fitted(w: u32, h: u32, max_w: i32, max_h: i32) -> (i32, i32) {
    Image { w, h, px: Vec::new() }.fit(max_w, max_h)
}

/// The file read and scaled to fit the box; `None` for a missing, damaged or oversized one (`image::size_ok`).
pub fn load_fitted(key: &Key) -> Option<Image> {
    let img = image::load_image(&key.0)?;
    let (w, h) = img.fit(key.1, key.2);
    (w > 0 && h > 0).then(|| img.resized(w, h))
}

pub struct Loader {
    tx: mpsc::Sender<Option<Key>>,
    /// Pictures the selection is heading for (`App::cover_prefetch`): decoded only when nothing asked for is waiting,
    /// nearest first, and replaced by the next window.
    soon: Arc<Mutex<VecDeque<Key>>>,
}

impl Loader {
    /// `done` runs on the thread for every request, with the picture or `None`. Prefetched pictures are answered the same
    /// way; the cache tells them apart (`App::picture_arrived`).
    pub fn start(done: impl Fn(Key, Option<Image>) + Send + 'static) -> Loader {
        let (tx, rx) = mpsc::channel::<Option<Key>>();
        let soon: Arc<Mutex<VecDeque<Key>>> = Arc::default();
        let next_soon = soon.clone();
        let _ = std::thread::Builder::new().name("pictures".into()).spawn(move || {
            let mut waiting = Vec::new();
            loop {
                // Drain before every decode: the newest selection goes next. Keep older
                // jobs so gallery/cache consumers still receive every answer. `None` only wakes the thread for `soon`.
                for key in rx.try_iter().flatten() {
                    waiting.retain(|old| old != &key);
                    waiting.push(key);
                }
                if crate::background_io::paused() {
                    match rx.recv_timeout(std::time::Duration::from_millis(150)) {
                        Ok(key) => waiting.extend(key),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    continue;
                }
                let (key, requested) = match waiting.pop() {
                    Some(key) => (key, true),
                    None => {
                        let ahead = next_soon.lock().unwrap().pop_front();
                        match ahead {
                            Some(key) => (key, false),
                            None => match rx.recv() {
                                Ok(key) => {
                                    waiting.extend(key);
                                    continue;
                                }
                                Err(_) => break,
                            },
                        }
                    }
                };
                if let Some(img) = crate::background_io::job(|| load_fitted(&key)) {
                    done(key, img);
                } else if requested {
                    waiting.insert(0, key);
                }
            }
        });
        Loader { tx, soon }
    }

    pub fn request(&self, key: Key) {
        self.soon.lock().unwrap().retain(|k| k != &key);
        let _ = self.tx.send(Some(key));
    }

    /// The pictures to decode in the background, nearest first, in place of the last window's that are not done yet.
    pub fn prefetch(&self, keys: Vec<Key>) {
        *self.soon.lock().unwrap() = keys.into();
        let _ = self.tx.send(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn r36s_secondary_picture_loader_keeps_the_request_until_the_game_ends() {
        let _lock = crate::covers::READER_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let (tx, rx) = mpsc::channel();
        let loader = Loader::start(move |key, _| { let _ = tx.send(key); });
        let key = ("queued.png".into(), 8, 8);
        crate::covers::set_game_on(true);
        loader.request(key.clone());
        let early = rx.recv_timeout(Duration::from_millis(300));
        crate::covers::set_game_on(false);
        assert!(matches!(early, Err(mpsc::RecvTimeoutError::Timeout)));
        let resumed = rx.recv_timeout(crate::testutil::timeout(Duration::from_secs(3))).unwrap();
        assert_eq!(resumed, key);
    }

    #[test]
    fn the_worker_returns_fitted_pixels_with_the_same_alpha_and_box_filter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cover.png");
        let mut encoder = png::Encoder::new(std::fs::File::create(&path).unwrap(), 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128,
        ]).unwrap();
        let (tx, rx) = mpsc::channel();
        let loader = Loader::start(move |key, img| { tx.send((key, img)).unwrap(); });
        let key = (path, 1, 1);
        loader.request(key.clone());
        let (returned, img) = rx.recv_timeout(crate::testutil::timeout(Duration::from_secs(5))).expect("cover decoded off-thread");
        assert_eq!(returned, key);
        let img = img.expect("valid cover");
        assert_eq!((img.w, img.h), (1, 1));
        assert_eq!(img.px, [0x5f5f5f]);
    }

    #[test]
    fn a_new_selection_passes_the_backlog_without_losing_older_answers() {
        let (tx, rx) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let loader = Loader::start(move |key, _| {
            let first = key.1 == 0;
            tx.send(key.1).unwrap();
            if first { blocked.recv().unwrap(); }
        });
        loader.request(("missing".into(), 0, 1));
        assert_eq!(rx.recv_timeout(crate::testutil::timeout(Duration::from_secs(5))).unwrap(), 0);
        for i in 1..20 { loader.request(("missing".into(), i, 1)); }
        release.send(()).unwrap();
        let got: Vec<_> = (1..20).map(|_| rx.recv_timeout(crate::testutil::timeout(Duration::from_secs(5))).unwrap()).collect();
        assert_eq!(got, (1..20).rev().collect::<Vec<_>>());
    }

    #[test]
    fn the_size_known_from_the_header_is_the_size_drawn() {
        for (w, h) in [(256, 224), (640, 480), (240, 160), (1, 1), (4096, 100)] {
            let img = Image { w, h, px: vec![0; (w * h) as usize] };
            assert_eq!(fitted(w, h, 590, 250), img.fit(590, 250));
        }
    }

    #[test]
    fn a_missing_file_comes_back_as_none_and_a_request_is_always_answered() {
        let (tx, rx) = mpsc::channel();
        let loader = Loader::start(move |key, img| {
            let _ = tx.send((key, img.is_some()));
        });
        loader.request((PathBuf::from("/nonexistent/none.png"), 100, 100));
        let (key, got) = rx.recv_timeout(crate::testutil::timeout(Duration::from_secs(5))).expect("answered");
        assert_eq!(key.0, PathBuf::from("/nonexistent/none.png"));
        assert!(!got);
    }
}
