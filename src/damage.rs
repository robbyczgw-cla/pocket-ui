//! Which rows of a frame changed since the one before: pocket-ui says them to the screen's bridge with each picture
//! (`wl_surface.damage_buffer`), and the Brick's `pocket-fb` puts only those rows into its GPU texture. The whole
//! 1024x768 upload cost 6.8-7.9 ms of every frame on the Brick's A133P, memory-bound like the paint beside it
//! (BRICK-60FPS.md); a clock, a cursor or one moving case changes a few rows of it.
//!
//! pocket-ui paints every frame whole, so it finds the rows by content: a 64-bit hash per row against the frame before
//! ([`RowHashes`]). What it doesn't know (the first frame, a new size, after a failed frame) is the whole frame.

/// Rows of a picture, top first: `Spans` are half-open `start..end`, sorted, apart by more than [`MERGE_GAP`]; `All` is
/// every row (a picture nobody said anything about).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rows {
    All,
    Spans(Vec<(u32, u32)>),
}

/// Spans this close are one: each upload call has its own cost, a few rows more in one call are cheaper than two calls.
pub const MERGE_GAP: u32 = 16;

impl Default for Rows {
    fn default() -> Rows {
        Rows::Spans(Vec::new())
    }
}

impl Rows {
    pub fn none() -> Rows {
        Rows::default()
    }

    /// Adds rows `start..end` (an empty range adds nothing).
    pub fn add(&mut self, start: u32, end: u32) {
        let Rows::Spans(spans) = self else { return };
        if start >= end {
            return;
        }
        let (mut s, mut e) = (start, end);
        // Everything that touches the new span (within MERGE_GAP) goes into it.
        let first = spans.partition_point(|&(_, se)| se.saturating_add(MERGE_GAP) < s);
        let mut last = first;
        while last < spans.len() && spans[last].0 <= e.saturating_add(MERGE_GAP) {
            s = s.min(spans[last].0);
            e = e.max(spans[last].1);
            last += 1;
        }
        spans.splice(first..last, [(s, e)]);
    }

    /// Everything in `other` too.
    pub fn union(&mut self, other: &Rows) {
        match other {
            Rows::All => *self = Rows::All,
            Rows::Spans(o) => {
                for &(s, e) in o {
                    self.add(s, e);
                }
            }
        }
    }

    /// How many of a picture's `height` rows these are.
    pub fn count(&self, height: u32) -> u32 {
        match self {
            Rows::All => height,
            Rows::Spans(s) => s.iter().map(|&(a, b)| b.min(height).saturating_sub(a.min(height))).sum(),
        }
    }

    /// The spans inside a picture of `height` rows (`All` is one span over all of them).
    pub fn spans(&self, height: u32) -> Vec<(u32, u32)> {
        match self {
            Rows::All => vec![(0, height)],
            Rows::Spans(s) => s.iter().map(|&(a, b)| (a.min(height), b.min(height))).filter(|(a, b)| a < b).collect(),
        }
    }
}

/// The row hashes of the frame painted last, to find the rows the next one changes.
#[derive(Default)]
pub struct RowHashes {
    size: (u32, u32),
    hashes: Vec<u64>,
    /// Per row of the last call: whether it changed (kept to save an allocation a frame).
    new: Vec<bool>,
}

/// The threads that hash a frame's rows (the painting one and two more). Reading the 3 MB of a 1024x768 frame is at
/// the A53's memory floor on one core (2.0 ms); three cores read it in 1.2 ms (bench on the Brick, docs/TRIMUI-BRICK.md).
/// The Brick has four: pocket-fb's picture goes up on the fourth meanwhile.
pub const HASH_THREADS: usize = 3;

impl RowHashes {
    /// The next frame is compared with nothing: it is all new (a failed frame, a new surface).
    pub fn forget(&mut self) {
        self.size = (0, 0);
        self.hashes.clear();
    }

    /// The rows of `pixels` (`w` by `h`, row by row) that differ from the frame given last; that frame is then this one.
    /// The rows are hashed in [`HASH_THREADS`] bands at once.
    pub fn changed(&mut self, pixels: &[u32], w: u32, h: u32) -> Rows {
        self.changed_in(pixels, w, h, HASH_THREADS)
    }

    /// `changed` with the rows hashed in `bands` bands, each on its own thread (one: on this thread only).
    pub fn changed_in(&mut self, pixels: &[u32], w: u32, h: u32, bands: usize) -> Rows {
        let (wu, hu) = (w as usize, h as usize);
        if w == 0 || pixels.len() < wu * hu {
            self.forget();
            return Rows::All;
        }
        let known = self.size == (w, h) && self.hashes.len() == hu;
        self.hashes.resize(hu, 0);
        self.new.resize(hu, false);
        self.size = (w, h);
        let hash_band = |rows: &[u32], kept: &mut [u64], new: &mut [bool]| {
            for ((row, kept), new) in rows.chunks_exact(wu).zip(kept.iter_mut()).zip(new.iter_mut()) {
                let hash = row_hash(row);
                *new = !known || *kept != hash;
                *kept = hash;
            }
        };
        let per = hu.div_ceil(bands.clamp(1, hu.max(1)));
        let pixels = &pixels[..wu * hu];
        if per >= hu {
            hash_band(pixels, &mut self.hashes, &mut self.new);
        } else {
            std::thread::scope(|s| {
                let mut parts = pixels.chunks(per * wu).zip(self.hashes.chunks_mut(per)).zip(self.new.chunks_mut(per));
                let mine = parts.next();
                for ((rows, kept), new) in parts {
                    s.spawn(move || hash_band(rows, kept, new));
                }
                if let Some(((rows, kept), new)) = mine {
                    hash_band(rows, kept, new);
                }
            });
        }
        if !known {
            return Rows::All;
        }
        let mut rows = Rows::none();
        let mut run: Option<u32> = None;
        for (y, &new) in self.new.iter().enumerate() {
            match (new, run) {
                (true, None) => run = Some(y as u32),
                (false, Some(s)) => {
                    rows.add(s, y as u32);
                    run = None;
                }
                _ => {}
            }
        }
        if let Some(s) = run {
            rows.add(s, h);
        }
        rows
    }
}

/// A row's 64-bit hash: four lanes of xor-multiply over the pixels two at a time (independent lanes keep the A53's
/// multiplier busy while the next words load), then mixed. Not linear, so the same change in two places doesn't cancel out.
pub fn row_hash(row: &[u32]) -> u64 {
    const K: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut lanes = [0x243f_6a88_85a3_08d3u64, 0x1319_8a2e_0370_7344, 0xa409_3822_299f_31d0, 0x082e_fa98_ec4e_6c89];
    let mut quads = row.chunks_exact(8);
    for q in &mut quads {
        for (i, lane) in lanes.iter_mut().enumerate() {
            let word = u64::from(q[2 * i]) | (u64::from(q[2 * i + 1]) << 32);
            *lane = (*lane ^ word).wrapping_mul(K);
        }
    }
    for (i, &p) in quads.remainder().iter().enumerate() {
        lanes[i % 4] = (lanes[i % 4] ^ u64::from(p) ^ 0xff00_0000_0000).wrapping_mul(K);
    }
    let mut h = row.len() as u64;
    for lane in lanes {
        h = (h ^ lane ^ (lane >> 29)).wrapping_mul(K).rotate_left(23);
    }
    h ^ (h >> 32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_merge_when_they_touch_or_come_close() {
        let mut r = Rows::none();
        r.add(100, 110);
        r.add(10, 20);
        r.add(200, 201);
        assert_eq!(r, Rows::Spans(vec![(10, 20), (100, 110), (200, 201)]));
        r.add(115, 120);
        assert_eq!(r, Rows::Spans(vec![(10, 20), (100, 120), (200, 201)]), "5 rows apart: one span");
        r.add(18, 190);
        assert_eq!(r, Rows::Spans(vec![(10, 201)]), "one span over the others takes them in");
        r.add(5, 5);
        assert_eq!(r.count(768), 191);
        let mut all = Rows::All;
        all.add(1, 2);
        assert_eq!(all, Rows::All);
        r.union(&Rows::All);
        assert_eq!(r.spans(768), vec![(0, 768)]);
        assert_eq!(Rows::Spans(vec![(700, 900)]).spans(768), vec![(700, 768)]);
        assert_eq!(Rows::Spans(vec![(800, 900)]).count(768), 0);
    }

    /// The rows are hashed in bands on threads: a change on a band's first or last row, a frame whose rows don't
    /// divide by the bands, a frame of fewer rows than bands, all find the same rows as one thread does.
    #[test]
    fn rows_hashed_on_threads_find_the_same_rows_as_one_thread() {
        for (w, h) in [(1024u32, 768u32), (640, 481), (7, 2), (33, 1), (1240, 1080)] {
            let mut frame: Vec<u32> = (0..w * h).map(|i| i.wrapping_mul(2_654_435_761)).collect();
            let (mut one, mut three, mut five) = (RowHashes::default(), RowHashes::default(), RowHashes::default());
            assert_eq!(three.changed_in(&frame, w, h, 3), Rows::All);
            assert_eq!(five.changed_in(&frame, w, h, 5), Rows::All);
            one.changed_in(&frame, w, h, 1);
            // Every band's edges at 3 and 5 bands, the first and last row, one in the middle.
            let mut edges = vec![0, h - 1, h / 2];
            for bands in [3, 5] {
                let per = h.div_ceil(bands);
                for b in 1..bands {
                    edges.extend([(b * per).saturating_sub(1), b * per].into_iter().filter(|&y| y < h));
                }
            }
            for (k, &y) in edges.iter().enumerate() {
                frame[(y * w + (k as u32 % w)) as usize] ^= 0x0001_0101;
                let want = one.changed_in(&frame, w, h, 1);
                assert_eq!(want.count(h), 1, "{w}x{h}: row {y} changed alone");
                assert_eq!(three.changed_in(&frame, w, h, 3), want, "{w}x{h}, 3 bands: row {y}");
                assert_eq!(five.changed_in(&frame, w, h, 5), want, "{w}x{h}, 5 bands: row {y}");
            }
            // Nothing changed: no rows, on threads too.
            assert_eq!(three.changed_in(&frame, w, h, 3), Rows::none());
        }
    }

    #[test]
    fn the_rows_a_frame_changed_are_found_and_nothing_else() {
        let (w, h) = (1024u32, 768u32);
        let mut frame: Vec<u32> = (0..w * h).map(|i| 0x0020_3040 + (i % 7)).collect();
        let mut hashes = RowHashes::default();
        assert_eq!(hashes.changed(&frame, w, h), Rows::All, "the first frame is all new");
        assert_eq!(hashes.changed(&frame, w, h), Rows::none(), "the same frame again changes nothing");
        // One pixel in row 300, the clock's rows 20..40, the bottom row.
        frame[(300 * w + 517) as usize] ^= 1;
        for y in 20..40 {
            frame[(y * w + 900) as usize] = 0xffffff;
        }
        frame[((h - 1) * w) as usize] = 0;
        assert_eq!(hashes.changed(&frame, w, h), Rows::Spans(vec![(20, 40), (300, 301), (767, 768)]));
        // The same change in two places of a row (a symmetric layout) is still a change.
        frame[(500 * w + 10) as usize] ^= 0x10;
        frame[(500 * w + 522) as usize] ^= 0x10;
        assert_eq!(hashes.changed(&frame, w, h), Rows::Spans(vec![(500, 501)]));
        // Two pixels swapped within a row.
        frame.swap((600 * w + 1) as usize, (600 * w + 2) as usize);
        assert_eq!(hashes.changed(&frame, w, h).count(h), 1);
        assert_eq!(hashes.changed(&frame[..(w * (h - 1)) as usize], w, h - 1), Rows::All, "a new size is all new");
        hashes.forget();
        assert_eq!(hashes.changed(&frame, w, h), Rows::All);
    }
}
