//! The pages of a game's manual PDF as the JPEG pictures they are (docs/SCREENSCRAPER.md "Manuals").
//!
//! ScreenScraper's manuals (`manuel`) are scans that Ghostscript put together: every page holds exactly one picture,
//! a JPEG (`DCTDecode`), and nothing else (no fonts, no text). So pocket-ui needs no PDF renderer: [`pages`] walks
//! the page tree and hands back where each page's JPEG lies in the file; the caller writes those bytes out unchanged
//! (`screenscraper::save_manual`), once, on the loading thread. A page that is anything else (text, a drawing, a
//! picture in another format) comes back as [`Page::Other`] and the viewer says so on that page.
//!
//! What it reads: the classic `xref` table and the PDF 1.5 cross-reference stream (with the PNG predictors), object
//! streams (`ObjStm`), `/Prev` chains, inherited `/Resources`, an indirect `/Length`. A broken table falls back to
//! looking for `N G obj` in the file. Flate comes from `miniz_oxide`, already a dependency.
//!
//! **The file comes from the network**, so everything has a limit and nothing panics: at most [`MAX_OBJECTS`] objects
//! and [`MAX_PAGES`] pages, every inflated stream at most [`MAX_INFLATE`] bytes and all of them together
//! [`MAX_INFLATE_TOTAL`] (a zip bomb stops there), nesting at most 64 deep, and the whole reading at most
//! [`MAX_TIME`]. An encrypted file, or one over a limit, is not readable ([`pages`] gives None): the manual is then
//! "can't be read here", never a hang.
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The most objects a manual may have (the study's largest: 296).
pub const MAX_OBJECTS: usize = 200_000;
/// The most pages read (the study's largest: 72).
pub const MAX_PAGES: usize = 2000;
/// The most one stream (an object or cross-reference stream) may inflate to.
pub const MAX_INFLATE: usize = 16 << 20;
/// The most all inflated streams of a file may come to.
pub const MAX_INFLATE_TOTAL: usize = 64 << 20;
/// The longest a file may take to read (the study's manuals: about 1 ms).
pub const MAX_TIME: Duration = Duration::from_secs(5);
/// The most values (numbers, names, strings, array and dictionary entries) parsed from one file, with repeats: a real
/// manual parses a few thousand. Counted before anything is made of them, so a dictionary of millions of entries, or
/// one shared by every page, stops here (Sol 26.11.2026).
/// 300 000 values come to at most some 30 MB of parsed values with their maps and vectors, also on the smallest
/// handhelds (Sol 26.11.2026: 4 million was no safe budget).
pub const MAX_ELEMENTS: usize = 300_000;
/// The most entries in one array or dictionary (the study's widest: a page tree of 72 kids).
pub const MAX_ENTRIES: usize = 50_000;
/// The most bytes all parsed values of one file may take, counted as they are made and never given back (so it bounds
/// the peak too): a value's own size plus its slot in a vector or map, a name's letters on top.
/// One budget for everything made from the file: parsed values, names, every copy of a value, stream data copied or
/// inflated (the lead, 26.11.2026: one global byte budget, every growth counts, over it = not readable).
pub const MAX_PARSED_BYTES: usize = 32 << 20;
/// The longest name taken: the PDF standard's limit, checked before the name is made.
pub const MAX_NAME: usize = 127;
/// What one value costs at least ([`MAX_PARSED_BYTES`]).
const VALUE_BYTES: usize = std::mem::size_of::<Obj>() + 32;

thread_local! {
    /// What is left of [`MAX_ELEMENTS`] for the file being read on this thread (`pages` sets it).
    static ELEMENTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// The bytes the parsed values of this file took so far ([`MAX_PARSED_BYTES`]).
    static BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// When this file's reading gives up ([`MAX_TIME`]), checked inside the lexer too.
    static DEADLINE: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

/// One value more of `extra` bytes beyond its own; false once a budget or the time is spent (and from then on: the
/// file is then not readable as a whole).
fn spend(weight: usize, extra: usize) -> bool {
    let left = ELEMENTS.with(|e| e.get());
    let bytes = BYTES.with(|b| b.get()).saturating_add(VALUE_BYTES * weight.max(1)).saturating_add(extra);
    // The clock every 1024 values: a long array in one call stops in time too.
    let late = left % 1024 == 0 && DEADLINE.with(|d| d.get()).is_some_and(|t| Instant::now() > t);
    if left < weight.max(1) || bytes > MAX_PARSED_BYTES || late {
        ELEMENTS.with(|e| e.set(0));
        return false;
    }
    ELEMENTS.with(|e| e.set(left - weight.max(1)));
    BYTES.with(|b| b.set(bytes));
    true
}

/// What a copy of `o` costs: one value per value in it, and its names' bytes.
fn weigh(o: &Obj, depth: u32) -> (usize, usize) {
    if depth > 64 {
        return (usize::MAX / 4, 0);
    }
    match o {
        Obj::Name(n) => (1, n.len()),
        Obj::Array(a) => a.iter().fold((1, 0), |(v, b), x| {
            let (xv, xb) = weigh(x, depth + 1);
            (v.saturating_add(xv), b.saturating_add(xb))
        }),
        Obj::Dict(d) | Obj::Stream(d, ..) => d.iter().fold((1, 0), |(v, b), (k, x)| {
            let (xv, xb) = weigh(x, depth + 1);
            (v.saturating_add(xv), b.saturating_add(xb).saturating_add(k.len()))
        }),
        _ => (1, 0),
    }
}

/// A copy of `o`, paid for in the budget first; None once it is spent.
fn copy(o: &Obj) -> Option<Obj> {
    let (values, bytes) = weigh(o, 0);
    spend(values, bytes).then(|| o.clone())
}

/// The bytes the last file's parsed values took ([`MAX_PARSED_BYTES`]; tests).
pub fn parsed_bytes() -> usize {
    BYTES.with(|b| b.get())
}

/// The widest cross-reference row or predictor line taken.
const MAX_COLUMNS: usize = 4096;

/// One page of a manual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    /// The page is one JPEG: `bytes` is its range in the file, `width` x `height` its pixels.
    Jpeg { start: usize, len: usize, width: u32, height: u32 },
    /// Something pocket-ui can't show (no single JPEG on it).
    Other,
}

#[derive(Debug, Clone, PartialEq)]
enum Obj {
    Null,
    Bool(bool),
    Num(f64),
    Name(String),
    Str,
    Array(Vec<Obj>),
    Dict(HashMap<String, Obj>),
    Ref(u32),
    /// A stream: its dictionary and the range of its raw bytes in the buffer it was read from.
    Stream(HashMap<String, Obj>, usize, usize),
}

impl Obj {
    fn dict(&self) -> Option<&HashMap<String, Obj>> {
        match self {
            Obj::Dict(d) | Obj::Stream(d, _, _) => Some(d),
            _ => None,
        }
    }
    fn num(&self) -> Option<f64> {
        match self {
            Obj::Num(n) => Some(*n),
            _ => None,
        }
    }
    fn name(&self) -> Option<&str> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }
}

/// Where an object lives: at a byte offset of the file, or as the `index`th object of an object stream.
#[derive(Debug, Clone, Copy)]
enum Loc {
    At(usize),
    InStream(u32, u32),
}

fn is_white(b: u8) -> bool {
    matches!(b, b' ' | b'\n' | b'\r' | b'\t' | 0x0c | 0)
}
fn is_delim(b: u8) -> bool {
    matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

struct Lexer<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    /// At `i`, never past the end (a table's offset can point anywhere).
    fn new(b: &'a [u8], i: usize) -> Self {
        Lexer { b, i: i.min(b.len()) }
    }
    fn skip(&mut self) {
        while self.i < self.b.len() {
            let c = self.b[self.i];
            if is_white(c) {
                self.i += 1;
            } else if c == b'%' {
                while self.i < self.b.len() && self.b[self.i] != b'\n' && self.b[self.i] != b'\r' {
                    self.i += 1;
                }
            } else {
                break;
            }
        }
    }
    fn word(&mut self) -> &'a [u8] {
        self.skip();
        let s = self.i;
        while self.i < self.b.len() && !is_white(self.b[self.i]) && !is_delim(self.b[self.i]) {
            self.i += 1;
        }
        &self.b[s..self.i]
    }
    fn peek_word(&mut self) -> &'a [u8] {
        let keep = self.i;
        let w = self.word();
        self.i = keep;
        w
    }
    fn uint(&mut self) -> Option<u64> {
        std::str::from_utf8(self.word()).ok()?.parse().ok()
    }

    /// One object; `depth` guards against a file that nests without end.
    fn obj(&mut self, depth: u32) -> Option<Obj> {
        if depth > 64 {
            return None;
        }
        self.skip();
        let c = *self.b.get(self.i)?;
        if !spend(1, 0) {
            return None;
        }
        match c {
            b'/' => {
                self.i += 1;
                let s = self.i;
                while self.i < self.b.len() && !is_white(self.b[self.i]) && !is_delim(self.b[self.i]) {
                    self.i += 1;
                }
                // A long name costs as much as its length (it is copied).
                // A name is short (the PDF standard: at most 127 bytes): a longer one ends the file's reading before
                // anything is made of it. Its letters are counted as the string they become (Sol 26.11.2026).
                if self.i - s > MAX_NAME {
                    ELEMENTS.with(|e| e.set(0));
                    return None;
                }
                let name = String::from_utf8_lossy(&self.b[s..self.i]).into_owned();
                if !spend(1, name.len()) {
                    return None;
                }
                Some(Obj::Name(name))
            }
            b'<' if self.b.get(self.i + 1) == Some(&b'<') => {
                self.i += 2;
                let mut d = HashMap::new();
                loop {
                    self.skip();
                    if self.b.get(self.i..self.i + 2)? == b">>" {
                        self.i += 2;
                        break;
                    }
                    let Obj::Name(k) = self.obj(depth + 1)? else { return None };
                    let v = self.obj(depth + 1)?;
                    if d.len() >= MAX_ENTRIES {
                        // Over the limit: the whole file is not readable, not only this value.
                        ELEMENTS.with(|e| e.set(0));
                        return None;
                    }
                    d.insert(k, v);
                }
                Some(Obj::Dict(d))
            }
            b'<' => {
                let end = self.b[self.i..].iter().position(|&x| x == b'>')?;
                self.i += end + 1;
                Some(Obj::Str)
            }
            b'(' => {
                let mut level = 0usize;
                while self.i < self.b.len() {
                    match self.b[self.i] {
                        b'\\' => self.i += 1,
                        b'(' => level += 1,
                        b')' => {
                            level -= 1;
                            if level == 0 {
                                self.i += 1;
                                return Some(Obj::Str);
                            }
                        }
                        _ => {}
                    }
                    self.i += 1;
                }
                None
            }
            b'[' => {
                self.i += 1;
                let mut v = Vec::new();
                loop {
                    self.skip();
                    if *self.b.get(self.i)? == b']' {
                        self.i += 1;
                        break;
                    }
                    if v.len() >= MAX_ENTRIES {
                        // Over the limit: the whole file is not readable, not only this value.
                        ELEMENTS.with(|e| e.set(0));
                        return None;
                    }
                    v.push(self.obj(depth + 1)?);
                }
                Some(Obj::Array(v))
            }
            _ => {
                let w = self.word();
                match w {
                    b"true" => Some(Obj::Bool(true)),
                    b"false" => Some(Obj::Bool(false)),
                    b"null" => Some(Obj::Null),
                    _ => {
                        let n: f64 = std::str::from_utf8(w).ok()?.parse().ok()?;
                        // `N G R` is a reference.
                        let keep = self.i;
                        if n.fract() == 0.0 && n >= 0.0 {
                            let g = self.peek_word();
                            if !g.is_empty() && g.iter().all(u8::is_ascii_digit) {
                                self.word();
                                if self.word() == b"R" {
                                    return Some(Obj::Ref(n as u32));
                                }
                            }
                        }
                        self.i = keep;
                        Some(Obj::Num(n))
                    }
                }
            }
        }
    }
}

struct Doc<'a> {
    b: &'a [u8],
    locs: HashMap<u32, Loc>,
    trailer: HashMap<String, Obj>,
    /// Decompressed object streams, by their object number.
    streams: std::cell::RefCell<HashMap<u32, std::rc::Rc<Vec<u8>>>>,
    /// Bytes inflated so far ([`MAX_INFLATE_TOTAL`]).
    inflated: std::cell::Cell<usize>,
    /// When the reading gives up ([`MAX_TIME`]).
    until: Instant,
    /// How deep `obj_at` is in itself: an indirect `/Length` in an object stream reads that stream, whose `/Length`
    /// may be indirect again (Sol 26.11.2026: a file can make that a loop and the stack overflows). At most
    /// [`MAX_LENGTH_HOPS`].
    hops: std::cell::Cell<u32>,
    /// Objects read so far ([`MAX_PARSES`]).
    parses: std::cell::Cell<usize>,
}

/// The most objects read (with repeats) from one file: a real manual reads a few hundred.
const MAX_PARSES: usize = 1_000_000;

/// The most streams an indirect `/Length` may lead through.
const MAX_LENGTH_HOPS: u32 = 4;

/// Flate with the PNG predictors of `/DecodeParms` (cross-reference streams use `/Predictor 12`).
fn inflate(raw: &[u8], parms: Option<&HashMap<String, Obj>>, room: usize) -> Option<Vec<u8>> {
    let mut out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(raw, room.min(MAX_INFLATE)).ok()?;
    let pred = parms.and_then(|p| p.get("Predictor")).and_then(Obj::num).unwrap_or(1.0) as u32;
    if pred >= 10 {
        let cols = parms.and_then(|p| p.get("Columns")).and_then(Obj::num).unwrap_or(1.0) as usize;
        if cols == 0 || cols > MAX_COLUMNS {
            return None;
        }
        let row = cols + 1;
        let mut prev = vec![0u8; cols];
        let mut res = Vec::with_capacity(out.len());
        for chunk in out.chunks(row) {
            if chunk.len() < row {
                break;
            }
            let (kind, data) = (chunk[0], &chunk[1..]);
            let mut line = data.to_vec();
            for x in 0..cols {
                let left = if x > 0 { line[x - 1] } else { 0 };
                let up = prev[x];
                let ul = if x > 0 { prev[x - 1] } else { 0 };
                line[x] = match kind {
                    0 => line[x],
                    1 => line[x].wrapping_add(left),
                    2 => line[x].wrapping_add(up),
                    3 => line[x].wrapping_add(((left as u16 + up as u16) / 2) as u8),
                    4 => {
                        let p = left as i16 + up as i16 - ul as i16;
                        let (pa, pb, pc) = ((p - left as i16).abs(), (p - up as i16).abs(), (p - ul as i16).abs());
                        let pr = if pa <= pb && pa <= pc { left } else if pb <= pc { up } else { ul };
                        line[x].wrapping_add(pr)
                    }
                    _ => return None,
                };
            }
            res.extend_from_slice(&line);
            prev = line;
        }
        out = res;
    }
    Some(out)
}

fn filters(d: &HashMap<String, Obj>) -> Vec<String> {
    match d.get("Filter") {
        Some(o @ Obj::Name(_)) => copy(o).and_then(|o| o.name().map(str::to_string)).into_iter().collect(),
        Some(Obj::Array(a)) => a.iter().filter_map(|o| copy(o).and_then(|o| o.name().map(str::to_string))).collect(),
        _ => Vec::new(),
    }
}

impl<'a> Doc<'a> {
    fn open(b: &'a [u8]) -> Option<Doc<'a>> {
        let mut doc = Doc { b, locs: HashMap::new(), trailer: HashMap::new(), streams: Default::default(), inflated: Default::default(), until: Instant::now() + MAX_TIME, hops: Default::default(), parses: Default::default() };
        if doc.read_xref_chain().is_none() || !doc.trailer.contains_key("Root") {
            doc.locs.clear();
            doc.trailer.clear();
            doc.scan();
        }
        // Encrypted: the pictures' bytes are not JPEGs as they are; over a limit: not read on.
        if doc.trailer.contains_key("Encrypt") || doc.locs.len() > MAX_OBJECTS || doc.late() {
            return None;
        }
        doc.trailer.contains_key("Root").then_some(doc)
    }

    /// Past [`MAX_TIME`]: every loop stops.
    fn late(&self) -> bool {
        Instant::now() > self.until
    }

    fn startxref(&self) -> Option<usize> {
        let tail = &self.b[self.b.len().saturating_sub(1024)..];
        let at = tail.windows(9).rposition(|w| w == b"startxref")?;
        let mut lx = Lexer::new(tail, at + 9);
        lx.uint().map(|n| n as usize)
    }

    fn read_xref_chain(&mut self) -> Option<()> {
        let mut at = Some(self.startxref()?);
        let mut seen = 0;
        while let Some(off) = at {
            seen += 1;
            if seen > 32 || off >= self.b.len() {
                return None;
            }
            let trailer = self.read_xref(off)?;
            at = trailer.get("Prev").and_then(Obj::num).map(|n| n as usize);
            for (k, v) in trailer {
                self.trailer.entry(k).or_insert(v);
            }
        }
        Some(())
    }

    /// One cross-reference section at `off`; newer sections were read first, so they win.
    fn read_xref(&mut self, off: usize) -> Option<HashMap<String, Obj>> {
        let mut lx = Lexer::new(self.b, off);
        if lx.peek_word() == b"xref" {
            lx.word();
            loop {
                if lx.peek_word() == b"trailer" {
                    lx.word();
                    // Taken as it is, not copied (a large trailer would be there twice).
                    return match lx.obj(0)? {
                        Obj::Dict(d) => Some(d),
                        _ => None,
                    };
                }
                let first = lx.uint()? as u32;
                let count = lx.uint()? as u32;
                if self.locs.len() > MAX_OBJECTS || self.late() {
                    return None;
                }
                for n in first..first.checked_add(count)? {
                    let o = lx.uint()? as usize;
                    lx.uint()?;
                    let kind = lx.word();
                    if kind == b"n" {
                        if self.locs.len() >= MAX_OBJECTS {
                            return None;
                        }
                        self.locs.entry(n).or_insert(Loc::At(o));
                    }
                }
            }
        }
        // A cross-reference stream: `N G obj << /Type /XRef … >> stream`.
        let (_, obj) = self.obj_at(off)?;
        let Obj::Stream(d, s, e) = obj else { return None };
        let data = self.stream_data(&d, s, e)?;
        let w: Vec<usize> = match d.get("W") {
            Some(Obj::Array(a)) => a.iter().map(|o| o.num().unwrap_or(0.0) as usize).collect(),
            _ => return None,
        };
        if w.len() != 3 || w.iter().any(|&x| x > 8) || w.iter().sum::<usize>() == 0 {
            return None;
        }
        let size = d.get("Size").and_then(Obj::num).unwrap_or(0.0) as u32;
        let index: Vec<u32> = match d.get("Index") {
            Some(Obj::Array(a)) => a.iter().map(|o| o.num().unwrap_or(0.0) as u32).collect(),
            _ => vec![0, size],
        };
        let row = w.iter().sum::<usize>();
        let field = |r: &[u8], from: usize, len: usize, default: u64| -> u64 {
            if len == 0 {
                return default;
            }
            r[from..from + len].iter().fold(0u64, |a, &x| (a << 8) | x as u64)
        };
        let mut rows = data.chunks(row);
        for pair in index.chunks(2) {
            let (first, count) = (pair[0], *pair.get(1)?);
            for n in first..first.checked_add(count)? {
                let Some(r) = rows.next() else { break };
                if r.len() < row {
                    break;
                }
                let kind = field(r, 0, w[0], 1);
                let a = field(r, w[0], w[1], 0);
                let b = field(r, w[0] + w[1], w[2], 0);
                let loc = match kind {
                    1 => Loc::At(a as usize),
                    2 => Loc::InStream(a as u32, b as u32),
                    _ => continue,
                };
                if self.locs.len() >= MAX_OBJECTS || self.late() {
                    return None;
                }
                self.locs.entry(n).or_insert(loc);
            }
        }
        Some(d)
    }

    /// No usable table: every `N G obj` at a line start, and the last `trailer` (or a cross-reference stream's dict).
    fn scan(&mut self) {
        let b = self.b;
        let mut i = 0;
        while let Some(p) = b[i..].windows(3).position(|w| w == b"obj") {
            if self.locs.len() > MAX_OBJECTS || self.late() {
                return;
            }
            let at = i + p;
            i = at + 3;
            // Back over `N G `.
            let mut s = at;
            while s > 0 && is_white(b[s - 1]) {
                s -= 1;
            }
            let ge = s;
            while s > 0 && b[s - 1].is_ascii_digit() {
                s -= 1;
            }
            if s == ge {
                continue;
            }
            while s > 0 && is_white(b[s - 1]) {
                s -= 1;
            }
            let ne = s;
            while s > 0 && b[s - 1].is_ascii_digit() {
                s -= 1;
            }
            if s == ne || (s > 0 && !is_white(b[s - 1])) {
                continue;
            }
            let Some(n) = std::str::from_utf8(&b[s..ne]).ok().and_then(|t| t.parse::<u32>().ok()) else { continue };
            self.locs.insert(n, Loc::At(s));
        }
        // Objects inside object streams.
        let streams: Vec<u32> = self.locs.keys().copied().collect();
        for n in streams {
            if self.late() {
                return;
            }
            let Some(Obj::Stream(d, ..)) = self.get(n) else { continue };
            match d.get("Type").and_then(Obj::name) {
                Some("ObjStm") => {
                    let count = d.get("N").and_then(Obj::num).unwrap_or(0.0) as u32;
                    if let Some(data) = self.objstm(n) {
                        let mut lx = Lexer::new(&data, 0);
                        for idx in 0..count {
                            let Some(m) = lx.uint() else { break };
                            lx.uint();
                            if self.locs.len() >= MAX_OBJECTS {
                                break;
                            }
                            self.locs.entry(m as u32).or_insert(Loc::InStream(n, idx));
                        }
                    }
                }
                Some("XRef") => {
                    for (k, v) in d {
                        self.trailer.entry(k).or_insert(v);
                    }
                }
                _ => {}
            }
        }
        if let Some(p) = b.windows(7).rposition(|w| w == b"trailer") {
            if let Some(Obj::Dict(d)) = Lexer::new(b, p + 7).obj(0) {
                for (k, v) in d {
                    self.trailer.insert(k, v);
                }
            }
        }
    }

    /// `N G obj <object> [stream … endstream]` at `off`.
    fn obj_at(&self, off: usize) -> Option<(u32, Obj)> {
        // Every object read counts; past the budget or the time, nothing more is read.
        self.parses.set(self.parses.get() + 1);
        if self.parses.get() > MAX_PARSES || self.late() {
            return None;
        }
        let mut lx = Lexer::new(self.b, off);
        let n = lx.uint()? as u32;
        lx.uint()?;
        if lx.word() != b"obj" {
            return None;
        }
        let o = lx.obj(0)?;
        let d = match o {
            Obj::Dict(d) => d,
            other => return Some((n, other)),
        };
        if lx.peek_word() != b"stream" {
            return Some((n, Obj::Dict(d)));
        }
            {
                lx.word();
                let mut s = lx.i;
                if self.b.get(s) == Some(&b'\r') {
                    s += 1;
                }
                if self.b.get(s) == Some(&b'\n') {
                    s += 1;
                }
                let len = match d.get("Length") {
                    Some(Obj::Num(l)) => Some(*l as usize),
                    Some(Obj::Ref(r)) if *r != n && self.hops.get() < MAX_LENGTH_HOPS => {
                        self.hops.set(self.hops.get() + 1);
                        let l = self.direct_num(*r).map(|l| l as usize);
                        self.hops.set(self.hops.get() - 1);
                        l
                    }
                    _ => None,
                };
                let e = match len {
                    Some(l) if s.checked_add(l).is_some_and(|e| e <= self.b.len()) => s + l,
                    // No usable length: up to `endstream`.
                    _ => s + self.b[s..].windows(9).position(|w| w == b"endstream")?,
                };
                // The dictionary moves into the stream, it is not copied (Sol 26.11.2026).
                Some((n, Obj::Stream(d, s, e)))
            }
    }

    /// A number object read straight from the file (an indirect `/Length`), without following streams.
    fn direct_num(&self, n: u32) -> Option<f64> {
        match self.locs.get(&n)? {
            Loc::At(off) => {
                let mut lx = Lexer::new(self.b, *off);
                lx.uint()?;
                lx.uint()?;
                (lx.word() == b"obj").then_some(())?;
                lx.obj(0)?.num()
            }
            Loc::InStream(..) => self.get(n)?.num(),
        }
    }

    fn stream_data(&self, d: &HashMap<String, Obj>, s: usize, e: usize) -> Option<Vec<u8>> {
        let raw = &self.b[s..e];
        match filters(d).as_slice() {
            // A plain stream's copy counts like an inflated one.
            [] if raw.len() <= MAX_INFLATE_TOTAL.saturating_sub(self.inflated.get()) && spend(0, raw.len()) => {
                self.inflated.set(self.inflated.get() + raw.len());
                Some(raw.to_vec())
            }
            [] => None,
            [f] if f == "FlateDecode" => {
                let parms = d.get("DecodeParms").and_then(Obj::dict);
                let room = MAX_INFLATE_TOTAL.saturating_sub(self.inflated.get());
                let out = inflate(raw, parms, room)?;
                self.inflated.set(self.inflated.get() + out.len());
                spend(0, out.len()).then_some(out)
            }
            _ => None,
        }
    }

    fn objstm(&self, n: u32) -> Option<std::rc::Rc<Vec<u8>>> {
        if let Some(d) = self.streams.borrow().get(&n) {
            return Some(d.clone());
        }
        let Loc::At(off) = *self.locs.get(&n)? else { return None };
        let (_, Obj::Stream(d, s, e)) = self.obj_at(off)? else { return None };
        let data = std::rc::Rc::new(self.stream_data(&d, s, e)?);
        self.streams.borrow_mut().insert(n, data.clone());
        Some(data)
    }

    fn get(&self, n: u32) -> Option<Obj> {
        match *self.locs.get(&n)? {
            Loc::At(off) => self.obj_at(off).map(|(_, o)| o),
            Loc::InStream(sn, idx) => {
                let Loc::At(off) = *self.locs.get(&sn)? else { return None };
                let (_, Obj::Stream(d, ..)) = self.obj_at(off)? else { return None };
                let first = d.get("First").and_then(Obj::num)? as usize;
                let data = self.objstm(sn)?;
                let mut lx = Lexer::new(&data, 0);
                let mut at = None;
                for i in 0..=idx {
                    let m = lx.uint()?;
                    let o = lx.uint()? as usize;
                    if i == idx && m as u32 == n {
                        at = Some(o);
                    }
                }
                let mut lx = Lexer::new(&data, first.checked_add(at?)?);
                lx.obj(0)
            }
        }
    }

    /// `o`, with a reference followed (once per step, at most a few steps).
    fn resolve(&self, o: &Obj) -> Option<Obj> {
        // A reference is read afresh (paid for as it is parsed); anything else is copied, paid for first.
        let mut cur = match o {
            Obj::Ref(n) => Obj::Ref(*n),
            other => copy(other)?,
        };
        for _ in 0..8 {
            match cur {
                Obj::Ref(n) => cur = self.get(n)?,
                other => return Some(other),
            }
        }
        None
    }

    /// The pages in reading order, each read as the tree is walked: the resources a page inherits are shared
    /// (`Rc`), never copied per page, and nothing of a page is kept but its [`Page`] (Sol 26.11.2026).
    fn page_list(&self) -> Vec<Page> {
        let mut out = Vec::new();
        let Some(root) = self.trailer.get("Root").and_then(|r| self.resolve(r)) else { return out };
        let Some(pages) = root.dict().and_then(|d| d.get("Pages")).and_then(copy) else { return out };
        let mut seen = std::collections::HashSet::new();
        self.walk(&pages, None, &mut out, &mut seen, 0);
        out
    }

    fn walk(&self, node: &Obj, inherited: Option<std::rc::Rc<HashMap<String, Obj>>>, out: &mut Vec<Page>, seen: &mut std::collections::HashSet<u32>, depth: u32) {
        if depth > 32 || out.len() >= MAX_PAGES || self.late() {
            return;
        }
        if let Obj::Ref(n) = node {
            if !seen.insert(*n) {
                return;
            }
        }
        let Some(o) = self.resolve(node) else { return };
        let Some(d) = (match o {
            Obj::Dict(d) | Obj::Stream(d, ..) => Some(d),
            _ => None,
        }) else {
            return;
        };
        let own = d.get("Resources").and_then(|r| self.resolve(r)).and_then(|r| match r {
            Obj::Dict(d) => Some(std::rc::Rc::new(d)),
            _ => None,
        });
        let res = own.or(inherited);
        match d.get("Type").and_then(Obj::name) {
            Some("Pages") | None if d.contains_key("Kids") => {
                if let Some(Obj::Array(kids)) = d.get("Kids").and_then(|k| self.resolve(k)) {
                    for k in &kids {
                        self.walk(k, res.clone(), out, seen, depth + 1);
                    }
                }
            }
            _ => out.push(self.page(res.as_deref())),
        }
    }

    /// The page's JPEG: its one image XObject in `DCTDecode`. More than one picture, or none: [`Page::Other`].
    fn page(&self, res: Option<&HashMap<String, Obj>>) -> Page {
        let Some(xo) = res.and_then(|r| r.get("XObject")).and_then(|x| self.resolve(x)) else { return Page::Other };
        let Some(xo) = xo.dict() else { return Page::Other };
        let mut images = Vec::new();
        for v in xo.values() {
            let Some(Obj::Stream(d, s, e)) = self.resolve(v) else { continue };
            if d.get("Subtype").and_then(Obj::name) == Some("Image") {
                images.push((d, s, e));
            }
        }
        let [(d, s, e)] = images.as_slice() else { return Page::Other };
        if filters(d).as_slice() != ["DCTDecode"] {
            return Page::Other;
        }
        let width = d.get("Width").and_then(Obj::num).unwrap_or(0.0) as u32;
        let height = d.get("Height").and_then(Obj::num).unwrap_or(0.0) as u32;
        // A JPEG starts with SOI (FF D8).
        if self.b.get(*s..*s + 2) != Some(&[0xFF, 0xD8][..]) || width == 0 || height == 0 {
            return Page::Other;
        }
        Page::Jpeg { start: *s, len: e - s, width, height }
    }
}

/// The pages of the PDF `bytes`, in reading order. `None`: not a PDF pocket-ui can read (no page tree found).
pub fn pages(bytes: &[u8]) -> Option<Vec<Page>> {
    if !bytes.starts_with(b"%PDF") {
        return None;
    }
    ELEMENTS.with(|e| e.set(MAX_ELEMENTS));
    BYTES.with(|b| b.set(0));
    DEADLINE.with(|d| d.set(Some(Instant::now() + MAX_TIME)));
    let doc = Doc::open(bytes)?;
    let pages = doc.page_list();
    // Empty, out of time or out of budget on the way: not read halfway, not readable.
    (!pages.is_empty() && !doc.late() && ELEMENTS.with(|e| e.get()) > 0).then_some(pages)
}

#[cfg(test)]
#[path = "manual_pdf_tests.rs"]
pub(crate) mod tests;
