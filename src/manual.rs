//! The manual on the device: the user guide (`docs/user/*.md`) bundled into the binary with `include_str!`,
//! so it always matches the build, read in Settings > Manual.
//!
//! * [`parse`] reads the small subset of Markdown the guide uses: headings, paragraphs, bullet and numbered
//!   lists (nested), block quotes, code blocks, rules and tables, with bold, italic, code and links inside text.
//!   HTML comments (the guide's TODO notes) are dropped, images show their alt text. Button names become
//!   chips (`Kind::Chip`): the long names (START, SELECT, HOME, L1 ...) wherever they stand in capitals,
//!   B, X and Y as capital letters on their own, and A (which is also the article) where it is certainly
//!   the button: in bold, after "press", in a table column headed "Button", at the start of a list item
//!   before a colon, inside a sentence (an article is never a capital there), and at the start of a
//!   sentence only before a verb of the button ("A switches it").
//! * [`layout`] sets a page in pocket-ui's own type and colours into a list of positioned items for a
//!   given width (words wrapped, chips inline, table columns fitted), once per page.
//! * [`Reader`] holds where the reader is (page, scroll per page, the speed of a held stick) and paints
//!   the visible part. It knows nothing about buttons: `main.rs` feeds it.

use std::time::Instant;

use crate::gfx::Canvas;
use crate::text::{Face, Text};

/// The guide, in reading order: the order of the "Pages" list on its first page (`index.md`). The names are the files
/// of `docs/user/`; every file there is either here or in `DESKTOP_ONLY` (checked by `doc_checks`, cargo test).
pub const PAGES: [(&str, &str); 34] = [
    ("index", include_str!("../docs/user/index.md")),
    ("getting-started", include_str!("../docs/user/getting-started.md")),
    ("coming-from", include_str!("../docs/user/coming-from.md")),
    ("playing", include_str!("../docs/user/playing.md")),
    ("pause-menu", include_str!("../docs/user/pause-menu.md")),
    ("options", include_str!("../docs/user/options.md")),
    ("options-big", include_str!("../docs/user/options-big.md")),
    ("controls", include_str!("../docs/user/controls.md")),
    ("controls-settings", include_str!("../docs/user/controls-settings.md")),
    ("settings", include_str!("../docs/user/settings.md")),
    ("settings-system", include_str!("../docs/user/settings-system.md")),
    ("quick-settings", include_str!("../docs/user/quick-settings.md")),
    ("storage", include_str!("../docs/user/storage.md")),
    ("bluetooth", include_str!("../docs/user/bluetooth.md")),
    ("language", include_str!("../docs/user/language.md")),
    ("night-light", include_str!("../docs/user/night-light.md")),
    ("theme", include_str!("../docs/user/theme.md")),
    ("performance", include_str!("../docs/user/performance.md")),
    ("tour", include_str!("../docs/user/tour.md")),
    ("library", include_str!("../docs/user/library.md")),
    ("curator", include_str!("../docs/user/curator.md")),
    ("achievements", include_str!("../docs/user/achievements.md")),
    ("systems", include_str!("../docs/user/systems.md")),
    ("arcade", include_str!("../docs/user/arcade.md")),
    ("patches-msu1", include_str!("../docs/user/patches-msu1.md")),
    ("extras", include_str!("../docs/user/extras.md")),
    ("sync", include_str!("../docs/user/sync.md")),
    ("remote", include_str!("../docs/user/remote.md")),
    ("web-companion", include_str!("../docs/user/web-companion.md")),
    ("pc-streaming", include_str!("../docs/user/pc-streaming.md")),
    ("updates", include_str!("../docs/user/updates.md")),
    ("safe-mode", include_str!("../docs/user/safe-mode.md")),
    ("hotel-wifi", include_str!("../docs/user/hotel-wifi.md")),
    ("troubleshooting", include_str!("../docs/user/troubleshooting.md")),
];

/// Pages of `docs/user/` that are not in the manual on the device, because they only make sense on a computer.
pub const DESKTOP_ONLY: &[&str] = &[];

/// The text of the page called `name` (a file of `docs/user/` without `.md`).
pub fn page(name: &str) -> Option<&'static str> {
    PAGES.iter().find(|(n, _)| *n == name).map(|(_, md)| *md)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Text,
    Code,
    Link,
    /// A button name, drawn as the chip of the hint bar.
    Chip,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading { level: u8, spans: Vec<Span> },
    Paragraph(Vec<Span>),
    Item { depth: u8, number: Option<u32>, spans: Vec<Span> },
    Quote(Vec<Span>),
    Code(Vec<String>),
    Table { header: Vec<Vec<Span>>, rows: Vec<Vec<Vec<Span>>> },
    Rule,
    /// A picture pocket-ui draws itself, from a line that is only `![what it shows](pocket-ui:<name>)`: `controls`
    /// is the Mini V2 with its buttons named (`controls_picture`). Elsewhere (GitHub) the line shows its words.
    Picture(String),
}

// ---- inline text ------------------------------------------------------------------------------------------

/// The long button names, which are buttons wherever they stand in capitals.
const LONG_BUTTONS: [&str; 9] = ["START", "SELECT", "HOME", "L1", "R1", "L2", "R2", "L3", "R3"];
const LETTER_BUTTONS: [&str; 4] = ["A", "B", "X", "Y"];

fn is_long_button(w: &str) -> bool {
    LONG_BUTTONS.contains(&w)
}

fn is_letter_button(w: &str) -> bool {
    LETTER_BUTTONS.contains(&w)
}

/// Where in a text a single letter may be a button.
#[derive(Clone, Copy, Default)]
struct Ctx {
    /// A table column headed "Button": single letters are buttons.
    letters: bool,
    /// The start of a list item: "A: type" begins with a button.
    lead: bool,
    /// Headings are plain.
    none: bool,
}

fn push_text(out: &mut Vec<Span>, text: &mut String, bold: bool, italic: bool) {
    if text.is_empty() {
        return;
    }
    let t = std::mem::take(text);
    match out.last_mut() {
        Some(last) if last.kind == Kind::Text && last.bold == bold && last.italic == italic => last.text.push_str(&t),
        _ => out.push(Span {
            text: t,
            bold,
            italic,
            kind: Kind::Text,
        }),
    }
}

/// `needle` in `chars` from `from`; the index it starts at.
fn find(chars: &[char], from: usize, needle: &str) -> Option<usize> {
    let n: Vec<char> = needle.chars().collect();
    (from..chars.len().saturating_sub(n.len() - 1)).find(|&i| chars[i..i + n.len()] == n[..])
}

fn inline(src: &str, bold: bool, italic: bool, out: &mut Vec<Span>) {
    let chars: Vec<char> = src.chars().collect();
    let (mut i, mut buf) = (0, String::new());
    let starts = |i: usize, s: &str| s.chars().enumerate().all(|(k, c)| chars.get(i + k) == Some(&c));
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && chars.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) {
            buf.push(chars[i + 1]);
            i += 2;
        } else if starts(i, "<!--") {
            i = find(&chars, i + 4, "-->").map_or(chars.len(), |e| e + 3);
        } else if c == '`' {
            match find(&chars, i + 1, "`") {
                Some(e) if e > i + 1 => {
                    push_text(out, &mut buf, bold, italic);
                    out.push(Span {
                        text: chars[i + 1..e].iter().collect(),
                        bold,
                        italic,
                        kind: Kind::Code,
                    });
                    i = e + 1;
                }
                _ => {
                    buf.push(c);
                    i += 1;
                }
            }
        } else if starts(i, "**") || starts(i, "__") {
            let marker: String = chars[i..i + 2].iter().collect();
            match find(&chars, i + 2, &marker) {
                Some(e) if e > i + 2 => {
                    push_text(out, &mut buf, bold, italic);
                    let inner: String = chars[i + 2..e].iter().collect();
                    inline(&inner, true, italic, out);
                    i = e + 2;
                }
                _ => {
                    buf.push_str(&marker);
                    i += 2;
                }
            }
        } else if c == '*' && chars.get(i + 1).is_some_and(|n| !n.is_whitespace() && *n != '*') {
            match find(&chars, i + 1, "*") {
                Some(e) if e > i + 1 && !chars[e - 1].is_whitespace() => {
                    push_text(out, &mut buf, bold, italic);
                    let inner: String = chars[i + 1..e].iter().collect();
                    inline(&inner, bold, true, out);
                    i = e + 1;
                }
                _ => {
                    buf.push(c);
                    i += 1;
                }
            }
        } else if c == '[' || (c == '!' && chars.get(i + 1) == Some(&'[')) {
            let image = c == '!';
            let open = i + image as usize;
            let close = find(&chars, open + 1, "](");
            let end = close.and_then(|cl| find(&chars, cl + 2, ")"));
            match (close, end) {
                (Some(cl), Some(e)) => {
                    push_text(out, &mut buf, bold, italic);
                    let inner: String = chars[open + 1..cl].iter().collect();
                    if image {
                        inline(&inner, bold, true, out);
                    } else {
                        let mut linked = Vec::new();
                        inline(&inner, bold, italic, &mut linked);
                        for mut s in linked {
                            if s.kind == Kind::Text {
                                s.kind = Kind::Link;
                            }
                            out.push(s);
                        }
                    }
                    i = e + 1;
                }
                _ => {
                    buf.push(c);
                    i += 1;
                }
            }
        } else if starts(i, "<http") {
            match find(&chars, i, ">") {
                Some(e) => {
                    push_text(out, &mut buf, bold, italic);
                    out.push(Span {
                        text: chars[i + 1..e].iter().collect(),
                        bold,
                        italic,
                        kind: Kind::Link,
                    });
                    i = e + 1;
                }
                None => {
                    buf.push(c);
                    i += 1;
                }
            }
        } else {
            // The arrow of "Settings → Battery" is not in the fonts; the single angle quote is.
            buf.push(if c == '\u{2192}' { '\u{203a}' } else { c });
            i += 1;
        }
    }
    push_text(out, &mut buf, bold, italic);
}

/// What the button A does in a sentence that starts with it ("A switches it"); the article is followed by a noun.
const A_VERBS: [&str; 18] = [
    "on", "again", "also", "switches", "sets", "keeps", "presses", "opens", "steps", "goes", "comes", "toggles", "does", "stays", "starts", "confirms",
    "plays", "to",
];

/// The word after position `i` (past blanks), lowercased, and whether a `,;:)` comes right after the run.
fn next_word(chars: &[char], i: usize) -> (String, bool) {
    let punct = chars.get(i).is_some_and(|c| matches!(c, ',' | ';' | ':' | ')'));
    let from = (i..chars.len()).find(|&k| !chars[k].is_whitespace()).unwrap_or(chars.len());
    let word: String = chars[from..].iter().take_while(|c| c.is_ascii_alphanumeric()).collect();
    (word.to_ascii_lowercase(), punct)
}

/// Splits a text span into runs of letters and digits and the rest, turning the button names into chips.
fn chipify(spans: Vec<Span>, ctx: Ctx) -> Vec<Span> {
    if ctx.none {
        return spans;
    }
    let mut out: Vec<Span> = Vec::new();
    let mut first = true;
    // At the start of a sentence a capital A is probably the article.
    let mut sentence_start = true;
    for s in spans {
        let at_start = std::mem::replace(&mut first, false);
        if s.kind != Kind::Text {
            sentence_start = false;
            out.push(s);
            continue;
        }
        // A whole bold span that is a single letter button.
        if s.bold && is_letter_button(s.text.trim()) {
            sentence_start = false;
            out.push(Span {
                text: s.text.trim().to_string(),
                bold: true,
                italic: s.italic,
                kind: Kind::Chip,
            });
            continue;
        }
        let chars: Vec<char> = s.text.chars().collect();
        let (mut i, mut plain) = (0, String::new());
        // The run before this one and where it ended: "press A" needs only blanks between them.
        let mut prev: Option<(String, usize)> = None;
        while i < chars.len() {
            if !chars[i].is_ascii_alphanumeric() {
                match chars[i] {
                    '.' | '!' | '?' | ':' => sentence_start = true,
                    c if c.is_whitespace() || matches!(c, '"' | '(' | '\'' | '\u{201c}') => {}
                    _ => sentence_start = false,
                }
                plain.push(chars[i]);
                i += 1;
                continue;
            }
            let start = i;
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                i += 1;
            }
            let run: String = chars[start..i].iter().collect();
            // Only a run that stands alone: not part of a word like "Wi-Fi" or "D-pad" or a file name.
            let before = start.checked_sub(1).map(|k| chars[k]);
            let after = chars.get(i).copied();
            let joined = before.is_some_and(|b| matches!(b, '-' | '_' | '.' | '\''))
                || after.is_some_and(|a| matches!(a, '-' | '_' | '\'') || (a == '.' && chars.get(i + 1).is_some_and(|n| n.is_ascii_alphanumeric())));
            let after_press = prev.as_ref().is_some_and(|(w, end)| {
                matches!(w.as_str(), "press" | "pressing" | "pressed" | "hold" | "tap") && start > *end && chars[*end..start].iter().all(|c| c.is_whitespace())
            });
            let lead = ctx.lead && at_start && start == 0 && after == Some(':');
            let letter = is_letter_button(&run) && {
                let (word, punct) = next_word(&chars, i);
                run != "A" || s.bold || ctx.letters || after_press || lead || !sentence_start || punct || A_VERBS.contains(&word.as_str())
            };
            let button = !joined && (is_long_button(&run) || letter);
            if button {
                push_text(&mut out, &mut plain, s.bold, s.italic);
                out.push(Span {
                    text: run.clone(),
                    bold: s.bold,
                    italic: s.italic,
                    kind: Kind::Chip,
                });
            } else {
                plain.push_str(&run);
            }
            sentence_start = false;
            prev = Some((run.to_ascii_lowercase(), i));
        }
        push_text(&mut out, &mut plain, s.bold, s.italic);
    }
    out
}

/// The spans of a line of inline Markdown.
pub fn spans(src: &str) -> Vec<Span> {
    let mut out = Vec::new();
    inline(src, false, false, &mut out);
    chipify(out, Ctx::default())
}

fn spans_in(src: &str, ctx: Ctx) -> Vec<Span> {
    let mut out = Vec::new();
    inline(src.trim(), false, false, &mut out);
    chipify(out, ctx)
}

// ---- blocks ---------------------------------------------------------------------------------------------------

/// Drops `<!-- ... -->`, also across lines.
fn strip_comments(md: &str) -> String {
    let (mut out, mut rest) = (String::new(), md);
    let mut fenced = false;
    // Comments inside a code fence are code; every other one goes.
    while let Some(at) = rest.find("<!--") {
        let before = &rest[..at];
        fenced ^= before.matches("```").count() % 2 == 1;
        if fenced {
            out.push_str(&rest[..at + 4]);
            rest = &rest[at + 4..];
            continue;
        }
        out.push_str(before);
        match rest[at..].find("-->") {
            Some(e) => rest = &rest[at + e + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn split_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').filter(|_| !t.ends_with("\\|")).unwrap_or(t);
    let (mut cells, mut cur, mut chars) = (Vec::new(), String::new(), t.chars().peekable());
    let mut in_code = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cur.push('|');
                chars.next();
            }
            '`' => {
                in_code = !in_code;
                cur.push(c);
            }
            '|' if !in_code => cells.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    cells.push(cur);
    cells
}

fn is_align_row(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.contains('-') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// `-`, `*`, `+` or `1.` at the start (after blanks): (indent, number, text after the marker).
fn list_marker(line: &str) -> Option<(usize, Option<u32>, &str)> {
    let indent = line.len() - line.trim_start().len();
    let t = line.trim_start();
    if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("+ ")) {
        return Some((indent, None, rest));
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits <= 3 {
        if let Some(rest) = t[digits..].strip_prefix(". ") {
            return Some((indent, t[..digits].parse().ok(), rest));
        }
    }
    None
}

fn flush_text(text: &mut Option<(Kind2, String)>, out: &mut Vec<Block>) {
    let Some((kind, t)) = text.take() else { return };
    out.push(match kind {
        Kind2::Paragraph => Block::Paragraph(spans_in(&t, Ctx::default())),
        Kind2::Quote => Block::Quote(spans_in(&t, Ctx::default())),
        Kind2::Item { depth, number } => Block::Item {
            depth,
            number,
            spans: spans_in(&t, Ctx { lead: true, ..Ctx::default() }),
        },
    });
}

#[derive(Clone, Copy)]
enum Kind2 {
    Paragraph,
    Quote,
    Item { depth: u8, number: Option<u32> },
}

fn join(t: &mut String, line: &str) {
    if !t.is_empty() {
        t.push(' ');
    }
    t.push_str(line.trim());
}

pub fn parse(md: &str) -> Vec<Block> {
    let md = strip_comments(md);
    let lines: Vec<&str> = md.lines().collect();
    let mut out = Vec::new();
    let mut cur: Option<(Kind2, String)> = None;
    // Indents of the open list levels, to give an item its depth.
    let mut levels: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();
        i += 1;
        if t.starts_with("```") {
            flush_text(&mut cur, &mut out);
            let mut code = Vec::new();
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code.push(lines[i].trim_end().replace('\u{2192}', "\u{203a}"));
                i += 1;
            }
            i += 1;
            out.push(Block::Code(code));
            continue;
        }
        if t.is_empty() {
            flush_text(&mut cur, &mut out);
            levels.clear();
            continue;
        }
        if let Some(h) = t.strip_prefix('#') {
            let level = 1 + h.chars().take_while(|c| *c == '#').count();
            let body = h.trim_start_matches('#');
            if level <= 6 && body.starts_with(' ') {
                flush_text(&mut cur, &mut out);
                levels.clear();
                out.push(Block::Heading {
                    level: level as u8,
                    spans: spans_in(body, Ctx { none: true, ..Ctx::default() }),
                });
                continue;
            }
        }
        if matches!(t, "---" | "***" | "___") {
            flush_text(&mut cur, &mut out);
            out.push(Block::Rule);
            continue;
        }
        if let Some(name) = t.strip_prefix("![").and_then(|r| r.split_once("](pocket-ui:")).and_then(|(_, r)| r.strip_suffix(')')) {
            flush_text(&mut cur, &mut out);
            levels.clear();
            out.push(Block::Picture(name.to_string()));
            continue;
        }
        if t.starts_with('|') && lines.get(i).is_some_and(|l| is_align_row(l)) {
            flush_text(&mut cur, &mut out);
            levels.clear();
            let head = split_row(t);
            let letters: Vec<bool> = head
                .iter()
                .map(|h| matches!(h.trim().to_ascii_lowercase().as_str(), "button" | "buttons"))
                .collect();
            i += 1;
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('|') {
                let mut cells = split_row(lines[i]);
                cells.resize(head.len(), String::new());
                rows.push(
                    cells
                        .iter()
                        .enumerate()
                        .map(|(c, s)| {
                            spans_in(
                                s,
                                Ctx {
                                    letters: letters[c],
                                    ..Ctx::default()
                                },
                            )
                        })
                        .collect(),
                );
                i += 1;
            }
            out.push(Block::Table {
                header: head.iter().map(|h| spans_in(h, Ctx { none: true, ..Ctx::default() })).collect(),
                rows,
            });
            continue;
        }
        if let Some(q) = t.strip_prefix('>') {
            let q = q.trim();
            if !matches!(cur, Some((Kind2::Quote, _))) {
                flush_text(&mut cur, &mut out);
                cur = Some((Kind2::Quote, String::new()));
            }
            if q.is_empty() {
                flush_text(&mut cur, &mut out);
            } else {
                join(&mut cur.get_or_insert((Kind2::Quote, String::new())).1, q);
            }
            continue;
        }
        if let Some((indent, number, rest)) = list_marker(line) {
            flush_text(&mut cur, &mut out);
            while levels.last().is_some_and(|l| *l > indent) {
                levels.pop();
            }
            if levels.last() != Some(&indent) {
                levels.push(indent);
            }
            let depth = (levels.len() - 1) as u8;
            cur = Some((Kind2::Item { depth, number }, rest.trim().to_string()));
            continue;
        }
        // Anything else continues the paragraph, item or quote before it, or starts a paragraph.
        match cur.as_mut() {
            Some((_, text)) => join(text, t),
            None => cur = Some((Kind2::Paragraph, t.to_string())),
        }
    }
    flush_text(&mut cur, &mut out);
    out
}

/// The first heading: the page's name.
pub fn title(md: &str) -> String {
    parse(md)
        .iter()
        .find_map(|b| match b {
            Block::Heading { spans, .. } => Some(spans.iter().map(|s| s.text.as_str()).collect::<String>()),
            _ => None,
        })
        .unwrap_or_default()
}

// ---- layout ----------------------------------------------------------------------------------------------------

/// Something placed on the page. `y` is the baseline for text and the top for the rest; all relative to the
/// top of the page.
pub enum Item {
    Text {
        x: i32,
        y: i32,
        px: u32,
        face: Face,
        color: u32,
        text: String,
    },
    Chip {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        px: u32,
        label: String,
    },
    Rect {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: u32,
        radius: i32,
    },
    /// The Controls picture (`controls_picture`), `k` times its pixels, in a box `w` wide and `h` high.
    Controls {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        k: i32,
    },
}

impl Item {
    fn span(&self) -> (i32, i32) {
        match self {
            Item::Text { y, px, .. } => (y - sz(*px) as i32, y + sz(*px) as i32 / 3),
            Item::Chip { y, h, .. } | Item::Rect { y, h, .. } | Item::Controls { y, h, .. } => (*y, y + h),
        }
    }
}

#[derive(Default)]
pub struct Laid {
    pub height: i32,
    pub items: Vec<Item>,
}

const BODY: u32 = 34;
const SMALL: u32 = 30;
// The page's colours in the appearance in use (dark, light); chips are the hint bar's (`nu_hints`), a grey
// pill with letters in the pill colour. The layout keeps the colours, so it is made again when they change.
fn text_color() -> u32 {
    crate::palette::pal().pick(0xc9c6be, 0x45423c)
}
fn bold_color() -> u32 {
    crate::palette::pal().pick(0xffffff, 0x1a1916)
}
fn heading_color() -> u32 {
    crate::palette::pal().pick(0xe9e6de, 0x24221e)
}
fn code_color() -> u32 {
    crate::palette::pal().pick(0xe8d9a0, 0x6e5208)
}
// Links are the page's brightest text with their underline, not a web blue (docs/DESIGN.md "Colour roles": colour
// that means something comes from content).
fn link_color() -> u32 {
    crate::palette::pal().pick(0xf2efe8, 0x1a1916)
}
fn grid_color() -> u32 {
    crate::palette::pal().pick(0x2c2c30, 0xd6d0c4)
}

#[derive(Clone)]
enum Piece {
    Word {
        text: String,
        face: Face,
        px: u32,
        color: u32,
        kind: Kind,
    },
    Chip {
        label: String,
        px: u32,
    },
}

struct Tok {
    piece: Piece,
    w: i32,
    /// A blank before it (unless it starts a line).
    space: bool,
}

#[derive(Clone, Copy)]
struct Style {
    face: Face,
    px: u32,
    color: u32,
    /// What bold text turns into: the same face, brighter.
    strong: u32,
}

/// The size a design size `px` is painted at (Settings → Text size, `type_scale`): the layout's line heights, chips
/// and code boxes follow the type that `Text` paints.
fn sz(px: u32) -> f32 {
    crate::type_scale::px(px) as f32
}

fn chip_size(text: &mut Text, px: u32, label: &str) -> (i32, i32) {
    let h = (sz(px) * 1.22) as i32;
    (h.max(text.width(Face::Body, (px as f32 * 0.8) as u32, label) + 24), h)
}

fn tokens(spans: &[Span], st: Style, text: &mut Text) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut pending = false;
    for s in spans {
        if s.kind == Kind::Chip {
            let (w, _) = chip_size(text, st.px, &s.text);
            out.push(Tok {
                piece: Piece::Chip {
                    label: s.text.clone(),
                    px: st.px,
                },
                w,
                space: pending && !out.is_empty(),
            });
            pending = false;
            continue;
        }
        let color = match s.kind {
            Kind::Code => code_color(),
            Kind::Link => link_color(),
            _ if s.bold => st.strong,
            _ if s.italic => mix_up(st.color),
            _ => st.color,
        };
        if s.text.starts_with(char::is_whitespace) && !out.is_empty() {
            pending = true;
        }
        for word in s.text.split_whitespace() {
            let w = text.width(st.face, st.px, word);
            out.push(Tok {
                piece: Piece::Word {
                    text: word.to_string(),
                    face: st.face,
                    px: st.px,
                    color,
                    kind: s.kind,
                },
                w,
                space: pending && !out.is_empty(),
            });
            pending = true;
        }
        pending = s.text.ends_with(char::is_whitespace) || (pending && !s.text.is_empty() && s.text.trim().is_empty());
    }
    out
}

fn mix_up(color: u32) -> u32 {
    crate::gfx::mix(color, crate::palette::pal().pick(0xffffff, 0x000000), 70)
}

/// Words broken into pieces that fit `max` pixels.
fn break_long(tok: Tok, max: i32, text: &mut Text) -> Vec<Tok> {
    if tok.w <= max {
        return vec![tok];
    }
    let Piece::Word {
        text: word,
        face,
        px,
        color,
        kind,
    } = tok.piece.clone()
    else {
        return vec![tok];
    };
    let (mut out, mut cur) = (Vec::new(), String::new());
    for ch in word.chars() {
        let mut next = cur.clone();
        next.push(ch);
        if text.width(face, px, &next) > max && !cur.is_empty() {
            let w = text.width(face, px, &cur);
            out.push(Tok {
                piece: Piece::Word {
                    text: std::mem::take(&mut cur),
                    face,
                    px,
                    color,
                    kind,
                },
                w,
                space: false,
            });
        }
        cur.push(ch);
    }
    let w = text.width(face, px, &cur);
    out.push(Tok {
        piece: Piece::Word {
            text: cur,
            face,
            px,
            color,
            kind,
        },
        w,
        space: false,
    });
    if let Some(first) = out.first_mut() {
        first.space = tok.space;
    }
    out
}

/// Tokens broken into lines of at most `max` pixels: (x offset, token). Tokens with no blank between them
/// ("(" and the code after it) are one unit and move to the next line together.
fn flow(toks: Vec<Tok>, max: i32, st: Style, text: &mut Text) -> Vec<Vec<(i32, Tok)>> {
    let space_w = text.width(st.face, st.px, " ");
    let toks: Vec<Tok> = toks.into_iter().flat_map(|t| break_long(t, max, text)).collect();
    let mut units: Vec<Vec<Tok>> = Vec::new();
    for t in toks {
        match units.last_mut() {
            Some(unit) if !t.space => unit.push(t),
            _ => units.push(vec![t]),
        }
    }
    // A unit wider than the line (a long path in code) may be broken after all.
    let units: Vec<Vec<Tok>> = units
        .into_iter()
        .flat_map(|u| {
            if u.iter().map(|t| t.w).sum::<i32>() > max {
                u.into_iter().map(|t| vec![t]).collect()
            } else {
                vec![u]
            }
        })
        .collect();
    let (mut lines, mut line, mut x): (Vec<Vec<(i32, Tok)>>, Vec<(i32, Tok)>, i32) = (Vec::new(), Vec::new(), 0);
    for unit in units {
        let width: i32 = unit.iter().map(|t| t.w).sum();
        let sp = if line.is_empty() || !unit[0].space { 0 } else { space_w };
        let wrap = !line.is_empty() && x + sp + width > max;
        if wrap {
            lines.push(std::mem::take(&mut line));
            x = 0;
        }
        let mut at = if wrap || line.is_empty() { 0 } else { x + sp };
        for t in unit {
            let w = t.w;
            line.push((at, t));
            at += w;
        }
        x = at;
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn line_height(px: u32) -> i32 {
    (sz(px) * 1.46).round() as i32
}

/// Puts the lines at (x0, y); the new bottom.
fn emit(lines: Vec<Vec<(i32, Tok)>>, x0: i32, mut y: i32, px: u32, text: &mut Text, items: &mut Vec<Item>) -> i32 {
    let lh = line_height(px);
    for line in lines {
        let base = y + (lh + (sz(px) * 0.7) as i32) / 2;
        for (dx, t) in line {
            let x = x0 + dx;
            match t.piece {
                Piece::Chip { label, px } => {
                    let (w, h) = chip_size(text, px, &label);
                    let centre = base - (sz(px) * 0.35) as i32;
                    items.push(Item::Chip {
                        x,
                        y: centre - h / 2,
                        w,
                        h,
                        px,
                        label,
                    });
                }
                Piece::Word {
                    text: word,
                    face,
                    px,
                    color,
                    kind,
                } => {
                    match kind {
                        Kind::Code => items.push(Item::Rect {
                            x: x - 6,
                            y: base - sz(px) as i32 + 2,
                            w: t.w + 12,
                            h: sz(px) as i32 + 8,
                            color: crate::palette::pal().pick(0x1f1f23, 0xe7e1d4),
                            radius: 8,
                        }),
                        Kind::Link => items.push(Item::Rect {
                            x,
                            y: base + 5,
                            w: t.w,
                            h: 2,
                            color: link_color(),
                            radius: 0,
                        }),
                        _ => {}
                    }
                    items.push(Item::Text {
                        x,
                        y: base,
                        px,
                        face,
                        color,
                        text: word,
                    });
                }
            }
        }
        y += lh;
    }
    y
}

/// Sets a table into `width` pixels: columns sized to their content when it fits, else the long ones wrap.
fn table(header: &[Vec<Span>], rows: &[Vec<Vec<Span>>], y: i32, width: i32, text: &mut Text, items: &mut Vec<Item>) -> i32 {
    let st = Style {
        face: Face::Body,
        px: SMALL,
        color: text_color(),
        strong: bold_color(),
    };
    let head_st = Style { color: bold_color(), ..st };
    let (pad_x, pad_y) = (16, 12);
    let n = header.len().max(1);
    let toks: Vec<Vec<Vec<Tok>>> = std::iter::once((header, head_st))
        .chain(rows.iter().map(|r| (&r[..], st)))
        .map(|(cells, style)| cells.iter().map(|c| tokens(c, style, text)).collect())
        .collect();
    let space_w = text.width(Face::Body, SMALL, " ");
    let (mut natural, mut min) = (vec![0i32; n], vec![0i32; n]);
    for row in &toks {
        for (c, cell) in row.iter().enumerate() {
            let one: i32 = cell.iter().map(|t| t.w + if t.space { space_w } else { 0 }).sum();
            natural[c] = natural[c].max(one);
            min[c] = min[c].max(cell.iter().map(|t| t.w).max().unwrap_or(0));
        }
    }
    let avail = width - 2 * pad_x * n as i32;
    if n > 1 {
        for c in 0..n {
            natural[c] = natural[c].min((avail * 36 / 100).max(min[c]));
        }
    }
    let widths: Vec<i32> = if natural.iter().sum::<i32>() <= avail {
        let mut w = natural.clone();
        w[n - 1] += avail - natural.iter().sum::<i32>();
        w
    } else {
        // The narrow columns keep the width their text wants; what is left goes to the wide ones, evenly.
        let mut widths = min.clone();
        let mut spare = (avail - min.iter().sum::<i32>()).max(0);
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&c| natural[c] - min[c]);
        for (k, c) in order.iter().enumerate() {
            let give = (natural[*c] - min[*c]).min(spare / (n - k) as i32);
            widths[*c] += give;
            spare -= give;
        }
        widths
    };
    let col_x: Vec<i32> = (0..n).map(|c| widths[..c].iter().sum::<i32>() + 2 * pad_x * c as i32).collect();
    let mut y = y;
    for (r, row) in toks.into_iter().enumerate() {
        let laid: Vec<Vec<Vec<(i32, Tok)>>> = row.into_iter().enumerate().map(|(c, cell)| flow(cell, widths[c].max(40), st, text)).collect();
        let lines = laid.iter().map(Vec::len).max().unwrap_or(1).max(1) as i32;
        let h = lines * line_height(SMALL) + 2 * pad_y;
        if r == 0 {
            items.push(Item::Rect {
                x: 0,
                y,
                w: width,
                h,
                color: crate::palette::pal().pick(0x1d1d21, 0xe7e1d4),
                radius: 10,
            });
        } else {
            items.push(Item::Rect {
                x: 0,
                y,
                w: width,
                h: 2,
                color: grid_color(),
                radius: 0,
            });
        }
        for (c, cell) in laid.into_iter().enumerate() {
            emit(cell, col_x[c] + pad_x, y + pad_y, SMALL, text, items);
        }
        y += h;
    }
    items.push(Item::Rect {
        x: 0,
        y,
        w: width,
        h: 2,
        color: grid_color(),
        radius: 0,
    });
    y + 2
}

/// Sets the page into `width` pixels.
pub fn layout(blocks: &[Block], text: &mut Text, width: i32) -> Laid {
    let mut items = Vec::new();
    let mut y = 0;
    for (i, b) in blocks.iter().enumerate() {
        let next = blocks.get(i + 1);
        let first = i == 0;
        match b {
            Block::Heading { level, spans } => {
                let (face, px, before, after) = match level {
                    1 => (Face::Title, 64, 0, 18),
                    2 => (Face::Title, 48, 44, 12),
                    _ => (Face::Title, 40, 30, 8),
                };
                let st = Style {
                    face,
                    px,
                    color: if *level == 1 { bold_color() } else { heading_color() },
                    strong: bold_color(),
                };
                y += if first { 0 } else { before };
                let lines = flow(tokens(spans, st, text), width, st, text);
                y = emit(lines, 0, y, px, text, &mut items) + after;
            }
            Block::Paragraph(spans) => {
                let st = Style {
                    face: Face::Body,
                    px: BODY,
                    color: text_color(),
                    strong: bold_color(),
                };
                let lines = flow(tokens(spans, st, text), width, st, text);
                y = emit(lines, 0, y, BODY, text, &mut items) + 20;
            }
            Block::Item { depth, number, spans } => {
                let st = Style {
                    face: Face::Body,
                    px: BODY,
                    color: text_color(),
                    strong: bold_color(),
                };
                let indent = 8 + *depth as i32 * 44;
                let marker = number.map_or("•".to_string(), |n| format!("{n}."));
                let marker_w = text.width(Face::Body, BODY, &marker);
                let body_x = indent + 44;
                let lines = flow(tokens(spans, st, text), width - body_x, st, text);
                let lh = line_height(BODY);
                let base = y + (lh + (BODY as f32 * 0.7) as i32) / 2;
                items.push(Item::Text {
                    x: indent + 30 - marker_w,
                    y: base,
                    px: BODY,
                    face: Face::Body,
                    color: if number.is_some() { bold_color() } else { crate::palette::pal().pick(0x8a877f, 0x7a766d) },
                    text: marker,
                });
                y = emit(lines, body_x, y, BODY, text, &mut items) + if matches!(next, Some(Block::Item { .. })) { 8 } else { 22 };
            }
            Block::Quote(spans) => {
                let st = Style {
                    face: Face::Body,
                    px: 32,
                    color: crate::palette::pal().pick(0xe3e0d8, 0x302e29),
                    strong: bold_color(),
                };
                let lines = flow(tokens(spans, st, text), width - 44, st, text);
                let top = y;
                let end = emit(lines, 36, y + 6, 32, text, &mut items) + 6;
                items.push(Item::Rect {
                    x: 0,
                    y: top,
                    w: 6,
                    h: end - top,
                    color: crate::palette::pal().pick(0x6fbf8a, 0x3f8f5a),
                    radius: 3,
                });
                y = end + 22;
            }
            Block::Code(lines) => {
                let st = Style {
                    face: Face::Body,
                    px: 28,
                    color: code_color(),
                    strong: code_color(),
                };
                let mut all = Vec::new();
                for l in lines {
                    // Kept as written: one token per line, broken only where it can't fit.
                    let toks = vec![Tok {
                        w: text.width(Face::Body, 28, l),
                        piece: Piece::Word {
                            text: l.clone(),
                            face: Face::Body,
                            px: 28,
                            color: code_color(),
                            kind: Kind::Text,
                        },
                        space: false,
                    }];
                    let flowed = flow(toks, width - 40, st, text);
                    all.extend(if flowed.is_empty() { vec![Vec::new()] } else { flowed });
                }
                let h = all.len() as i32 * line_height(28) + 28;
                items.push(Item::Rect {
                    x: 0,
                    y,
                    w: width,
                    h,
                    color: crate::palette::pal().pick(0x17171a, 0xebe6da),
                    radius: 14,
                });
                emit(all, 20, y + 14, 28, text, &mut items);
                y += h + 22;
            }
            Block::Table { header, rows } => {
                y = table(header, rows, y + 4, width, text, &mut items) + 26;
            }
            Block::Rule => {
                items.push(Item::Rect {
                    x: 0,
                    y: y + 10,
                    w: width,
                    h: 2,
                    color: grid_color(),
                    radius: 0,
                });
                y += 34;
            }
            Block::Picture(name) if name == "controls" => {
                if let Some(k) = controls_scale(text, width) {
                    let h = controls_height(k);
                    items.push(Item::Controls { x: 0, y: y + 4, w: width, h, k });
                    y += h + 22;
                }
            }
            Block::Picture(_) => {}
        }
    }
    Laid { height: y, items }
}

/// Draws the part of `laid` that lies in the band (top .. top + view_h), its top-left at (x, top); what
/// is cut off by the band stays as it was.
pub fn paint(laid: &Laid, text: &mut Text, c: &mut Canvas, x: i32, top: i32, scroll: i32, view_h: i32) {
    // Items are drawn whole; whatever they put above or below the band is put back afterwards.
    let (w, margin) = (c.w as usize, 120);
    let above = (top - margin).max(0) as usize..top.clamp(0, c.h) as usize;
    let below = (top + view_h).clamp(0, c.h) as usize..(top + view_h + margin).clamp(0, c.h) as usize;
    let saved: Vec<Vec<u32>> = [above.clone(), below.clone()].iter().map(|r| c.buf[r.start * w..r.end * w].to_vec()).collect();
    let mark = crate::text::boxes::mark();
    let (lo, hi) = (scroll - 60, scroll + view_h + 60);
    for item in &laid.items {
        let (a, b) = item.span();
        if b < lo || a > hi {
            continue;
        }
        let oy = top - scroll;
        match item {
            Item::Text {
                x: ix,
                y,
                px,
                face,
                color,
                text: t,
            } => {
                text.draw(c, *face, *px, x + ix, y + oy, *color, t);
            }
            Item::Chip { x: ix, y, w, h, px, label } => {
                // As the hint bars show the key (`kit::key_cap`, Button symbols).
                let small = (*px as f32 * 0.8) as u32;
                let p = crate::palette::pal();
                crate::kit::key_cap(c, text, (x + ix, y + oy, *w, *h), label, small, y + oy + h / 2 + (sz(small) * 0.35) as i32, (p.chip, p.pill));
            }
            Item::Controls { x: ix, y, w, k, .. } => controls_picture(c, text, x + ix, y + oy, *w, *k),
            Item::Rect { x: ix, y, w, h, color, radius } => {
                // Whole rectangles keep their corners; one cut by the band is clipped to it (it can be tall).
                let (y0, y1) = ((y + oy).max(top), (y + oy + h).min(top + view_h));
                if *radius > 0 && y0 == y + oy && y1 == y + oy + h {
                    c.round_rect(x + ix, y0, *w, *h, *radius, *color);
                } else if y1 > y0 {
                    c.rect(x + ix, y0, *w, y1 - y0, *color);
                }
            }
        }
    }
    for (r, rows) in [above, below].into_iter().zip(saved) {
        c.buf[r.start * w..r.end * w].copy_from_slice(&rows);
    }
    // For the snapshot checks of Settings → Text size: what was put back is not on the picture.
    crate::text::boxes::clip_since(mark, c, top, top + view_h);
}

// ---- the Controls picture ----------------------------------------------------------------------------------------

/// The pictures a line `![…](pocket-ui:<name>)` can ask for (`Block::Picture`; `doc_checks` refuses any other name).
pub const PICTURES: &[&str] = &["controls"];

/// The words beside the Controls picture: (what, the control it points at, left of the picture). Button names are
/// chips as in the text, as printed on the Pocket; the other words are in the interface's language, like the manual's
/// title line and buttons (its pages stay English for now).
const CONTROLS_WORDS: [(&str, (i32, i32), bool); 3] = [
    ("D-pad", crate::ui_art::anchors::DPAD, true),
    ("Left stick", crate::ui_art::anchors::LSTICK, true),
    ("Right stick", crate::ui_art::anchors::RSTICK, false),
];
const CONTROLS_CHIPS: [(&str, (i32, i32), bool); 4] = [
    ("SELECT", crate::ui_art::anchors::SELECT, true),
    ("HOME", crate::ui_art::anchors::HOME, true),
    ("START", crate::ui_art::anchors::START, false),
    ("BACK", crate::ui_art::anchors::BACK, false),
];
/// Above the picture, over the shoulders.
const CONTROLS_TOP: i32 = 52;

/// The largest whole-number size of the Mini V2 that leaves room for the words on both sides in `width`.
fn controls_scale(text: &mut Text, width: i32) -> Option<i32> {
    let art = crate::ui_art::Art::Device(crate::device::Model::MiniV2);
    let (w1, _) = crate::ui_art::size(art, 1);
    // The words are in the interface's language, as the manual's buttons are (its pages stay English).
    let was = text.translating();
    text.set_translate(true);
    let words = CONTROLS_WORDS.iter().map(|(t, ..)| text.width(Face::Body, SMALL, t)).max().unwrap_or(0);
    text.set_translate(was);
    let chips = CONTROLS_CHIPS.iter().map(|(t, ..)| chip_w(text, t)).max().unwrap_or(0);
    let side = words.max(chips) + 56;
    // Twice its pixels at most: the Controls page is long already (`every_bundled_page_lays_out_inside_the_screen`).
    (1..=2).rev().find(|k| w1 * k + 2 * side <= width && w1 > 0)
}

fn controls_height(k: i32) -> i32 {
    CONTROLS_TOP + crate::ui_art::size(crate::ui_art::Art::Device(crate::device::Model::MiniV2), k).1
}

fn chip_w(text: &mut Text, label: &str) -> i32 {
    text.width(Face::Body, (SMALL as f32 * 0.8) as u32, label) + 28
}

/// A line 2 px thick from `a` to `b`.
fn slant(c: &mut Canvas, a: (i32, i32), b: (i32, i32), color: u32) {
    let n = (b.0 - a.0).abs().max((b.1 - a.1).abs()).max(1);
    for i in 0..=n {
        c.rect(a.0 + (b.0 - a.0) * i / n - 1, a.1 + (b.1 - a.1) * i / n - 1, 2, 2, color);
    }
}

/// A chip as the text has them, its middle at `cy`; returns its width.
fn chip(c: &mut Canvas, text: &mut Text, x: i32, cy: i32, label: &str) -> i32 {
    let (w, h, small) = (chip_w(text, label), line_height(SMALL) - 6, (SMALL as f32 * 0.8) as u32);
    let p = crate::palette::pal();
    crate::kit::key_cap(c, text, (x, cy - h / 2, w, h), label, small, cy + (sz(small) * 0.35) as i32, (p.chip, p.pill));
    w
}

/// The Mini V2 `k` times its pixels in the middle of a box `w` wide at (x, y), every control named beside it with a thin
/// line to it, the face buttons lettered as the button layout in use has them (Settings › Playing › Button layout).
fn controls_picture(c: &mut Canvas, text: &mut Text, x: i32, y: i32, w: i32, k: i32) {
    use crate::pad::Button;
    use crate::ui_art::anchors;
    let art = crate::ui_art::Art::Device(crate::device::Model::MiniV2);
    let (aw, _) = crate::ui_art::size(art, k);
    let (ax, ay) = (x + (w - aw) / 2, y + CONTROLS_TOP);
    crate::ui_art::paint(c, art, ax, ay, k);
    let at = |(px, py): (i32, i32)| (ax + px * k + k / 2, ay + py * k + k / 2);
    let line = grid_color();
    // Each side's names top to bottom, at least a line apart (Large text's chips are taller than the controls are far
    // apart); a thin line runs from the control to its name, slanting where the name had to move down.
    let gap = line_height(SMALL) + 4;
    let ink = text_color();
    let was = text.translating();
    for left in [true, false] {
        let mut names: Vec<(&str, (i32, i32), bool)> = CONTROLS_WORDS
            .iter()
            .map(|(t, a, l)| (*t, *a, *l, false))
            .chain(CONTROLS_CHIPS.iter().map(|(t, a, l)| (*t, *a, *l, true)))
            .filter(|n| n.2 == left)
            .map(|(t, a, _, chip)| (t, a, chip))
            .collect();
        names.sort_by_key(|n| n.1 .1);
        let mut last = i32::MIN / 2;
        for (label, anchor, is_chip) in names {
            let (px, py) = at(anchor);
            let ly = py.max(last + gap);
            last = ly;
            let edge = if left { ax - 16 } else { ax + aw + 16 };
            slant(c, (px, py), (edge, ly), line);
            c.disc(px, py, 2 + k / 2, line);
            if is_chip {
                let cw = chip_w(text, label);
                chip(c, text, if left { ax - 24 - cw } else { ax + aw + 24 }, ly, label);
            } else {
                text.set_translate(true);
                let tw = text.width(Face::Body, SMALL, label);
                text.draw(c, Face::Body, SMALL, if left { ax - 24 - tw } else { ax + aw + 24 }, ly + (sz(SMALL) * 0.35) as i32, ink, label);
                text.set_translate(was);
            }
        }
    }
    // The shoulders: L1 and L2 over the left one, R1 and R2 over the right one, a short line down to it.
    for (labels, anchor) in [(["L1", "L2"], anchors::L), (["R1", "R2"], anchors::R)] {
        let p = at(anchor);
        let total = chip_w(text, labels[0]) + 10 + chip_w(text, labels[1]);
        let mut cx = p.0 - total / 2;
        let cy = y + line_height(SMALL) / 2;
        c.rect(p.0 - 1, cy + line_height(SMALL) / 2, 2, p.1 - cy - line_height(SMALL) / 2, line);
        for l in labels {
            cx += chip(c, text, cx, cy, l) + 10;
        }
    }
    // The face buttons' letters, on the buttons, as the menus read them in the layout in use.
    let layout = crate::button_layout::get();
    for (phys, anchor) in [(Button::North, anchors::NORTH), (Button::East, anchors::EAST), (Button::South, anchors::SOUTH), (Button::West, anchors::WEST)] {
        let letter = match layout.role(phys) {
            Button::East => "A",
            Button::South => "B",
            Button::North => "X",
            _ => "Y",
        };
        let (px, py) = at(anchor);
        let px_size = (4 * k + 6).clamp(14, 24) as u32;
        // In the Button symbols in use (`button_prompts`): the letter in its Xbox colour, lightened to read on the
        // dark button, or the PlayStation shape.
        let ink = crate::button_prompts::colours(letter).map_or(0xe9e6de, |(cap, _)| crate::gfx::mix(cap, 0xffffff, 90));
        let letter = crate::button_prompts::shown(letter);
        let lw = text.width(Face::Title, px_size, letter);
        text.draw(c, Face::Title, px_size, px - lw / 2, py + (sz(px_size) * 0.36) as i32, ink, letter);
    }
}

// ---- reading ----------------------------------------------------------------------------------------------------

/// Reading position: which page, how far down each page, and the speed a held stick or button scrolls at.
/// What a page was laid out for: the width and the text size.
type LaidFor = (i32, crate::type_scale::TextSize);

pub struct Reader {
    pub page: usize,
    scroll: Vec<i32>,
    /// Per page: the width and the text size it was set for, and the result.
    laid: Vec<Option<(LaidFor, Laid)>>,
    /// The page colour the layouts were made in, and the layouts of the other appearance (kept, so that a
    /// crossfade, which paints both, does not lay the page out twice a frame).
    laid_for: u32,
    laid_other: (u32, Vec<Option<(LaidFor, Laid)>>),
    view_h: i32,
    /// Pixels a second, signed; 0 at rest.
    vel: f32,
    /// What the velocity has not scrolled yet (fractions of a pixel).
    carry: f32,
    last: Option<Instant>,
}

/// One press of Up or Down: a couple of lines.
pub const STEP: i32 = 72;
/// A screen jump keeps this much of the old view.
const OVERLAP: i32 = 100;

impl Default for Reader {
    fn default() -> Self {
        Self::new()
    }
}

impl Reader {
    pub fn new() -> Self {
        Reader {
            page: 0,
            scroll: vec![0; PAGES.len()],
            laid: (0..PAGES.len()).map(|_| None).collect(),
            laid_for: crate::palette::DARK.void,
            laid_other: (crate::palette::LIGHT.void, Vec::new()),
            view_h: 0,
            vel: 0.0,
            carry: 0.0,
            last: None,
        }
    }

    pub fn scroll(&self) -> i32 {
        self.scroll[self.page]
    }

    /// How far the page can be scrolled (known after the first paint; unlimited before).
    pub fn max_scroll(&self) -> i32 {
        self.laid[self.page].as_ref().map_or(i32::MAX, |(_, l)| (l.height - self.view_h).max(0))
    }

    pub fn scroll_by(&mut self, dy: i32) {
        let max = self.max_scroll();
        let s = &mut self.scroll[self.page];
        *s = (*s as i64 + dy as i64).clamp(0, max as i64) as i32;
    }

    /// A screen down (`dir` 1) or up (-1).
    pub fn screen(&mut self, dir: i32) {
        self.scroll_by(dir.signum() * (self.view_h - OVERLAP).max(STEP));
    }

    /// The next page (`dir` 1) or the one before, round and round.
    pub fn turn(&mut self, dir: i32) {
        let n = PAGES.len() as i32;
        self.page = (self.page as i32 + dir).rem_euclid(n) as usize;
        self.vel = 0.0;
        self.carry = 0.0;
    }

    /// Scrolls by itself at `px_per_s` (negative: up) until set to 0; time counts from `now`.
    pub fn set_velocity(&mut self, px_per_s: f32, now: Instant) {
        self.tick(now);
        self.vel = px_per_s;
        self.carry = 0.0;
        self.last = (px_per_s != 0.0).then_some(now);
    }

    pub fn moving(&self) -> bool {
        self.vel != 0.0
    }

    /// Moves by the velocity for the time since the last tick; true when the picture changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let Some(last) = self.last.filter(|_| self.vel != 0.0) else { return false };
        let dt = now.saturating_duration_since(last).as_secs_f32();
        self.last = Some(now);
        let before = self.scroll();
        let moved = self.vel * dt + self.carry;
        let whole = moved.trunc();
        self.carry = moved - whole;
        self.scroll_by(whole as i32);
        self.scroll() != before
    }

    /// Paints the page into the band (x, top, w, h) of the canvas, and a thin scroll bar at the right edge.
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, x: i32, top: i32, w: i32, h: i32) {
        self.view_h = h;
        let page = self.page;
        let void = crate::palette::pal().void;
        if self.laid_for != void {
            let fresh = if self.laid_other.0 == void { std::mem::take(&mut self.laid_other.1) } else { Vec::new() };
            let old = std::mem::replace(&mut self.laid, fresh);
            self.laid_other = (self.laid_for, old);
            self.laid.resize_with(PAGES.len(), || None);
            self.laid_for = void;
        }
        let key = (w, crate::type_scale::get());
        if self.laid[page].as_ref().map(|(k, _)| *k) != Some(key) {
            self.laid[page] = Some((key, layout(&parse(PAGES[page].1), text, w)));
        }
        let height = self.laid[page].as_ref().map_or(0, |(_, l)| l.height);
        let max = (height - h).max(0);
        self.scroll[page] = self.scroll[page].clamp(0, max);
        if let Some((_, laid)) = &self.laid[page] {
            paint(laid, text, c, x, top, self.scroll[page], h);
        }
        if max > 0 {
            let bar_h = (h as i64 * h as i64 / height as i64).max(24) as i32;
            let bar_y = ((h - bar_h) as i64 * self.scroll[page] as i64 / max as i64) as i32;
            c.rect(c.w - 12, top, 4, h, crate::palette::pal().pill);
            c.rect(c.w - 12, top + bar_y, 4, bar_h, crate::palette::pal().chip);
        }
    }

    /// For touch: the scroll position as a selected row of rows one `STEP` apart, as a list reports itself.
    pub fn touch_rows(&self) -> (usize, usize) {
        let max = self.max_scroll().min(1_000_000);
        (((self.scroll() + STEP - 1) / STEP) as usize, ((max + STEP - 1) / STEP + 1) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(t: &str) -> Span {
        Span {
            text: t.into(),
            ..Span::default()
        }
    }
    fn bold(t: &str) -> Span {
        Span {
            text: t.into(),
            bold: true,
            ..Span::default()
        }
    }
    fn chip(t: &str) -> Span {
        Span {
            text: t.into(),
            kind: Kind::Chip,
            ..Span::default()
        }
    }
    fn texts(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }
    fn chips(spans: &[Span]) -> Vec<&str> {
        spans.iter().filter(|s| s.kind == Kind::Chip).map(|s| s.text.as_str()).collect()
    }

    // ---- the parser ----------------------------------------------------------------------------------------

    #[test]
    fn headings_have_their_level_and_inline_markup() {
        let b = parse("# Settings, quick settings\n\n## Open **Settings**\n### Third\n");
        assert_eq!(b.len(), 3);
        assert!(matches!(&b[0], Block::Heading { level: 1, spans } if texts(spans) == "Settings, quick settings"));
        assert!(matches!(&b[1], Block::Heading { level: 2, spans } if spans == &vec![plain("Open "), bold("Settings")]));
        assert!(matches!(&b[2], Block::Heading { level: 3, spans } if texts(spans) == "Third"));
    }

    #[test]
    fn lines_of_a_paragraph_join_and_a_blank_line_ends_it() {
        let b = parse("First line\nsecond line.\n\nNext paragraph.\n");
        assert_eq!(b.len(), 2);
        assert_eq!(b[0], Block::Paragraph(vec![plain("First line second line.")]));
        assert_eq!(b[1], Block::Paragraph(vec![plain("Next paragraph.")]));
    }

    #[test]
    fn html_comments_vanish_even_across_lines_and_inside_text() {
        let md = "<!-- Product name:\n     second line -->\n\n# Title\n\nText <!-- note --> goes on.\n\n<!-- TODO: x -->\nAfter.\n";
        let b = parse(md);
        assert_eq!(b.len(), 3, "{b:?}");
        assert!(
            matches!(&b[1], Block::Paragraph(s) if texts(s) == "Text  goes on." || texts(s) == "Text goes on."),
            "{:?}",
            b[1]
        );
        assert!(matches!(&b[2], Block::Paragraph(s) if texts(s) == "After."));
    }

    #[test]
    fn lists_keep_their_depth_numbers_and_continuation_lines() {
        let md = "- one\n- two that goes\n  on a second line\n  - nested\n\n1. first\n2. second\n   - sub a\n   - sub b\n3. third\n";
        let b = parse(md);
        let items: Vec<(u8, Option<u32>, String)> = b
            .iter()
            .map(|x| match x {
                Block::Item { depth, number, spans } => (*depth, *number, texts(spans)),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            items,
            vec![
                (0, None, "one".to_string()),
                (0, None, "two that goes on a second line".to_string()),
                (1, None, "nested".to_string()),
                (0, Some(1), "first".to_string()),
                (0, Some(2), "second".to_string()),
                (1, None, "sub a".to_string()),
                (1, None, "sub b".to_string()),
                (0, Some(3), "third".to_string()),
            ]
        );
    }

    #[test]
    fn a_table_has_a_header_rows_and_cells_with_markup() {
        let md =
            "Before.\n\n| Entry | What it holds |\n|---|:---:|\n| **Look** | Choose how the shelf looks, `x` |\n| Wi-Fi | a \\| b |\n| Short |\n\nAfter.\n";
        let b = parse(md);
        assert_eq!(b.len(), 3, "{b:?}");
        let Block::Table { header, rows } = &b[1] else { panic!("{:?}", b[1]) };
        assert_eq!(header.iter().map(|c| texts(c)).collect::<Vec<_>>(), vec!["Entry", "What it holds"]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0][0], vec![bold("Look")]);
        assert_eq!(
            rows[0][1][1],
            Span {
                text: "x".into(),
                kind: Kind::Code,
                ..Span::default()
            }
        );
        assert_eq!(texts(&rows[1][1]), "a | b", "an escaped bar is part of the cell");
        assert_eq!(rows[2].len(), 2, "a short row is filled up to the header");
        assert!(rows[2][1].is_empty());
        assert!(matches!(&b[2], Block::Paragraph(s) if texts(s) == "After."));
    }

    #[test]
    fn inline_markup_bold_italic_code_links_and_images() {
        let s = spans("a **b** *c* `d e` [link text](playing.md#looks) tail");
        assert_eq!(texts(&s), "a b c d e link text tail");
        assert_eq!(s[1], bold("b"));
        assert_eq!(
            s[3],
            Span {
                text: "c".into(),
                italic: true,
                ..Span::default()
            }
        );
        assert_eq!(
            s[5],
            Span {
                text: "d e".into(),
                kind: Kind::Code,
                ..Span::default()
            }
        );
        assert_eq!(
            s[7],
            Span {
                text: "link text".into(),
                kind: Kind::Link,
                ..Span::default()
            }
        );
        // A link inside bold stays a bold link; the address is never shown.
        let s = spans("**[install guide](../INSTALL.md)**");
        assert_eq!(
            s,
            vec![Span {
                text: "install guide".into(),
                bold: true,
                kind: Kind::Link,
                ..Span::default()
            }]
        );
        // Images: the alt text, in italics, or nothing.
        assert_eq!(texts(&spans("see ![the shelf](shelf.png) here")), "see the shelf here");
        assert_eq!(texts(&spans("x![](a.png)y")), "xy");
        // Things that only look like markup stay as they are.
        assert_eq!(texts(&spans("2 * 3 = 6, a_b_c, <address> and 100%")), "2 * 3 = 6, a_b_c, <address> and 100%");
        assert_eq!(texts(&spans("an \\*escaped\\* star")), "an *escaped* star");
        assert_eq!(texts(&spans("**never closed")), "**never closed");
    }

    #[test]
    fn button_names_become_chips_where_they_are_unmistakable() {
        // Bold single buttons, and the long names anywhere in text.
        assert_eq!(chips(&spans("Press **START** on Home")), vec!["START"]);
        assert_eq!(chips(&spans("tap **A** or **B**")), vec!["A", "B"]);
        assert_eq!(chips(&spans("SELECT + START, held for 1 second")), vec!["SELECT", "START"]);
        assert_eq!(chips(&spans("**hold HOME and press SELECT**")), vec!["HOME", "SELECT"]);
        assert_eq!(chips(&spans("L1 / R1 jump, L2/R2 too")), vec!["L1", "R1", "L2", "R2"]);
        // "Press A", and B, X and Y on their own; A is also the article, so it needs more than being a capital.
        assert_eq!(chips(&spans("Press A to begin. Press B to go back.")), vec!["A", "B"]);
        assert_eq!(
            chips(&spans("B comes back. Tap X, then Y. A on it goes back; A switches it; use A or B.")),
            vec!["B", "X", "Y", "A", "A", "A", "B"]
        );
        assert_eq!(
            chips(&spans("A small picture of the shelf. A system that works. A game it found. A, B and C")),
            vec!["A", "B"]
        );
        assert_eq!(chips(&spans("**A is the right face button, B the bottom one.**")), vec!["A", "B"]);
        // Not buttons: the Home screen, a Start menu, code, bold words, links.
        assert_eq!(chips(&spans("Home and Start and `HOME` and **Save** and [START](x.md)")), Vec::<&str>::new());
        // The text around a chip keeps its spacing.
        let s = spans("Press **A** now");
        assert_eq!(s, vec![plain("Press "), Span { bold: true, ..chip("A") }, plain(" now")]);
    }

    #[test]
    fn a_button_column_and_a_leading_button_are_chips_too() {
        let b = parse("| Button | What it does |\n|---|---|\n| A | Play. |\n| L1 / R1 | Jump. |\n| B or HOME | Back. |\n| D-pad Up / Down | Move. |\n\n- A: type the selected key\n- B: delete\n- A game, as an article\n");
        let Block::Table { rows, .. } = &b[0] else { panic!("{:?}", b[0]) };
        assert_eq!(chips(&rows[0][0]), vec!["A"]);
        assert_eq!(chips(&rows[1][0]), vec!["L1", "R1"]);
        assert_eq!(chips(&rows[2][0]), vec!["B", "HOME"]);
        assert!(chips(&rows[3][0]).is_empty(), "D-pad is not a chip");
        assert!(chips(&rows[0][1]).is_empty(), "only the button column takes single letters");
        let lead: Vec<Vec<&str>> = b[1..]
            .iter()
            .map(|x| match x {
                Block::Item { spans, .. } => chips(spans),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(lead, vec![vec!["A"], vec!["B"], vec![]]);
        // The same letters in an ordinary table are text.
        let b = parse("| Look | Blurb |\n|---|---|\n| A | b |\n");
        let Block::Table { rows, .. } = &b[0] else { panic!() };
        assert!(chips(&rows[0][0]).is_empty());
    }

    #[test]
    fn code_blocks_quotes_and_rules() {
        let b = parse("```\nssh root@<address>\n  second\n```\n\n> **Status:** built\n> and not tested.\n\n---\nEnd\n");
        assert_eq!(b[0], Block::Code(vec!["ssh root@<address>".into(), "  second".into()]));
        assert!(
            matches!(&b[1], Block::Quote(s) if s[0] == bold("Status:") && texts(s) == "Status: built and not tested."),
            "{:?}",
            b[1]
        );
        assert_eq!(b[2], Block::Rule);
        assert!(matches!(&b[3], Block::Paragraph(s) if texts(s) == "End"));
    }

    #[test]
    fn every_character_of_the_guide_has_a_glyph_in_both_fonts() {
        let text = Text::new();
        let mut missing = std::collections::BTreeSet::new();
        for (name, md) in PAGES {
            for b in parse(md) {
                let mut all: Vec<String> = Vec::new();
                match &b {
                    Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } => all.push(texts(spans)),
                    Block::Table { header, rows } => {
                        all.extend(header.iter().map(|c| texts(c)));
                        all.extend(rows.iter().flatten().map(|c| texts(c)));
                    }
                    Block::Code(lines) => all.extend(lines.iter().cloned()),
                    Block::Rule | Block::Picture(_) => {}
                }
                for ch in all.iter().flat_map(|t| t.chars()) {
                    if !ch.is_whitespace() && !(text.has_glyph(Face::Body, ch) && text.has_glyph(Face::Title, ch)) {
                        missing.insert((name, ch));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "drawn as empty boxes: {missing:?}");
    }

    #[test]
    fn words_without_a_blank_between_them_stay_on_one_line() {
        let mut text = Text::new();
        // "(" and the code after it must not be split by a line end.
        let md = "word word word word word word word word word word word word (`fastboot boot`, nothing) end\n";
        for width in 300..700 {
            let l = layout(&parse(md), &mut text, width);
            let x_of = |s: &str| {
                l.items.iter().find_map(|i| {
                    if let Item::Text { x, y, text, .. } = i {
                        (text == s).then_some((*x, *y))
                    } else {
                        None
                    }
                })
            };
            let (open, fast) = (x_of("(").map(|p| p.1), x_of("fastboot").map(|p| p.1));
            assert_eq!(open, fast, "width {width}: a bracket and its code on one line");
        }
    }

    #[test]
    fn the_bundled_pages_parse_into_clean_text() {
        for (name, md) in PAGES {
            let blocks = parse(md);
            assert!(
                matches!(blocks.first(), Some(Block::Heading { level: 1, .. })),
                "{name} starts with its title: {:?}",
                blocks.first()
            );
            assert!(!title(md).is_empty(), "{name}");
            let mut all = Vec::new();
            for b in &blocks {
                match b {
                    Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } => all.push(texts(spans)),
                    Block::Table { header, rows } => {
                        for row in rows {
                            assert_eq!(row.len(), header.len(), "{name}: a row as wide as the header");
                            all.extend(row.iter().map(|c| texts(c)));
                        }
                        all.extend(header.iter().map(|c| texts(c)));
                    }
                    Block::Code(lines) => all.extend(lines.iter().filter(|l| l.contains("**")).cloned()),
                    Block::Rule | Block::Picture(_) => {}
                }
            }
            for t in all {
                for bad in ["**", "](", "<!--", "-->", "|---"] {
                    assert!(!t.contains(bad), "{name}: {bad:?} left in {t:?}");
                }
                assert!(!t.contains('`'), "{name}: backtick left in {t:?}");
            }
        }
        let titles: Vec<String> = PAGES.iter().map(|(_, md)| title(md)).collect();
        let mut unique = titles.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), titles.len(), "{titles:?}");
        let controls = parse(page("controls").expect("the Controls page"));
        let n: usize = controls
            .iter()
            .map(|b| match b {
                Block::Table { rows, .. } => rows.iter().map(|r| chips(&r[0]).len()).sum(),
                _ => 0,
            })
            .sum();
        assert!(n > 25, "the Controls tables are full of button chips: {n}");
    }

    // ---- layout and reading ---------------------------------------------------------------------------------

    fn extent(l: &Laid, text: &mut Text) -> i32 {
        l.items
            .iter()
            .map(|i| match i {
                Item::Text { x, px, face, text: t, .. } => x + text.width(*face, *px, t),
                Item::Chip { x, w, .. } | Item::Rect { x, w, .. } | Item::Controls { x, w, .. } => x + w,
            })
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn a_paragraph_wraps_inside_its_width_and_grows_downwards() {
        let mut text = Text::new();
        let md = "A long paragraph that has to be broken into several lines because the width is small, with a **bold** word and a chip: press **A** now.\n";
        let narrow = layout(&parse(md), &mut text, 500);
        let wide = layout(&parse(md), &mut text, 1100);
        assert!(extent(&narrow, &mut text) <= 500, "{}", extent(&narrow, &mut text));
        assert!(narrow.height >= wide.height + 2 * 49, "{} vs {}", narrow.height, wide.height);
        assert!(wide.height > 40 && narrow.items.len() > 10);
        // One chip, inline, as tall as a hint chip is round.
        let chips: Vec<_> = narrow.items.iter().filter(|i| matches!(i, Item::Chip { .. })).collect();
        assert_eq!(chips.len(), 1);
        assert!(matches!(chips[0], Item::Chip { w, h, label, .. } if *w >= *h && *h >= 36 && label == "A"));
    }

    #[test]
    fn a_table_fits_its_width_with_wrapped_cells_and_a_header() {
        let mut text = Text::new();
        let md = "| Button | What it does |\n|---|---|\n| A | Play the selected game, or open the selected system or Favorites, and so on for a while longer than one line fits. |\n| B | Back. |\n";
        let l = layout(&parse(md), &mut text, 1000);
        assert!(extent(&l, &mut text) <= 1000, "{}", extent(&l, &mut text));
        // The long cell wrapped into more than one line: more text items in that row than words in the short ones.
        let ys: std::collections::BTreeSet<i32> = l
            .items
            .iter()
            .filter_map(|i| if let Item::Text { y, .. } = i { Some(*y) } else { None })
            .collect();
        assert!(ys.len() >= 4, "header, 2+ lines of row one, row two: {ys:?}");
        // Two columns: the second starts to the right of the first.
        let x_of = |s: &str| {
            l.items.iter().find_map(|i| {
                if let Item::Text { x, text, .. } = i {
                    (text == s).then_some(*x)
                } else {
                    None
                }
            })
        };
        assert!(x_of("What").unwrap() > x_of("Button").unwrap() + 40, "{:?} {:?}", x_of("What"), x_of("Button"));
        assert!(l.height > 150);
    }

    #[test]
    fn every_bundled_page_lays_out_inside_the_screen() {
        let mut text = Text::new();
        for (name, md) in PAGES {
            let l = layout(&parse(md), &mut text, 1108);
            let wide: Vec<String> = l
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Text { x, px, face, text: t, .. } if x + text.width(*face, *px, t) > 1110 => Some(t.clone()),
                    _ => None,
                })
                .collect();
            assert!(extent(&l, &mut text) <= 1108 + 2, "{name}: {} {wide:?}", extent(&l, &mut text));
            assert!(l.height > 600, "{name}: {}", l.height);
            // Not a renderer limit (the page scrolls), but what still reads well on the 1240x1080 screen: about 30
            // screens. playing.md grew past 40 000 and was split into playing, pause-menu and options (2026-10-23;
            // options is the longest at about 26 000). Split a page that grows past this; don't raise it.
            assert!(l.height < 30_000, "{name}: {}", l.height);
            assert!(
                l.items.iter().all(|i| match i {
                    Item::Text { y, .. } | Item::Chip { y, .. } | Item::Rect { y, .. } | Item::Controls { y, .. } => *y >= 0 && *y <= l.height + 4,
                }),
                "{name}: items inside the document"
            );
        }
    }

    #[test]
    fn the_reader_scrolls_clamped_turns_pages_and_remembers_where_each_was() {
        let mut text = Text::new();
        let mut buf = vec![0u32; 1240 * 1080];
        let mut c = Canvas::new(&mut buf, 1240, 1080);
        let mut r = Reader::new();
        r.paint(&mut c, &mut text, 66, 140, 1108, 800);
        let max = r.max_scroll();
        assert!(max > 1000, "the index page is longer than the screen: {max}");
        r.scroll_by(-500);
        assert_eq!(r.scroll(), 0, "not above the top");
        r.scroll_by(STEP);
        assert_eq!(r.scroll(), STEP);
        r.scroll_by(1_000_000);
        assert_eq!(r.scroll(), max, "not below the end");
        r.screen(-1);
        assert_eq!(r.scroll(), max - (800 - 100), "a screen is the view minus a few lines of overlap");
        // Next page starts at the top, and coming back finds the old place.
        let (before, page) = (r.scroll(), r.page);
        r.turn(1);
        assert_eq!((r.page, r.scroll()), (page + 1, 0));
        r.paint(&mut c, &mut text, 66, 140, 1108, 800);
        r.scroll_by(300);
        r.turn(-1);
        assert_eq!((r.page, r.scroll()), (page, before));
        // Round and round.
        r.turn(-1);
        assert_eq!(r.page, PAGES.len() - 1);
        r.turn(1);
        assert_eq!(r.page, 0);
    }

    #[test]
    fn holding_the_stick_scrolls_at_its_speed_and_stops_when_let_go() {
        let mut text = Text::new();
        let mut buf = vec![0u32; 1240 * 1080];
        let mut c = Canvas::new(&mut buf, 1240, 1080);
        let mut r = Reader::new();
        r.paint(&mut c, &mut text, 66, 140, 1108, 800);
        let t0 = Instant::now();
        r.set_velocity(600.0, t0);
        assert!(r.moving());
        assert!(r.tick(t0 + std::time::Duration::from_millis(500)));
        assert!((r.scroll() - 300).abs() <= 2, "{}", r.scroll());
        r.set_velocity(0.0, t0 + std::time::Duration::from_millis(500));
        assert!(!r.moving());
        assert!(!r.tick(t0 + std::time::Duration::from_secs(5)));
        assert!((r.scroll() - 300).abs() <= 2);
        // Up and past the top stops at the top.
        r.set_velocity(-2000.0, t0 + std::time::Duration::from_secs(6));
        r.tick(t0 + std::time::Duration::from_secs(9));
        assert_eq!(r.scroll(), 0);
    }

    #[test]
    fn touch_sees_the_page_as_rows_of_one_step() {
        let mut text = Text::new();
        let mut buf = vec![0u32; 1240 * 1080];
        let mut c = Canvas::new(&mut buf, 1240, 1080);
        let mut r = Reader::new();
        r.paint(&mut c, &mut text, 66, 140, 1108, 800);
        let (sel, len) = r.touch_rows();
        assert_eq!(sel, 0);
        assert_eq!(len as i32, (r.max_scroll() + STEP - 1) / STEP + 1);
        r.scroll_by(5 * STEP);
        assert_eq!(r.touch_rows().0, 5);
    }

    #[test]
    fn a_page_paints_text_and_chips_and_scrolling_changes_the_picture() {
        let mut text = Text::new();
        let mut a = vec![0u32; 1240 * 1080];
        let mut b = vec![0u32; 1240 * 1080];
        let mut r = Reader::new();
        r.page = 3; // Controls: tables full of chips
        r.paint(&mut Canvas::new(&mut a, 1240, 1080), &mut text, 66, 140, 1108, 800);
        assert!(a.contains(&crate::palette::DARK.chip), "chip colour of the hint bar");
        assert!(a.iter().filter(|p| **p != 0).count() > 20_000, "text on screen");
        r.scroll_by(600);
        r.paint(&mut Canvas::new(&mut b, 1240, 1080), &mut text, 66, 140, 1108, 800);
        assert!(a != b);
        // Nothing above the view's top edge is touched by the page itself (the header lives there).
        assert!(a[..1240 * 100].iter().all(|p| *p == 0), "the area above the document stays empty");
    }
}
