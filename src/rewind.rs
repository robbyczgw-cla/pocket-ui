//! Rewind history: the last seconds of save states, kept as small reverse deltas.
//!
//! The newest state is stored whole; every older one is stored as the difference to the state
//! after it (XOR, with runs of unchanged 8-byte words left out). Two neighbouring frames differ in
//! a few kilobytes, so a minute of a 400 KB state fits into a few megabytes, and stepping back is
//! one XOR pass over the newest state. When the history outgrows its budget the oldest delta goes.

use std::collections::VecDeque;

pub struct Rewind {
    /// The newest state, whole.
    head: Vec<u8>,
    has_head: bool,
    /// Deltas, oldest first: `deltas[i]` turns state i+1 back into state i.
    deltas: VecDeque<Vec<u8>>,
    delta_bytes: usize,
    budget: usize,
    max_states: usize,
}

fn put_varint(out: &mut Vec<u8>, mut v: usize) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn get_varint(data: &[u8], pos: &mut usize) -> Option<usize> {
    let mut v = 0usize;
    let mut shift = 0;
    loop {
        let b = *data.get(*pos)?;
        *pos += 1;
        v |= ((b & 0x7f) as usize) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
        if shift > 56 {
            return None;
        }
    }
}

fn word(data: &[u8], i: usize) -> u64 {
    u64::from_ne_bytes(data[i * 8..i * 8 + 8].try_into().unwrap())
}

/// `old ^ new` as tokens: (words unchanged, words changed, the changed words XORed), repeated, then the
/// trailing bytes that don't fill a word. Both slices have the same length.
fn encode(old: &[u8], new: &[u8]) -> Vec<u8> {
    let words = old.len() / 8;
    let mut out = Vec::with_capacity(256);
    let mut i = 0;
    while i < words {
        let start = i;
        while i < words && word(old, i) == word(new, i) {
            i += 1;
        }
        let changed_from = i;
        while i < words && word(old, i) != word(new, i) {
            i += 1;
        }
        if i == words && changed_from == i {
            break; // nothing but unchanged words left; the decoder stops at the end of the state
        }
        put_varint(&mut out, changed_from - start);
        put_varint(&mut out, i - changed_from);
        for w in changed_from..i {
            out.extend_from_slice(&(word(old, w) ^ word(new, w)).to_ne_bytes());
        }
    }
    for k in words * 8..old.len() {
        out.push(old[k] ^ new[k]);
    }
    out
}

/// XORs a delta into `state`, turning the newer state into the older one.
fn apply(state: &mut [u8], delta: &[u8]) -> bool {
    let words = state.len() / 8;
    let tail = state.len() % 8;
    if delta.len() < tail {
        return false;
    }
    // The trailing bytes sit after the tokens; a run of unchanged words at the end has no token.
    let (tokens, tail_bytes) = delta.split_at(delta.len() - tail);
    let (mut pos, mut w) = (0, 0);
    while pos < tokens.len() {
        let (Some(skip), Some(run)) = (get_varint(tokens, &mut pos), get_varint(tokens, &mut pos)) else { return false };
        w += skip;
        if w + run > words || pos + run * 8 > tokens.len() {
            return false;
        }
        for chunk in tokens[pos..pos + run * 8].chunks_exact(8) {
            let cur = u64::from_ne_bytes(state[w * 8..w * 8 + 8].try_into().unwrap());
            state[w * 8..w * 8 + 8].copy_from_slice(&(cur ^ u64::from_ne_bytes(chunk.try_into().unwrap())).to_ne_bytes());
            w += 1;
        }
        pos += run * 8;
    }
    for (k, b) in tail_bytes.iter().enumerate() {
        state[words * 8 + k] ^= b;
    }
    true
}

impl Rewind {
    /// Keeps at most `budget` bytes in total and `max_states` steps back.
    pub fn new(budget: usize, max_states: usize) -> Self {
        Rewind { head: Vec::new(), has_head: false, deltas: VecDeque::new(), delta_bytes: 0, budget, max_states }
    }

    pub fn clear(&mut self) {
        self.has_head = false;
        self.head.clear();
        self.deltas.clear();
        self.delta_bytes = 0;
    }

    /// How many frames can be stepped back.
    pub fn len(&self) -> usize {
        self.deltas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.deltas.is_empty()
    }

    /// Memory in use.
    pub fn bytes(&self) -> usize {
        self.head.len() + self.delta_bytes
    }

    /// Adds the state after the latest frame.
    pub fn push(&mut self, state: &[u8]) {
        if state.is_empty() {
            return;
        }
        if !self.has_head || self.head.len() != state.len() {
            // First state, or a core whose state size changed: start over.
            self.clear();
            self.head.extend_from_slice(state);
            self.has_head = true;
            return;
        }
        let delta = encode(&self.head, state);
        self.delta_bytes += delta.len();
        self.deltas.push_back(delta);
        self.head.copy_from_slice(state);
        while self.deltas.len() > self.max_states || (self.bytes() > self.budget && !self.deltas.is_empty()) {
            if let Some(old) = self.deltas.pop_front() {
                self.delta_bytes -= old.len();
            }
        }
    }

    /// Goes back one frame and returns that state, or None when the history is used up.
    pub fn step_back(&mut self) -> Option<&[u8]> {
        let delta = self.deltas.pop_back()?;
        self.delta_bytes -= delta.len();
        if !apply(&mut self.head, &delta) {
            self.clear();
            return None;
        }
        Some(&self.head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake emulator state: some bytes change every frame, most stay.
    fn state(frame: usize, len: usize) -> Vec<u8> {
        let mut s: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
        for k in 0..40 {
            let at = (frame * 977 + k * 4093) % len;
            s[at] = s[at].wrapping_add(frame as u8).wrapping_add(1);
        }
        s
    }

    #[test]
    fn steps_back_through_every_state_exactly() {
        for len in [1000usize, 1003, 8, 7, 65_536 + 5] {
            let mut r = Rewind::new(usize::MAX, usize::MAX);
            let states: Vec<_> = (0..50).map(|f| state(f, len)).collect();
            for s in &states {
                r.push(s);
            }
            assert_eq!(r.len(), 49);
            for f in (0..49).rev() {
                assert_eq!(r.step_back().unwrap(), &states[f][..], "len {len} frame {f}");
            }
            assert!(r.step_back().is_none() && r.is_empty());
        }
    }

    #[test]
    fn identical_and_fully_different_states_roundtrip() {
        let mut r = Rewind::new(usize::MAX, usize::MAX);
        let a = vec![7u8; 4096];
        let b: Vec<u8> = (0..4096).map(|i| !(i as u8)).collect();
        r.push(&a);
        r.push(&a);
        r.push(&b);
        r.push(&b);
        assert_eq!(r.step_back().unwrap(), &b[..]);
        assert_eq!(r.step_back().unwrap(), &a[..]);
        assert_eq!(r.step_back().unwrap(), &a[..]);
        assert!(r.step_back().is_none());
    }

    #[test]
    fn deltas_are_small_and_the_budget_drops_the_oldest() {
        let len = 400_000;
        let mut r = Rewind::new(1_000_000, usize::MAX);
        for f in 0..2000 {
            r.push(&state(f, len));
        }
        assert!(r.bytes() <= 1_000_000, "{}", r.bytes());
        assert!(r.len() > 100, "400 KB states at ~1 KB per frame: {} frames in 1 MB", r.len());
        // What is left is still exact, newest first.
        let n = r.len();
        assert_eq!(r.step_back().unwrap(), &state(1998, len)[..]);
        assert_eq!(r.len(), n - 1);
    }

    /// `cargo test --release --lib rewind_speed -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn rewind_speed() {
        for len in [130_000usize, 400_000, 1_000_000] {
            let mut r = Rewind::new(48 << 20, 3600);
            let states: Vec<_> = (0..120).map(|f| state(f, len)).collect();
            let t = std::time::Instant::now();
            for round in 0..10 {
                for s in &states {
                    r.push(s);
                    let _ = round;
                }
            }
            let per = t.elapsed().as_secs_f64() * 1000.0 / 1200.0;
            let t = std::time::Instant::now();
            let mut n = 0;
            while r.step_back().is_some() {
                n += 1;
            }
            println!("{len} bytes: push {per:.3} ms, step back {:.3} ms, {n} frames kept", t.elapsed().as_secs_f64() * 1000.0 / n.max(1) as f64);
        }
    }

    #[test]
    fn frame_limit_keeps_the_newest() {
        let mut r = Rewind::new(usize::MAX, 10);
        for f in 0..100 {
            r.push(&state(f, 512));
        }
        assert_eq!(r.len(), 10);
        let mut last = Vec::new();
        while let Some(s) = r.step_back() {
            last = s.to_vec();
        }
        assert_eq!(last, state(89, 512));
    }

    #[test]
    fn a_changed_state_size_starts_a_new_history() {
        let mut r = Rewind::new(usize::MAX, usize::MAX);
        r.push(&[1u8; 64]);
        r.push(&[2u8; 64]);
        assert_eq!(r.len(), 1);
        r.push(&[3u8; 80]);
        assert_eq!(r.len(), 0);
        r.push(&[4u8; 80]);
        assert_eq!(r.step_back().unwrap(), &[3u8; 80][..]);
        r.clear();
        assert!(r.step_back().is_none());
        assert_eq!(r.bytes(), 0);
    }

    #[test]
    fn corrupt_deltas_end_the_history_instead_of_panicking() {
        let mut s = vec![0u8; 64];
        assert!(!apply(&mut s, &[0x01, 0xff, 0xff]));
        assert!(!apply(&mut s, &[0x00, 0x09]));
        assert!(!apply(&mut s, &[0x80]));
        assert!(!apply(&mut s, &[0x00, 0x01, 1, 2]), "a run needs its 8 bytes");
        let mut odd = vec![0u8; 9];
        assert!(!apply(&mut odd, &[]), "the trailing byte is missing");
    }
}
