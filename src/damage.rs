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
}

impl RowHashes {
    /// The next frame is compared with nothing: it is all new (a failed frame, a new surface).
    pub fn forget(&mut self) {
        self.size = (0, 0);
        self.hashes.clear();
    }

    /// The rows of `pixels` (`w` by `h`, row by row) that differ from the frame given last; that frame is then this one.
    pub fn changed(&mut self, pixels: &[u32], w: u32, h: u32) -> Rows {
        let (wu, hu) = (w as usize, h as usize);
        if w == 0 || pixels.len() < wu * hu {
            self.forget();
            return Rows::All;
        }
        let known = self.size == (w, h) && self.hashes.len() == hu;
        self.hashes.resize(hu, 0);
        self.size = (w, h);
        let mut rows = Rows::none();
        let mut run: Option<u32> = None;
        for (y, (row, kept)) in pixels.chunks_exact(wu).take(hu).zip(self.hashes.iter_mut()).enumerate() {
            let hash = row_hash(row);
            let same = known && *kept == hash;
            *kept = hash;
            match (same, run) {
                (false, None) => run = Some(y as u32),
                (true, Some(s)) => {
                    rows.add(s, y as u32);
                    run = None;
                }
                _ => {}
            }
        }
        if let Some(s) = run {
            rows.add(s, h);
        }
        if known {
            rows
        } else {
            Rows::All
        }
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
