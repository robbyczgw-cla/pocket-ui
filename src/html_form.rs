//! A small, tolerant reader for the HTML of a Wi-Fi sign-in page (`portal`): the page's title and first heading, its
//! plain text, its forms and their fields, and a `<meta http-equiv="refresh">` or trivial JavaScript redirect. No
//! tree is built and nothing is evaluated: one pass over the text, a few flags of where it is, and hard limits on
//! everything kept, so a hostile page (garbage, gigabytes of nesting, endless attributes) costs one linear scan and
//! a bounded amount of memory, and never a panic (pocket-ui is built with `panic = "abort"`).
//!
//! What it understands is what plain sign-in pages use: `<form action method enctype>`, `<input>` of the types text,
//! email, number, tel, password, checkbox, radio, hidden, submit, image and button, `<select>`/`<option>`,
//! `<textarea>`, `<button>`, labels (`<label for>`, a label around a field, `aria-label`, `placeholder`, the text just
//! before a field), `<noscript>` content (read as a browser without JavaScript reads it), entities, and the
//! `charset` of the page. Scripts and styles are skipped and never run; what a "please wait" interstitial does with
//! them is read out instead of run: a script (or an `onload` handler) that goes to one fixed address
//! (`location.href = "…"`, `location.replace(…)`, `setTimeout(function(){ location… = "…" }, n)`), a form the page
//! submits on load whose fields are all hidden ([`Page::auto_form`]), a link in `<noscript>` ([`Page::noscript_link`],
//! preferred over the scripts), and a single `<iframe src>` ([`Page::iframes`]). Nothing built at run time (a `+`, a
//! variable) is ever followed.

/// Most text kept of a page (the terms are shown shortened anyway).
pub const MAX_TEXT: usize = 12_000;
pub const MAX_FORMS: usize = 8;
pub const MAX_FIELDS: usize = 64;
pub const MAX_OPTIONS: usize = 200;
/// Most characters kept of an attribute value (a hidden token can be long; nothing a person types is longer).
pub const MAX_VALUE: usize = 8192;
/// Most characters of a label, a title or a heading.
pub const MAX_LABEL: usize = 300;
const MAX_ATTRS: usize = 64;
/// Text gathered while waiting for a field (the "Room number:" before an `<input>`).
const MAX_BEFORE: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Hidden,
    Text,
    Email,
    Number,
    Tel,
    Password,
    Checkbox,
    /// A group of radio buttons of one name, as one field with `options` (merged after reading).
    Radio,
    Select,
    Textarea,
    /// `<input type=submit|image>` or `<button>` that submits; `image` sends `name.x` and `name.y`.
    Submit { image: bool },
    /// What a sign-in helper can't fill in (`file`, `<button type=button>`, `reset`, unknown types): not sent.
    Other,
}

impl Kind {
    /// A field the person fills in or ticks.
    pub fn shown(self) -> bool {
        matches!(self, Kind::Text | Kind::Email | Kind::Number | Kind::Tel | Kind::Password | Kind::Checkbox | Kind::Radio | Kind::Select | Kind::Textarea)
    }

    /// Typed with the keyboard.
    pub fn typed(self) -> bool {
        matches!(self, Kind::Text | Kind::Email | Kind::Number | Kind::Tel | Kind::Password | Kind::Textarea)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub kind: Kind,
    pub name: String,
    /// What is sent: the typed text, the hidden value, a checkbox's or a submit button's value.
    pub value: String,
    pub label: String,
    pub id: String,
    pub checked: bool,
    pub required: bool,
    pub disabled: bool,
    /// (value, label) of a select or a radio group.
    pub options: Vec<(String, String)>,
    /// The chosen option; None for a radio group with nothing chosen yet.
    pub selected: Option<usize>,
    /// `maxlength`, capped at `MAX_VALUE`.
    pub max: usize,
}

impl Field {
    fn new(kind: Kind) -> Field {
        Field {
            kind,
            name: String::new(),
            value: String::new(),
            label: String::new(),
            id: String::new(),
            checked: false,
            required: false,
            disabled: false,
            options: Vec::new(),
            selected: None,
            max: MAX_VALUE,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    /// The `action` as written (resolved against the page's address by `portal`); None: the page's own address.
    pub action: Option<String>,
    pub method: Method,
    pub multipart: bool,
    pub fields: Vec<Field>,
    /// `onsubmit` is set: a script runs before (or instead of) sending.
    pub onsubmit: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub title: String,
    pub heading: String,
    /// The visible text, one paragraph a line, whitespace folded, at most `MAX_TEXT` characters.
    pub text: String,
    pub forms: Vec<Form>,
    /// Where `<meta http-equiv=refresh>`, a trivial script or an `onload` handler sends the browser (as written).
    pub redirect: Option<String>,
    /// The first link in a `<noscript>` block: what a browser without scripts offers instead (the real one, often).
    pub noscript_link: Option<String>,
    /// The `src` of each `<iframe>` (at most 4); a page that is one frame leads to it.
    pub iframes: Vec<String>,
    /// A form the page's own script submits on load (`document.forms[0].submit()`, `<body onload="…submit()">`) whose
    /// fields are all hidden: nothing to fill in or tick, only what the page carries. Set after reading the page.
    pub auto_form: Option<usize>,
    /// How many `<script>` elements the page has.
    pub scripts: usize,
}

/// The page's text as a string: UTF-8 unless the `Content-Type` or a `<meta charset>` near the top says Latin-1
/// (or Windows-1252, read as Latin-1); invalid bytes become U+FFFD.
pub fn decode(bytes: &[u8], content_type: &str) -> String {
    let head = &bytes[..bytes.len().min(2048)];
    let head = String::from_utf8_lossy(head).to_ascii_lowercase();
    let latin = |s: &str| {
        s.split("charset=").nth(1).is_some_and(|c| {
            let c = c.trim_start_matches(['"', '\'', ' ']);
            c.starts_with("iso-8859-1") || c.starts_with("windows-1252") || c.starts_with("latin1") || c.starts_with("iso8859-1")
        })
    };
    let ct = content_type.to_ascii_lowercase();
    let is_latin = if ct.contains("charset=") { latin(&ct) } else { latin(&head) || head.contains("charset=\"iso-8859-1") };
    if is_latin && std::str::from_utf8(bytes).is_err() {
        bytes.iter().map(|&b| b as char).collect()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Characters that don't belong in what is shown: controls (but the line breaks `parse` makes) and the invisible
/// direction and width marks that can make text read differently from what it is.
fn shown_char(c: char) -> bool {
    !(c.is_control() || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2069}' | '\u{feff}'))
}

/// `s` with entities decoded (`&amp;`, `&#228;`, `&#xE4;`, the common named ones); an unknown one stays as written.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        // Up to 12 characters to the ';' (the longest name below, "&hellip;", and "&#x10FFFF;" fit).
        let end = rest.char_indices().take(14).find(|(_, c)| *c == ';').map(|(k, _)| k);
        let decoded = end.and_then(|e| entity(&rest[1..e]).map(|c| (c, e + 1)));
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let n = match num.strip_prefix(['x', 'X']) {
            Some(hex) if !hex.is_empty() && hex.len() <= 8 => u32::from_str_radix(hex, 16).ok()?,
            None if !num.is_empty() && num.len() <= 8 => num.parse().ok()?,
            _ => return None,
        };
        return Some(char::from_u32(n).filter(|c| *c != '\0').unwrap_or('\u{fffd}'));
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "auml" => 'ä',
        "ouml" => 'ö',
        "uuml" => 'ü',
        "Auml" => 'Ä',
        "Ouml" => 'Ö',
        "Uuml" => 'Ü',
        "szlig" => 'ß',
        "eacute" => 'é',
        "egrave" => 'è',
        "agrave" => 'à',
        "aacute" => 'á',
        "ccedil" => 'ç',
        "ntilde" => 'ñ',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "euro" => '€',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "laquo" => '«',
        "raquo" => '»',
        "bdquo" => '„',
        "ldquo" => '“',
        "rdquo" => '”',
        "lsquo" => '‘',
        "rsquo" => '’',
        "middot" => '·',
        "bull" => '•',
        _ => return None,
    })
}

/// One tag as read: its name in lower case, whether it closes, its attributes (names in lower case, values unescaped).
struct Tag {
    name: String,
    end: bool,
    attrs: Vec<(String, String)>,
}

impl Tag {
    fn get(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    fn has(&self, name: &str) -> bool {
        self.attrs.iter().any(|(k, _)| k == name)
    }
}

/// Cuts `s` to at most `max` characters (at a character boundary).
fn cap(mut s: String, max: usize) -> String {
    if let Some((i, _)) = s.char_indices().nth(max) {
        s.truncate(i);
    }
    s
}

/// Whitespace folded to single spaces, invisible marks and controls dropped, cut to `max`.
fn clean(s: &str, max: usize) -> String {
    let mut out = String::new();
    for word in s.split(|c: char| c.is_whitespace() || c == '\u{a0}') {
        let word: String = word.chars().filter(|c| shown_char(*c)).collect();
        if word.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&word);
        if out.len() > max * 4 {
            break;
        }
    }
    cap(out, max)
}

/// Reads one tag starting after its '<' in `s` at `i`; returns it and where it ends (after '>'), or None when this
/// '<' starts no tag (plain text).
fn read_tag(s: &str, mut i: usize) -> Option<(Tag, usize)> {
    let b = s.as_bytes();
    let end = b.get(i) == Some(&b'/');
    if end {
        i += 1;
    }
    let start = i;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-' || b[i] == b':') {
        i += 1;
    }
    if i == start || !b[start].is_ascii_alphabetic() {
        return None;
    }
    let name = s[start..i.min(start + 32)].to_ascii_lowercase();
    let mut attrs = Vec::new();
    loop {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        if i >= b.len() {
            return Some((Tag { name, end, attrs }, b.len()));
        }
        if b[i] == b'>' {
            return Some((Tag { name, end, attrs }, i + 1));
        }
        let k0 = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'>' && b[i] != b'/' {
            i += 1;
        }
        // A lone '=' or another odd byte: step over it, so the loop always moves on.
        if i == k0 {
            i += 1;
            continue;
        }
        let key = s.get(k0..i).unwrap_or("").to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let v0 = i + 1;
                let v1 = b[v0.min(b.len())..].iter().position(|&c| c == q).map_or(b.len(), |p| v0 + p);
                value = s.get(v0..v1).unwrap_or("").to_string();
                i = (v1 + 1).min(b.len());
            } else {
                let v0 = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                    i += 1;
                }
                value = s.get(v0..i).unwrap_or("").to_string();
            }
        }
        if attrs.len() < MAX_ATTRS && !attrs.iter().any(|(k, _): &(String, String)| *k == key) {
            attrs.push((cap(key, 64), cap(unescape(&cap(value, MAX_VALUE * 2)), MAX_VALUE)));
        }
    }
}

/// Where the raw text of `<script>`, `<style>`, `<textarea>` or `<title>` that starts at `i` ends: the start of its
/// end tag (case-insensitive), or the end of the page.
fn raw_end(s: &str, i: usize, name: &str) -> usize {
    let b = s.as_bytes();
    let needle = format!("</{name}");
    let n = needle.as_bytes();
    let mut k = i;
    while let Some(p) = b.get(k..).and_then(|rest| rest.iter().position(|&c| c == b'<')) {
        let at = k + p;
        if b.len() - at >= n.len() && b[at..at + n.len()].eq_ignore_ascii_case(n) {
            return at;
        }
        k = at + 1;
    }
    b.len()
}

/// The address a tiny script (or an `onload` handler) sends the browser to and nothing else: `location.href = "…"`,
/// `window.location = '…'`, `document.location…`, `top.location…`, `location.replace("…")`, `location.assign(…)`, and
/// the same inside `setTimeout(function(){ location… = "…" }, n)`. Anything built at run time (a `+`, a variable) is
/// not followed. The quote after the address may close a wrapper string (`setTimeout("location='…'", 500)`).
pub fn script_redirect(script: &str) -> Option<String> {
    if script.len() > 2000 {
        return None;
    }
    let compact: String = script.chars().filter(|c| !c.is_whitespace()).collect();
    for pat in ["location.href=", "location=", "location.replace(", "location.assign("] {
        let mut from = 0;
        while let Some(p) = compact[from..].find(pat) {
            let at = from + p + pat.len();
            from = at;
            let rest = &compact[at..];
            let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else { continue };
            let Some(close) = rest[1..].find(q) else { continue };
            let url = &rest[1..1 + close];
            let after = rest[1 + close + 1..].chars().next();
            if matches!(after, None | Some(';') | Some(')') | Some('}') | Some(',') | Some('"') | Some('\''))
                && !url.is_empty()
                && !url.contains('\\')
            {
                // The text without whitespace: an address has none anyway.
                return Some(url.to_string());
            }
        }
    }
    None
}

/// `content` of `<meta http-equiv="refresh" content="0; url=…">`: the address. Odd spacing, quotes and case are fine
/// (`0; URL = '…'`, `0;URL="…"`); the word `url` must stand alone (`…?curly=x` is not it).
fn refresh_url(content: &str) -> Option<String> {
    let lower = content.to_ascii_lowercase();
    let mut from = 0;
    while let Some(p) = lower[from..].find("url") {
        let at = from + p;
        from = at + 3;
        let alone = at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric();
        let b = content.as_bytes();
        let mut k = at + 3;
        while k < b.len() && b[k].is_ascii_whitespace() {
            k += 1;
        }
        if alone && b.get(k) == Some(&b'=') {
            k += 1;
            while k < b.len() && b[k].is_ascii_whitespace() {
                k += 1;
            }
            let url = content[k..].trim().trim_matches(['"', '\'']).trim();
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// `code` with the inside of strings and comments blanked (the quotes stay), so keyword search can't be fooled by
/// what is only written down.
fn outside_strings(code: &str) -> String {
    let b = code.as_bytes();
    let mut out: Vec<u8> = b.to_vec();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            q @ (b'"' | b'\'' | b'`') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    if b[i] == b'\\' && i + 1 < b.len() {
                        out[i] = b' ';
                        i += 1;
                    }
                    out[i] = b' ';
                    i += 1;
                }
                if i < b.len() {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                out[i] = b' ';
                out[i + 1] = b' ';
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    out[i] = b' ';
                    i += 1;
                }
                if i + 1 < b.len() {
                    out[i] = b' ';
                    out[i + 1] = b' ';
                    i += 2;
                }
            }
            _ => i += 1,
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The range of the group `open` starts at its matching close (`(`/`{`, strings already blanked): (start, end).
fn balanced(s: &str, open: usize, close: u8) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let other = if b[open] == b'(' { b'(' } else { b'{' };
    let mut n = 0;
    for (i, &c) in b.iter().enumerate().skip(open) {
        if c == other {
            n += 1;
        } else if c == close {
            n -= 1;
            if n == 0 {
                return Some((open, i + 1));
            }
        }
    }
    None
}

/// Byte ranges of one script: (start, end).
type Ranges = Vec<(usize, usize)>;

/// The byte ranges that run when the page loads (a `setTimeout`/`setInterval` call, jQuery's `$(function(){…})` and
/// `.ready(…)`, a `load`/`DOMContentLoaded` listener, a function assigned to `onload`) and the function bodies (what
/// only a call by name or a press runs). `s`: the code with strings and comments blanked, `raw`: as written.
fn load_and_fn_regions(s: &str, raw: &str) -> (Ranges, Ranges) {
    let low = s.to_ascii_lowercase();
    let mut loads: Vec<(usize, usize)> = Vec::new();
    let mut fns: Vec<(usize, usize)> = Vec::new();
    // Function bodies: `function name(…) {…}` and `… => {…}`.
    let mut i = 0;
    while let Some(p) = low[i..].find("function") {
        i += p;
        let word = i == 0 || !low.as_bytes()[i - 1].is_ascii_alphanumeric();
        let mut k = i + 8;
        while k < low.len() && low.as_bytes()[k].is_ascii_whitespace() {
            k += 1;
        }
        while k < low.len() && (low.as_bytes()[k].is_ascii_alphanumeric() || low.as_bytes()[k] == b'_' || low.as_bytes()[k] == b'$') {
            k += 1;
        }
        while k < low.len() && low.as_bytes()[k].is_ascii_whitespace() {
            k += 1;
        }
        if word && low.as_bytes().get(k) == Some(&b'(') {
            if let Some((_, params_end)) = balanced(s, k, b')') {
                let mut e = params_end;
                while e < low.len() && low.as_bytes()[e].is_ascii_whitespace() {
                    e += 1;
                }
                if low.as_bytes().get(e) == Some(&b'{') {
                    if let Some(r) = balanced(s, e, b'}') {
                        fns.push(r);
                    }
                }
            }
        }
        i += 8;
    }
    let mut i = 0;
    while let Some(p) = low[i..].find("=>") {
        i += p + 2;
        let mut e = i;
        while e < low.len() && low.as_bytes()[e].is_ascii_whitespace() {
            e += 1;
        }
        if low.as_bytes().get(e) == Some(&b'{') {
            if let Some(r) = balanced(s, e, b'}') {
                fns.push(r);
            }
        }
    }
    // The calls whose region runs on load.
    for name in ["settimeout(", "setinterval(", "$(", "jquery(", "ready("] {
        let mut from = 0;
        while let Some(p) = low[from..].find(name) {
            let at = from + p;
            from = at + name.len();
            let open = at + name.len() - 1;
            if let Some(r) = balanced(s, open, b')') {
                loads.push(r);
            }
        }
    }
    // A listener of `load` or `DOMContentLoaded` (not `click` or `beforeunload`): its whole argument list.
    for name in ["addeventlistener(", ".on("] {
        let mut from = 0;
        while let Some(p) = low[from..].find(name) {
            let at = from + p;
            from = at + name.len();
            let open = at + name.len() - 1;
            if let Some((a, b)) = balanced(s, open, b')') {
                // The event name sits inside a string `outside_strings` blanked: read it from the code as written.
                let args = &raw[a..b];
                if ["'load'", "\"load\"", "'domcontentloaded'", "\"domcontentloaded\""].iter().any(|w| args.contains(w)) {
                    loads.push((a, b));
                }
            }
        }
    }
    // A function assigned to `onload` (its body is among `fns`): what stands between `onload` and its start.
    let mut from = 0;
    while let Some(p) = low[from..].find("onload") {
        let at = from + p;
        from = at + 6;
        let after = &low[at + 6..(at + 400).min(low.len())];
        let end = after.find(';').unwrap_or(after.len());
        if let Some(r) = fns.iter().copied().find(|(s, _)| *s >= at + 6 && *s <= at + 6 + end) {
            loads.push(r);
        }
    }
    (loads, fns)
}

/// Does this script (or `onload` handler) submit a form **on load**: a bare `submit()` at the top level, or inside
/// `setTimeout(…)`, `window.onload = function(){…}`, a `load`/`DOMContentLoaded` listener or jQuery's ready. Never
/// what only runs on a press: `submit(` inside a function body that is not a load callback (an `onclick` handler, a
/// named function called from one), and never jQuery's `submit(function(){…})`, which binds a handler and does not
/// submit. Only a bare `submit()` counts (`form.submit(…)` with arguments is a handler being bound). The form this
/// unlocks must still have only hidden fields and no button of its own (`Page::auto_form`), so nothing a person
/// chose is ever sent.
fn submits_on_load(code: &str) -> bool {
    if code.len() > 2000 {
        return false;
    }
    let s = outside_strings(code);
    let low = s.to_ascii_lowercase();
    let (loads, fns) = load_and_fn_regions(&s, &code.to_ascii_lowercase());
    let b = low.as_bytes();
    let mut from = 0;
    while let Some(p) = low[from..].find("submit(") {
        let at = from + p;
        from = at + 7;
        // `submit(function(){…})` (jQuery) binds a handler; only a bare call submits.
        let mut k = at + 7;
        while k < b.len() && b[k].is_ascii_whitespace() {
            k += 1;
        }
        if b.get(k) != Some(&b')') {
            continue;
        }
        if loads.iter().any(|&(a, e)| at >= a && at < e) || !fns.iter().any(|&(a, e)| at >= a && at < e) {
            return true;
        }
    }
    false
}

/// Tags after which the text starts a new paragraph.
fn block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div" | "br" | "li" | "ul" | "ol" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "tr" | "table" | "form" | "section" | "article" | "header" | "footer" | "fieldset" | "legend" | "hr" | "dt" | "dd" | "blockquote" | "main" | "nav" | "label"
    )
}

/// An open `<label>`: its `for`, its text so far, the fields it holds (form, field).
type OpenLabel = (String, String, Vec<(usize, usize)>);

struct Reader {
    page: Page,
    /// The form being read (an index into `page.forms`), None outside one or past `MAX_FORMS`.
    form: Option<usize>,
    in_form: bool,
    /// Text since the last field (for a label written just before it).
    before: String,
    /// The open `<label>`: its `for`, its text, the fields it holds (form, field).
    label: Option<OpenLabel>,
    /// `<label for>` texts: (id, text).
    for_labels: Vec<(String, String)>,
    /// The open `<select>` (form, field) and the option being read.
    select: Option<(usize, usize)>,
    option: Option<(String, String, bool)>,
    /// The open `<button>` (form, field) and its text.
    button: Option<(usize, usize, String)>,
    /// The open `<h1>`/`<h2>` and its text.
    heading: Option<String>,
    /// Inside `<head>` (only the title counts there).
    head: bool,
    /// Inside `<noscript>`: its links count (`noscript_link`), its forms and text count as they always did.
    noscript: usize,
    /// A script or `onload` handler submits a form on load (`submits_on_load`).
    auto_submit: bool,
    /// The page has something a person is meant to press (a button, a button-like input, an action link): a form the
    /// page submits on load is not sent by itself then.
    press: bool,
    text_full: bool,
    /// The text so far ended with whitespace (the next piece is a new word).
    space: bool,
}

impl Reader {
    fn push_text(&mut self, t: &str) {
        if let Some(h) = &mut self.heading {
            if h.len() < MAX_LABEL * 4 {
                h.push_str(t);
            }
        }
        if let Some((_, l, _)) = &mut self.label {
            if l.len() < MAX_LABEL * 4 {
                l.push_str(t);
            }
        }
        // A label's words name what it holds or points to, not the next field.
        let in_label = self.label.is_some();
        if let Some((_, l, _)) = &mut self.option {
            if l.len() < MAX_LABEL * 4 {
                l.push_str(t);
            }
            return;
        }
        if let Some((_, _, l)) = &mut self.button {
            if l.len() < MAX_LABEL * 4 {
                l.push_str(t);
            }
            return;
        }
        if self.in_form && !in_label {
            self.before.push_str(t);
            if self.before.len() > MAX_BEFORE * 4 {
                let cut = self.before.len() - MAX_BEFORE * 2;
                let cut = (cut..self.before.len()).find(|i| self.before.is_char_boundary(*i)).unwrap_or(self.before.len());
                self.before.drain(..cut);
            }
        }
        // A label's words are on its field's row already.
        if self.head || self.text_full || in_label {
            return;
        }
        let spaced = std::mem::replace(&mut self.space, t.ends_with(char::is_whitespace)) || t.starts_with(char::is_whitespace);
        let t = clean(t, MAX_TEXT);
        if t.is_empty() {
            self.space |= spaced;
            return;
        }
        let text = &mut self.page.text;
        // A space only where the page had one: "the <a>terms</a>." stays "the terms."
        if spaced && !text.is_empty() && !text.ends_with('\n') {
            text.push(' ');
        }
        text.push_str(&t);
        if text.chars().count() >= MAX_TEXT {
            *text = cap(std::mem::take(text), MAX_TEXT);
            self.text_full = true;
        }
    }

    fn paragraph(&mut self) {
        let text = &mut self.page.text;
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
            self.space = false;
        }
    }

    /// Adds a field to the open form (nothing outside a form, nothing past the limits); returns where it went.
    fn add(&mut self, mut f: Field, tag: &Tag) -> Option<(usize, usize)> {
        let fi = self.form?;
        let form = self.page.forms.get_mut(fi)?;
        if form.fields.len() >= MAX_FIELDS {
            return None;
        }
        f.name = tag.get("name").unwrap_or("").to_string();
        f.id = cap(tag.get("id").unwrap_or("").to_string(), 200);
        f.required = tag.has("required") || tag.get("aria-required") == Some("true");
        f.disabled = tag.has("disabled");
        if let Some(m) = tag.get("maxlength").and_then(|m| m.trim().parse::<usize>().ok()) {
            f.max = m.clamp(1, MAX_VALUE);
        }
        // The label, best first: aria-label, the text just before it (a label element comes later), placeholder, title.
        let before = clean(&self.before, MAX_LABEL);
        let before = before.rsplit(['\n', '.', '!', '?']).next().unwrap_or("").trim().trim_end_matches(['*', ':']).trim().to_string();
        f.label = [tag.get("aria-label").unwrap_or(""), &before, tag.get("placeholder").unwrap_or(""), tag.get("title").unwrap_or("")]
            .iter()
            .map(|l| clean(l, MAX_LABEL))
            .find(|l| !l.is_empty())
            .unwrap_or_default();
        if f.kind.shown() {
            self.before.clear();
        }
        form.fields.push(f);
        let at = (fi, form.fields.len() - 1);
        if let Some((_, _, held)) = &mut self.label {
            held.push(at);
        }
        Some(at)
    }

    fn field_mut(&mut self, (fi, k): (usize, usize)) -> Option<&mut Field> {
        self.page.forms.get_mut(fi)?.fields.get_mut(k)
    }

    fn start(&mut self, tag: &Tag) {
        match tag.name.as_str() {
            "head" => self.head = true,
            "body" => self.head = false,
            "form" if !self.in_form => {
                self.in_form = true;
                self.before.clear();
                if self.page.forms.len() < MAX_FORMS {
                    let method = if tag.get("method").is_some_and(|m| m.trim().eq_ignore_ascii_case("post")) { Method::Post } else { Method::Get };
                    self.page.forms.push(Form {
                        action: tag.get("action").map(|a| a.trim().to_string()).filter(|a| !a.is_empty()),
                        method,
                        multipart: tag.get("enctype").is_some_and(|e| e.trim().eq_ignore_ascii_case("multipart/form-data")),
                        fields: Vec::new(),
                        onsubmit: tag.has("onsubmit"),
                    });
                    self.form = Some(self.page.forms.len() - 1);
                }
            }
            "input" => {
                let ty = tag.get("type").unwrap_or("text").trim().to_ascii_lowercase();
                if matches!(ty.as_str(), "button" | "submit" | "image" | "reset") {
                    self.press = true;
                }
                let kind = match ty.as_str() {
                    "hidden" => Kind::Hidden,
                    "text" | "search" | "url" | "" => Kind::Text,
                    "email" => Kind::Email,
                    "number" => Kind::Number,
                    "tel" => Kind::Tel,
                    "password" => Kind::Password,
                    "checkbox" => Kind::Checkbox,
                    "radio" => Kind::Radio,
                    "submit" => Kind::Submit { image: false },
                    "image" => Kind::Submit { image: true },
                    _ => Kind::Other,
                };
                let mut f = Field::new(kind);
                f.checked = tag.has("checked");
                f.value = match kind {
                    Kind::Checkbox | Kind::Radio => tag.get("value").unwrap_or("on").to_string(),
                    _ => tag.get("value").unwrap_or("").to_string(),
                };
                // A button without words gets the helper's own "Send" when it is shown (`portal_page`).
                let submit_label = matches!(kind, Kind::Submit { .. }).then(|| clean(tag.get("value").or(tag.get("alt")).unwrap_or(""), MAX_LABEL));
                if let Some(at) = self.add(f, tag) {
                    if let (Some(l), Some(f)) = (submit_label, self.field_mut(at)) {
                        f.label = l;
                    }
                }
            }
            "textarea" => {
                let f = Field::new(Kind::Textarea);
                self.add(f, tag);
            }
            "select" => {
                let f = Field::new(Kind::Select);
                self.select = self.add(f, tag);
            }
            "option" => {
                self.end_option();
                if self.select.is_some() {
                    self.option = Some((tag.get("value").map(str::to_string).unwrap_or_default(), String::new(), tag.has("selected")));
                    // No value attribute: the text is the value (`end_option`).
                    if !tag.has("value") {
                        if let Some(o) = &mut self.option {
                            o.0 = "\u{0}".into();
                        }
                    }
                }
            }
            "button" => {
                // Any button is meant to be pressed: nothing submits by itself on such a page.
                self.press = true;
                let ty = tag.get("type").unwrap_or("submit").trim().to_ascii_lowercase();
                let kind = if ty == "submit" { Kind::Submit { image: false } } else { Kind::Other };
                let mut f = Field::new(kind);
                f.value = tag.get("value").unwrap_or("").to_string();
                if let Some((fi, k)) = self.add(f, tag) {
                    self.button = Some((fi, k, String::new()));
                }
            }
            "label" => {
                self.label = Some((tag.get("for").unwrap_or("").to_string(), String::new(), Vec::new()));
            }
            "h1" | "h2" if self.page.heading.is_empty() && self.heading.is_none() => self.heading = Some(String::new()),
            "meta" if tag.get("http-equiv").is_some_and(|h| h.eq_ignore_ascii_case("refresh")) => {
                if self.page.redirect.is_none() {
                    self.page.redirect = tag.get("content").and_then(refresh_url);
                }
            }
            "noscript" => self.noscript += 1,
            "a" => {
                // The link a browser without scripts is offered instead ("If you are not redirected, click here").
                if self.noscript > 0 {
                    if let Some(href) = tag.get("href").map(str::trim).filter(|h| {
                        let low = h.to_ascii_lowercase();
                        !h.is_empty() && !h.starts_with('#') && !low.starts_with("javascript:") && !low.starts_with("mailto:")
                    }) {
                        self.page.noscript_link.get_or_insert_with(|| href.to_string());
                    }
                }
                // An action link ("Accept and connect"): a person is meant to press it.
                let acts = tag.has("onclick")
                    || tag.get("href").is_none_or(|h| {
                        let h = h.trim();
                        h.is_empty() || h.starts_with('#') || h.trim().to_ascii_lowercase().starts_with("javascript:")
                    });
                if acts {
                    self.press = true;
                }
            }
            "iframe" => {
                // A frame that shows something (a sign-in form), never a tracker: not in <noscript> (the Google Tag
                // Manager snippet) and not one pixel tall.
                let tiny = tag.get("width").is_some_and(|w| w.trim().parse::<u32>().is_ok_and(|v| v <= 1))
                    || tag.get("height").is_some_and(|h| h.trim().parse::<u32>().is_ok_and(|v| v <= 1))
                    || tag.has("hidden")
                    || tag.get("style").is_some_and(|s| {
                        let s = s.to_ascii_lowercase().replace(' ', "");
                        s.contains("display:none") || s.contains("visibility:hidden")
                    });
                if self.noscript == 0 && !tiny {
                    if let Some(src) = tag.get("src").map(str::trim).filter(|s| !s.is_empty() && !s.starts_with('#')) {
                        if self.page.iframes.len() < 4 {
                            self.page.iframes.push(src.to_string());
                        }
                    }
                }
            }
            "img" => {
                if let Some(alt) = tag.get("alt").filter(|a| !a.trim().is_empty()) {
                    // Only inside a button: a logo's alt text says nothing on its own.
                    if let Some((_, _, l)) = &mut self.button {
                        l.push_str(alt);
                    }
                }
            }
            _ => {}
        }
        // An `onload` that goes to one address is followed as a redirect; one that submits a form counts for
        // `auto_form`. Only `onload`: what a click handler does is never taken for a navigation.
        if let Some(v) = tag.get("onload") {
            if self.page.redirect.is_none() {
                self.page.redirect = script_redirect(v);
            }
            if !self.auto_submit {
                self.auto_submit = submits_on_load(v);
            }
        }
        if block(&tag.name) {
            self.paragraph();
        }
    }

    fn end_option(&mut self) {
        let (Some(at), Some((value, label, selected))) = (self.select, self.option.take()) else { return };
        let label = clean(&label, MAX_LABEL);
        let value = if value == "\u{0}" { label.clone() } else { value };
        if let Some(f) = self.field_mut(at) {
            if f.options.len() < MAX_OPTIONS {
                f.options.push((value, label));
                if selected || f.selected.is_none() {
                    f.selected = Some(f.options.len() - 1);
                }
            }
        }
    }

    fn end(&mut self, name: &str) {
        match name {
            "head" => self.head = false,
            "noscript" => self.noscript = self.noscript.saturating_sub(1),
            "form" => {
                self.in_form = false;
                self.form = None;
                self.label = None;
            }
            "select" => {
                self.end_option();
                self.select = None;
            }
            "option" => self.end_option(),
            "button" => {
                if let Some((fi, k, text)) = self.button.take() {
                    let text = clean(&text, MAX_LABEL);
                    if let Some(f) = self.field_mut((fi, k)) {
                        if !text.is_empty() {
                            f.label = text;
                        } else if f.label.is_empty() {
                            f.label = clean(&f.value, MAX_LABEL);
                        }
                    }
                }
            }
            "label" => {
                self.before.clear();
                if let Some((target, text, held)) = self.label.take() {
                    let text = clean(&text, MAX_LABEL).trim_end_matches(['*', ':']).trim().to_string();
                    if !text.is_empty() {
                        for at in held {
                            if let Some(f) = self.field_mut(at) {
                                if f.kind.shown() {
                                    f.label = text.clone();
                                }
                            }
                        }
                        if !target.is_empty() && self.for_labels.len() < 200 {
                            self.for_labels.push((target, text));
                        }
                    }
                }
            }
            "h1" | "h2" => {
                if let Some(h) = self.heading.take() {
                    self.page.heading = clean(&h, MAX_LABEL);
                }
            }
            _ => {}
        }
        if block(name) {
            self.paragraph();
        }
    }
}

/// Reads a page. Linear in its length; everything kept is capped (`MAX_*`).
pub fn parse(html: &str) -> Page {
    let mut r = Reader {
        page: Page::default(),
        form: None,
        in_form: false,
        before: String::new(),
        label: None,
        for_labels: Vec::new(),
        select: None,
        option: None,
        button: None,
        heading: None,
        head: false,
        noscript: 0,
        auto_submit: false,
        press: false,
        text_full: false,
        space: false,
    };
    let b = html.as_bytes();
    let mut i = 0;
    let mut text_from = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        // Text up to here.
        if text_from < i {
            let t = unescape(&html[text_from..i]);
            r.push_text(&t);
        }
        let rest = &b[i..];
        if rest.starts_with(b"<!--") {
            i = html[i + 4..].find("-->").map_or(b.len(), |p| i + 4 + p + 3);
            text_from = i;
            continue;
        }
        if rest.starts_with(b"<!") || rest.starts_with(b"<?") {
            i = b[i..].iter().position(|&c| c == b'>').map_or(b.len(), |p| i + p + 1);
            text_from = i;
            continue;
        }
        let Some((tag, after)) = read_tag(html, i + 1) else {
            // A '<' that starts no tag is text.
            text_from = i;
            i += 1;
            continue;
        };
        i = after;
        text_from = i;
        if tag.end {
            r.end(&tag.name);
            continue;
        }
        match tag.name.as_str() {
            "script" | "style" | "template" | "title" | "textarea" | "xmp" => {
                let stop = raw_end(html, i, &tag.name);
                let raw = &html[i..stop];
                match tag.name.as_str() {
                    "script" => {
                        r.page.scripts += 1;
                        if !tag.has("src") {
                            if r.page.redirect.is_none() {
                                r.page.redirect = script_redirect(raw);
                            }
                            if !r.auto_submit {
                                r.auto_submit = submits_on_load(raw);
                            }
                        }
                    }
                    "title" if r.page.title.is_empty() => r.page.title = clean(&unescape(raw), MAX_LABEL),
                    "textarea" => {
                        r.start(&tag);
                        let value = cap(unescape(raw), MAX_VALUE);
                        if let Some(f) = r.page.forms.get_mut(r.form.unwrap_or(usize::MAX)).and_then(|f| f.fields.last_mut()) {
                            if f.kind == Kind::Textarea {
                                f.value = value;
                            }
                        }
                    }
                    _ => {}
                }
                // Past the end tag.
                i = b[stop..].iter().position(|&c| c == b'>').map_or(b.len(), |p| stop + p + 1);
                text_from = i;
            }
            _ => r.start(&tag),
        }
    }
    if text_from < b.len() {
        let t = unescape(&html[text_from..]);
        r.push_text(&t);
    }
    r.end_option();
    if let Some(h) = r.heading.take() {
        r.page.heading = clean(&h, MAX_LABEL);
    }
    // `<label for=id>` names the field with that id.
    let for_labels = std::mem::take(&mut r.for_labels);
    let auto_submit = r.auto_submit;
    let mut page = r.page;
    for form in &mut page.forms {
        for f in &mut form.fields {
            if !f.id.is_empty() && f.kind.shown() {
                if let Some((_, text)) = for_labels.iter().find(|(id, _)| *id == f.id) {
                    f.label = text.clone();
                }
            }
        }
        merge_radios(form);
    }
    // The form a script submits on load: only when everything it carries is hidden (nothing to fill in or tick, and
    // no button anyone presses), no `onsubmit` script stands in the way, and the page has no button or action link of
    // its own (a page built around an "Accept and connect" press never sends anything by itself).
    page.auto_form = if auto_submit && !r.press {
        page.forms.iter().position(|f| {
            !f.onsubmit
                && !f.fields.is_empty()
                && f.fields.iter().all(|x| x.kind == Kind::Hidden && !x.disabled)
        })
    } else {
        None
    };
    page.text = page.text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
    page
}

/// Radio buttons of one name become one field with their labels as options, where the first of them was.
fn merge_radios(form: &mut Form) {
    let mut out: Vec<Field> = Vec::with_capacity(form.fields.len());
    for f in std::mem::take(&mut form.fields) {
        if f.kind != Kind::Radio {
            out.push(f);
            continue;
        }
        let label = if f.label.is_empty() { f.value.clone() } else { f.label.clone() };
        match out.iter_mut().find(|g| g.kind == Kind::Radio && g.name == f.name) {
            Some(group) => {
                if group.options.len() < MAX_OPTIONS {
                    group.options.push((f.value.clone(), label));
                    if f.checked {
                        group.selected = Some(group.options.len() - 1);
                    }
                    group.required |= f.required;
                }
            }
            None => {
                let mut group = f.clone();
                group.options = vec![(f.value.clone(), label)];
                group.selected = f.checked.then_some(0);
                group.label = String::new();
                out.push(group);
            }
        }
    }
    form.fields = out;
}

/// Why a page can't be signed in here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoForm {
    /// Scripts and no form a person can send: the page is a JavaScript app.
    NeedsScripts,
    /// No form at all, and no scripts: a page with links only (social login, "pay here", an error page).
    Nothing,
    /// The page's own form carries what it holds to an address the helper never sends to (`Session::allowed`);
    /// the person pressed nothing. Only a form a person pressed is refused as "Not sent".
    ElseWhere,
}

impl Form {
    /// Overwrites every value a person could have typed or chosen (`portal::wipe`): when the sheet closes and after
    /// the form was sent. Hidden values and buttons are the page's own and stay.
    pub fn wipe(&mut self) {
        for f in &mut self.fields {
            if f.kind.typed() {
                crate::portal::wipe(&mut f.value);
            }
        }
    }

    /// The form can be sent without JavaScript: an action that isn't a script, and a button that sends it (or, on a
    /// page without scripts, at least a field: a browser sends a single field with Enter).
    pub fn usable(&self, page_scripts: usize) -> bool {
        let scripted_action = self.action.as_deref().is_some_and(|a| a.trim_start().to_ascii_lowercase().starts_with("javascript:"));
        let submits = self.fields.iter().any(|f| matches!(f.kind, Kind::Submit { .. }) && !f.disabled);
        let shown = self.fields.iter().any(|f| f.kind.shown() && !f.disabled);
        !scripted_action && (submits || (shown && page_scripts == 0))
    }

    /// A field a person fills in or ticks, or a button: something to show.
    pub fn shown(&self) -> impl Iterator<Item = (usize, &Field)> {
        self.fields.iter().enumerate().filter(|(_, f)| (f.kind.shown() || matches!(f.kind, Kind::Submit { .. })) && !f.disabled)
    }
}

impl Page {
    /// The forms that can be sent here, in page order.
    pub fn usable(&self) -> Vec<usize> {
        (0..self.forms.len()).filter(|&i| self.forms[i].usable(self.scripts)).collect()
    }

    /// Why nothing can be signed in, when nothing can.
    pub fn no_form(&self) -> Option<NoForm> {
        if !self.usable().is_empty() {
            None
        } else if self.scripts > 0
            || self.forms.iter().any(|f| f.onsubmit || f.action.as_deref().is_some_and(|a| a.trim_start().to_ascii_lowercase().starts_with("javascript:")))
        {
            Some(NoForm::NeedsScripts)
        } else {
            Some(NoForm::Nothing)
        }
    }

    /// The best name for the page: its first heading, else its title.
    pub fn name(&self) -> &str {
        if !self.heading.is_empty() {
            &self.heading
        } else {
            &self.title
        }
    }
}

/// What a form sends, in order, as (name, value) pairs: HTML's rules for the successful controls. `submitter`: the
/// button pressed (its name and value are sent; an image button sends `name.x=0` and `name.y=0`).
pub fn form_data(form: &Form, submitter: Option<usize>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (k, f) in form.fields.iter().enumerate() {
        if f.disabled {
            continue;
        }
        match f.kind {
            Kind::Submit { image } if Some(k) == submitter => {
                if image {
                    let prefix = if f.name.is_empty() { String::new() } else { format!("{}.", f.name) };
                    out.push((format!("{prefix}x"), "0".into()));
                    out.push((format!("{prefix}y"), "0".into()));
                } else if !f.name.is_empty() {
                    out.push((f.name.clone(), f.value.clone()));
                }
            }
            _ if f.name.is_empty() => {}
            Kind::Submit { .. } | Kind::Other => {}
            Kind::Checkbox => {
                if f.checked {
                    out.push((f.name.clone(), f.value.clone()));
                }
            }
            Kind::Radio | Kind::Select => {
                if let Some((v, _)) = f.selected.and_then(|s| f.options.get(s)) {
                    out.push((f.name.clone(), v.clone()));
                }
            }
            Kind::Hidden | Kind::Text | Kind::Email | Kind::Number | Kind::Tel | Kind::Password | Kind::Textarea => {
                out.push((f.name.clone(), f.value.clone()));
            }
        }
    }
    out
}

/// `application/x-www-form-urlencoded`: every byte but letters, digits and `*-._` as `%XX`, a space as `+`.
pub fn urlencode(pairs: &[(String, String)]) -> String {
    fn enc(s: &str, out: &mut String) {
        for &b in s.as_bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => out.push(b as char),
                b' ' => out.push('+'),
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
    }
    let mut out = String::new();
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push('&');
        }
        enc(k, &mut out);
        out.push('=');
        enc(v, &mut out);
    }
    out
}

/// `multipart/form-data` with `boundary`: one part per pair. Quotes and line breaks in a name are escaped as browsers do.
pub fn multipart(pairs: &[(String, String)], boundary: &str) -> String {
    let mut out = String::new();
    for (k, v) in pairs {
        let name = k.replace('"', "%22").replace('\r', "%0D").replace('\n', "%0A");
        out.push_str(&format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{v}\r\n"));
    }
    out.push_str(&format!("--{boundary}--\r\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_decode_and_unknown_ones_stay() {
        assert_eq!(unescape("Gr&uuml;&szlig;e &amp; &#228;&#xE4; &lt;b&gt;"), "Grüße & ää <b>");
        assert_eq!(unescape("AT&T &bogus; &"), "AT&T &bogus; &");
        assert_eq!(unescape("&#0; &#xD800; &#99999999999;"), "\u{fffd} \u{fffd} &#99999999999;");
        assert_eq!(unescape("&#x110000;"), "\u{fffd}");
    }

    #[test]
    fn latin1_pages_read_as_latin1() {
        assert_eq!(decode(b"Gr\xfc\xdfe", "text/html; charset=ISO-8859-1"), "Grüße");
        assert_eq!(decode(b"<meta charset=\"iso-8859-1\">\xe4", ""), "<meta charset=\"iso-8859-1\">ä");
        assert_eq!(decode("Grüße".as_bytes(), "text/html; charset=utf-8"), "Grüße");
        assert_eq!(decode(b"\xff\xfe", "text/html"), "\u{fffd}\u{fffd}");
    }

    #[test]
    fn a_trivial_script_redirect_is_followed_and_nothing_built_at_run_time() {
        assert_eq!(script_redirect("window.location.href = \"/login?x=1\";"), Some("/login?x=1".into()));
        assert_eq!(script_redirect(" location.replace('http://192.0.2.1/a') "), Some("http://192.0.2.1/a".into()));
        assert_eq!(script_redirect("window.location='/p'"), Some("/p".into()));
        assert_eq!(script_redirect("location.href = base + '/x';"), None);
        assert_eq!(script_redirect("location.href = '/x' + q;"), None);
        assert_eq!(script_redirect("var a = 1;"), None);
    }

    #[test]
    fn interstitial_redirects_read_out_of_scripts_timers_and_onload() {
        // The hotel's "please wait" page: a timer with a literal address, in every spelling.
        assert_eq!(
            script_redirect("setTimeout(function(){ location.href = '/guest/gateway.html'; }, 250);"),
            Some("/guest/gateway.html".into())
        );
        assert_eq!(script_redirect("setTimeout(\"top.location='http://192.0.2.1/guest'\", 50);"), Some("http://192.0.2.1/guest".into()));
        assert_eq!(script_redirect("window.onload = function(){ document.location.href = \"/next\"; };"), Some("/next".into()));
        assert_eq!(script_redirect("location.replace (\"a/b\")"), Some("a/b".into()));
        assert_eq!(script_redirect("location.assign('/x');"), Some("/x".into()));
        // Built at run time: never followed.
        assert_eq!(script_redirect("location.href = '/x' + q;"), None);
        assert_eq!(script_redirect("location = base + '/x';"), None);
        assert_eq!(script_redirect("location.reload();"), None);
        // The "please wait" fixture of the hotel test: read out as written.
        let js = parse(include_str!("../testdata/portal/interstitial-js.html"));
        assert_eq!(js.redirect.as_deref(), Some("/guest/gateway.html"));
        // An <onload> handler goes the same way; a click handler is never taken for a navigation.
        let page = parse(r#"<frameset onload="top.location='http://a.example/'"></frameset>"#);
        assert_eq!(page.redirect.as_deref(), Some("http://a.example/"));
        let page = parse(r#"<a href="/x" onclick="location.href='/y'">x</a>"#);
        assert_eq!(page.redirect, None, "what a click does is not navigation");
    }

    #[test]
    fn a_meta_refresh_is_read_through_odd_spacing_quotes_and_case() {
        for (content, want) in [
            ("0; URL = 'https://portal.example.net:8443/guest/login'", "https://portal.example.net:8443/guest/login"),
            ("0;url=/x", "/x"),
            ("5; URL=&quot;/y&quot;", "/y"),
            ("0;URL =/z", "/z"),
            ("0 ; uRl =  /w ", "/w"),
        ] {
            let page = parse(&format!("<META HTTP-EQUIV=\"Refresh\" CONTENT=\"{content}\">"));
            assert_eq!(page.redirect.as_deref(), Some(want), "{content:?}");
        }
        assert_eq!(parse(r#"<meta http-equiv="refresh" content="0; curl=5">"#).redirect, None, "url must stand alone");
        assert_eq!(parse(include_str!("../testdata/portal/interstitial-meta.html")).redirect.as_deref(), Some("/guest/login.html"), "the odd-syntax fixture");
    }

    #[test]
    fn the_noscript_link_is_the_real_one_and_a_single_frame_is_seen() {
        let page = parse(
            r#"<script>location.href="/app";</script><noscript><p>Not redirected? <a href="/login">Sign in here</a>.</p></noscript>"#,
        );
        assert_eq!(page.noscript_link.as_deref(), Some("/login"), "preferred over the script");
        assert_eq!(page.redirect.as_deref(), Some("/app"));
        // A link outside <noscript> is one the person chooses: the helper never follows it.
        assert_eq!(parse(r#"<p><a href="/terms">terms</a></p>"#).noscript_link, None);
        assert_eq!(parse(r##"<noscript><a href="#top">top</a> <a href="/real">real</a></noscript>"##).noscript_link.as_deref(), Some("/real"));
        let frame = parse(r#"<html><body><iframe src="login.html"></iframe></body></html>"#);
        assert_eq!(frame.iframes, ["login.html"]);
        assert_eq!(parse(r#"<iframe src=a><iframe src=b>"#).iframes.len(), 2, "kept apart; only a single frame is followed");
        // The fixtures: the noscript link is the real one, the page that is one frame is seen.
        let noscript = parse(include_str!("../testdata/portal/interstitial-noscript.html"));
        assert_eq!(noscript.noscript_link.as_deref(), Some("/guest/login.html"), "preferred over the script's welcome page");
        assert_eq!(noscript.redirect.as_deref(), Some("/guest/welcome.html"));
        assert_eq!(parse(include_str!("../testdata/portal/interstitial-iframe.html")).iframes, ["login.html"]);
        let hostile = parse(include_str!("../testdata/portal/hostile-scheme.html"));
        assert_eq!(hostile.noscript_link.as_deref(), Some("file:///etc/shadow"), "read as written; the address rules refuse it later");
        assert_eq!(hostile.redirect.as_deref(), Some("file:///etc/passwd"));
        // A tracker's frame (the Google Tag Manager snippet) is not a page to load: it sits in <noscript>, one pixel
        // tall and hidden, and is never followed.
        let gtm = parse(include_str!("../testdata/portal/hostile-gtm.html"));
        assert!(gtm.iframes.is_empty(), "no tracker frame is taken for the page's own");
        assert_eq!(gtm.noscript_link.as_deref(), Some("https://www.enable-javascript.com/"), "read as written; `follow` keeps to the page's own site");
        assert_eq!(parse(r#"<iframe src=x width=0 height=0 style="display:none"><iframe src=y hidden><iframe src=z>"#).iframes, ["z"], "tiny and hidden frames are not it");
    }

    #[test]
    fn a_form_the_page_submits_on_load_is_the_one_with_only_hidden_fields() {
        // Cisco/Aruba-style: <body onload="document.forms[0].submit()">, everything hidden.
        let wait = parse(
            r#"<body onload="document.forms[0].submit();"><form method=post action=/guest/auth>
               <input type=hidden name=buttonClicked value=4><input type=hidden name=mac value=00:00:5E:00:53:7F></form></body>"#,
        );
        assert_eq!(wait.auto_form, Some(0));
        assert!(wait.usable().is_empty(), "nothing to fill in or press");
        // The same in a script, with the name or id of the form.
        let s = parse(r#"<form id=f><input type=hidden name=tok value=x></form><script>setTimeout(function(){ document.getElementById('f').submit(); }, 50);</script>"#);
        assert_eq!(s.auto_form, Some(0));
        // Not when there is something to fill in (the form is shown instead), a button, or an onsubmit script.
        let shown = parse(r#"<body onload="document.forms[0].submit()"><form><input name=room></form></body>"#);
        assert_eq!(shown.auto_form, None);
        assert_eq!(shown.usable(), [0], "shown for the person to fill in");
        assert_eq!(parse(r#"<form onsubmit="x()"><input type=hidden name=t></form><script>document.forms[0].submit()</script>"#).auto_form, None);
        assert_eq!(parse(r#"<form><input type=hidden name=t></form><script>var s = 'no hint';</script>"#).auto_form, None, "no submit on load");
        assert_eq!(parse(r#"<form action="javascript:x()"><input type=hidden name=t></form><script>document.forms[0].submit()</script>"#).auto_form, Some(0), "the action is refused later, not here");
        // The Cisco/Aruba-style fixture: the form with only hidden fields, and nothing shown.
        let wait = parse(include_str!("../testdata/portal/interstitial-wait.html"));
        assert_eq!(wait.auto_form, Some(0));
        assert!(wait.usable().is_empty());
        assert_eq!(wait.title, "Guest Access - Connecting...");
    }

    #[test]
    fn a_submit_that_only_runs_on_a_press_never_auto_submits() {
        // The click-through portal from the review: an "Accept and connect" link outside the all-hidden form, its
        // submit only in the click handler. Nothing may go out without the press.
        let accept = parse(include_str!("../testdata/portal/hostile-accept-button.html"));
        assert_eq!(accept.auto_form, None, "the submit runs on a press, not on load");
        assert!(accept.usable().is_empty());
        // The same without the press elements: still not on load.
        let click = parse(r#"<form id=f><input type=hidden name=accept value=1></form><script>document.getElementById('f').onclick = function(){ document.getElementById('f').submit(); };</script>"#);
        assert_eq!(click.auto_form, None);
        let named = parse(r#"<form><input type=hidden name=t></form><script>function go(){ document.forms[0].submit(); }</script>"#);
        assert_eq!(named.auto_form, None, "a named function only a press or a call runs");
        let binding = parse(r#"<form id=f><input type=hidden name=t></form><script>$('#f').submit(function(){ return true; });</script>"#);
        assert_eq!(binding.auto_form, None, "submit(function…) binds a handler, it does not submit");
        // What runs on load still counts, with the load context spelled in every way.
        for code in [
            "document.forms[0].submit();",
            "window.onload = function(){ document.forms[0].submit(); };",
            "document.addEventListener('DOMContentLoaded', function(){ document.forms[0].submit(); });",
            "setTimeout(function(){ document.forms[0].submit(); }, 250);",
            "$(function(){ document.forms[0].submit(); });",
        ] {
            let page = parse(&format!("<form><input type=hidden name=t></form><script>{code}</script>"));
            assert_eq!(page.auto_form, Some(0), "{code}");
        }
        // A button of its own (here outside the form): the page is about a press.
        let button = parse(r#"<form><input type=hidden name=t></form><button>Accept and connect</button><script>document.forms[0].submit()</script>"#);
        assert_eq!(button.auto_form, None);
    }

    #[test]
    fn fields_get_their_labels_from_wherever_the_page_put_them() {
        let page = parse(
            r#"<form method=post action="/login">
              <label for="room">Room number</label> <input id=room name=room required>
              Surname: <input name="surname">
              <label><input type=checkbox name=terms value=yes required> I accept the <a href=/t>terms</a></label>
              <input type=email name=mail placeholder="E-mail address">
              <input aria-label="Voucher code" name=code>
              <input type=hidden name=token value="a&amp;b">
              <button type=submit name=go value=1>Connect <b>now</b></button>
            </form>"#,
        );
        let f = &page.forms[0];
        let labels: Vec<(&str, &str)> = f.fields.iter().map(|f| (f.name.as_str(), f.label.as_str())).collect();
        assert_eq!(
            labels,
            [("room", "Room number"), ("surname", "Surname"), ("terms", "I accept the terms"), ("mail", "E-mail address"), ("code", "Voucher code"), ("token", ""), ("go", "Connect now")]
        );
        assert!(f.fields[0].required && f.fields[2].required);
        assert_eq!(f.fields[5].value, "a&b");
        assert_eq!(f.method, Method::Post);
        assert_eq!(f.action.as_deref(), Some("/login"));
        assert!(f.usable(0));
    }

    #[test]
    fn selects_and_radio_groups_are_choices() {
        let page = parse(
            r#"<form><select name=plan><option value=free>Free, 1 hour<option value=day selected>Day pass</select>
               <input type=radio name=lang value=en id=en><label for=en>English</label>
               <input type=radio name=lang value=de id=de checked><label for=de>Deutsch</label>
               <select name=empty></select><input type=submit></form>"#,
        );
        let f = &page.forms[0];
        assert_eq!(f.fields[0].options, [("free".to_string(), "Free, 1 hour".to_string()), ("day".into(), "Day pass".into())]);
        assert_eq!(f.fields[0].selected, Some(1));
        assert_eq!(f.fields[1].kind, Kind::Radio);
        assert_eq!(f.fields[1].options, [("en".to_string(), "English".to_string()), ("de".into(), "Deutsch".into())]);
        assert_eq!(f.fields[1].selected, Some(1));
        assert_eq!(f.fields[2].selected, None);
        assert_eq!(f.fields[3].label, "", "no words: the helper names it");
        assert_eq!(form_data(f, Some(3)), [("plan".to_string(), "day".to_string()), ("lang".into(), "de".into())]);
    }

    #[test]
    fn what_a_form_sends_follows_the_browsers_rules() {
        let page = parse(
            r#"<form><input type=hidden name=h value=1><input name=t value="a b"><input type=checkbox name=c>
               <input type=checkbox name=d checked value=x><input name=off disabled value=z><input type=text value=noname>
               <input type=submit name=a value=A><input type=image name=img src=go.png><input type=file name=f></form>"#,
        );
        let f = &page.forms[0];
        let a = f.fields.iter().position(|x| x.name == "a");
        let img = f.fields.iter().position(|x| x.name == "img");
        assert_eq!(form_data(f, a), [("h", "1"), ("t", "a b"), ("d", "x"), ("a", "A")].map(|(k, v)| (k.to_string(), v.to_string())));
        assert_eq!(form_data(f, img).last().unwrap(), &("img.y".to_string(), "0".to_string()));
        assert_eq!(urlencode(&form_data(f, a)), "h=1&t=a+b&d=x&a=A");
        assert_eq!(urlencode(&[("n".into(), "Müller & Söhne=1%".into())]), "n=M%C3%BCller+%26+S%C3%B6hne%3D1%25");
        let m = multipart(&[("x\"y".into(), "1".into())], "B");
        assert_eq!(m, "--B\r\nContent-Disposition: form-data; name=\"x%22y\"\r\n\r\n1\r\n--B--\r\n");
    }

    #[test]
    fn the_page_text_title_and_heading_are_plain() {
        let page = parse(
            "<html><head><title> Hotel &amp; Spa </title><style>p{x}</style><script>var s='<p>no</p>';</script></head>\
             <body><h1>Welcome to <i>Seeblick</i></h1><p>By using this service you agree\n   to the terms.</p><!-- <p>hidden</p> -->\
             <ul><li>No streaming</li><li>Fair use</li></ul>\u{202e}evil</body></html>",
        );
        assert_eq!(page.title, "Hotel & Spa");
        assert_eq!(page.heading, "Welcome to Seeblick");
        assert_eq!(page.text, "Welcome to Seeblick\nBy using this service you agree to the terms.\nNo streaming\nFair use\nevil");
        assert_eq!(page.scripts, 1);
        assert_eq!(page.name(), "Welcome to Seeblick");
    }

    #[test]
    fn a_page_says_why_it_cant_be_signed_in() {
        let app = parse(r#"<html><body><div id="app"></div><script src="/app.js"></script><noscript>Please enable JavaScript.</noscript></body></html>"#);
        assert_eq!(app.no_form(), Some(NoForm::NeedsScripts));
        assert_eq!(app.text, "Please enable JavaScript.");
        let links = parse(r#"<p>Sign in with <a href="https://facebook.example/">Facebook</a></p>"#);
        assert_eq!(links.no_form(), Some(NoForm::Nothing));
        let js_action = parse(r#"<form action="javascript:login()"><input name=a><input type=submit></form>"#);
        assert_eq!(js_action.no_form(), Some(NoForm::NeedsScripts), "an onsubmit-less form whose action is a script");
        let button_only = parse(r#"<form><input name=room><button type=button onclick="go()">Go</button></form><script>function go(){}</script>"#);
        assert_eq!(button_only.no_form(), Some(NoForm::NeedsScripts), "no button that sends it, scripts on the page");
        let enter = parse(r#"<form action=/v><input name=voucher></form>"#);
        assert_eq!(enter.usable(), [0], "no scripts: Enter sends a single field");
        let noscript_form = parse(r#"<script src=a.js></script><noscript><form action=/ok><input type=submit value=Accept></form></noscript>"#);
        assert_eq!(noscript_form.usable(), [0], "the form a browser without JavaScript shows");
        let refresh = parse(r#"<meta http-equiv="Refresh" content="0; URL='/splash.html'">"#);
        assert_eq!(refresh.redirect.as_deref(), Some("/splash.html"));
    }

    /// A small deterministic generator for the garbage tests (no crate for it).
    fn noise(seed: &mut u64, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n);
        let alphabet = b"<>/=\"' \n\tabcformiputsel&#;x!-?";
        for _ in 0..n {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            let r = *seed;
            out.push(if r.is_multiple_of(5) { (r >> 8) as u8 } else { alphabet[(r >> 16) as usize % alphabet.len()] });
        }
        out
    }

    #[test]
    fn garbage_never_panics_and_stays_within_its_limits() {
        let mut seed = 0x9e3779b97f4a7c15u64;
        for round in 0..400 {
            let bytes = noise(&mut seed, 1 + round * 37 % 5000);
            let page = parse(&decode(&bytes, ""));
            assert!(page.forms.len() <= MAX_FORMS);
            assert!(page.forms.iter().all(|f| f.fields.len() <= MAX_FIELDS));
            assert!(page.text.chars().count() <= MAX_TEXT);
        }
        // Cut-off tags, attributes and entities at the very end.
        for s in ["<", "<f", "<form", "<form a", "<form a=", "<form a=\"", "<input name='x", "&", "&#", "&#x", "<!--", "<!", "<script>", "</", "<select><option", "<textarea>abc"] {
            let _ = parse(s);
        }
    }

    #[test]
    fn huge_and_deep_pages_are_one_pass_and_bounded() {
        let deep = "<div>".repeat(200_000) + "<form><input type=submit></form>" + &"</div>".repeat(200_000);
        let t = std::time::Instant::now();
        let page = parse(&deep);
        assert_eq!(page.usable(), [0]);
        let many = "<form><input name=a>".repeat(5000) + &"<input name=b>".repeat(5000);
        let page = parse(&many);
        assert_eq!(page.forms.len(), 1, "a form inside a form is ignored, as browsers do");
        assert_eq!(page.forms[0].fields.len(), MAX_FIELDS);
        let long = format!("<p>{}</p><form><input name=x value=\"{}\"></form>", "word ".repeat(100_000), "v".repeat(100_000));
        let page = parse(&long);
        assert!((MAX_TEXT - 1..=MAX_TEXT).contains(&page.text.chars().count()), "{}", page.text.chars().count());
        assert_eq!(page.forms[0].fields[0].value.len(), MAX_VALUE);
        let attrs = format!("<input {}>", "a=1 ".repeat(100_000));
        let _ = parse(&attrs);
        let options = format!("<form><select name=s>{}</select></form>", "<option>x".repeat(10_000));
        assert_eq!(parse(&options).forms[0].fields[0].options.len(), MAX_OPTIONS);
        assert!(t.elapsed() < std::time::Duration::from_secs(5), "{:?}", t.elapsed());
    }
}
