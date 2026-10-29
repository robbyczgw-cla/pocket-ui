//! Pictures decoded and scaled on a thread of their own, so that the shelf never waits for a PNG: the app asks
//! (`Loader::request`), draws what it has, and the picture arrives as an event, to be put in the image cache
//! (`app_pictures.rs`). Used for box art, in-game screenshots and title screens. One thread, one picture at a
//! time, started on the first request; the same decoder and box filter as the synchronous snapshots.

use std::path::PathBuf;
use std::sync::mpsc;

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
    tx: mpsc::Sender<Key>,
}

impl Loader {
    /// `done` runs on the thread for every request, with the picture or `None`.
    pub fn start(done: impl Fn(Key, Option<Image>) + Send + 'static) -> Loader {
        let (tx, rx) = mpsc::channel::<Key>();
        let _ = std::thread::Builder::new().name("pictures".into()).spawn(move || {
            for key in rx {
                let img = load_fitted(&key);
                done(key, img);
            }
        });
        Loader { tx }
    }

    pub fn request(&self, key: Key) {
        let _ = self.tx.send(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
        let (returned, img) = rx.recv_timeout(Duration::from_secs(5)).expect("cover decoded off-thread");
        assert_eq!(returned, key);
        let img = img.expect("valid cover");
        assert_eq!((img.w, img.h), (1, 1));
        assert_eq!(img.px, [0x5f5f5f]);
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
        let (key, got) = rx.recv_timeout(Duration::from_secs(5)).expect("answered");
        assert_eq!(key.0, PathBuf::from("/nonexistent/none.png"));
        assert!(!got);
    }
}
