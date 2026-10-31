//! Shared file-name and disc-sheet rules; no storage enumeration or host services.
pub(crate) fn sheet_references(kind: &str, text: &str) -> Vec<String> {
    let lines = text.lines().map(|l| l.trim().trim_start_matches('\u{feff}')).filter(|l| !l.is_empty());
    match kind {
        "m3u" => lines.filter(|l| !l.starts_with('#')).map(str::to_string).collect(),
        "cue" => lines
            .filter(|l| l.len() > 5 && l.is_char_boundary(5) && l[..5].eq_ignore_ascii_case("FILE "))
            .filter_map(|l| {
                let rest = l[5..].trim();
                match rest.strip_prefix('"') {
                    Some(quoted) => quoted.split('"').next().map(str::to_string),
                    // Unquoted: everything but the format word at the end ("FILE track01.bin BINARY").
                    None => rest.rsplit_once(char::is_whitespace).map(|(file, _)| file.trim().to_string()),
                }
            })
            .collect(),
        // "1 0 4 2352 track01.bin 0": the first line is the track count; the name may be quoted and hold spaces.
        "gdi" => lines
            .skip(1)
            .filter_map(|l| {
                let mut rest = l;
                for _ in 0..4 {
                    rest = rest.split_once(char::is_whitespace)?.1.trim_start();
                }
                match rest.strip_prefix('"') {
                    Some(quoted) => quoted.split('"').next().map(str::to_string),
                    None => rest.split_whitespace().next().map(str::to_string),
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Title as people say it: release tags like "(World) (Rev 1)" and "[!]" dropped, and a trailing
/// article moved to the front ("Legend of Zelda, The - Link's Awakening" → "The Legend of Zelda - …").
pub fn display_name(stem: &str) -> String {
    let stem = strip_inner_ext(stem);
    // "Legend of Zelda,The-Oracle of Ages": the article after a comma with no space, and a dash with none either.
    let glued = fix_glued_article(stem);
    let stem = glued.as_str();
    let mut out = String::new();
    let mut depth = 0;
    for ch in stem.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    let name = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = split_version(&name).0.to_string();
    // A disc named by its serial ("SLES_556.73.Something"): the serial in front goes, the title stays (`disc_serial`).
    let name = crate::disc_serial::clean_stem(&name);
    let (head, tail) = match name.split_once(" - ") {
        Some((h, t)) => (h.to_string(), format!(" - {t}")),
        None => (name.clone(), String::new()),
    };
    for article in ["The", "A", "An", "Die", "Der", "Das"] {
        if let Some(base) = head.strip_suffix(&format!(", {article}")) {
            return format!("{article} {base}{tail}");
        }
    }
    if name.is_empty() { stem.to_string() } else { name }
}

/// "Zelda,The-Oracle" → "Zelda, The - Oracle": a trailing article written without the spaces around it. Names that
/// don't have one are returned as they are (the hyphen of "Spider-Man" stays).
fn fix_glued_article(stem: &str) -> String {
    for article in ["The", "A", "An"] {
        for sep in [",", ", "] {
            let Some(at) = stem.find(&format!("{sep}{article}")).filter(|&at| at > 0) else { continue };
            let after = &stem[at + sep.len() + article.len()..];
            let glued_dash = after.starts_with('-');
            let ends_there = after.is_empty() || after.starts_with(" -") || after.starts_with(" (") || after.starts_with(" [");
            // Something must be glued, or the name is fine as it is; and the article must really end there ("Anna").
            if !(sep == "," || glued_dash) || !(glued_dash || ends_there) {
                continue;
            }
            let rest = if glued_dash { format!(" - {}", after[1..].trim_start()) } else { after.to_string() };
            return format!("{}, {article}{rest}", &stem[..at]);
        }
    }
    stem.to_string()
}

/// Small words that stay lower case in a title made by [`title_case`], unless they are its first word or the first
/// after " - " or ":".
const TITLE_SMALL_WORDS: &[&str] = &["a", "an", "and", "as", "at", "by", "for", "from", "in", "of", "on", "or", "the", "to", "vs", "with"];
/// Roman numerals, upper case in a title made by [`title_case`] ("Super Mario Land II").
const TITLE_ROMAN: &[&str] = &["i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x"];

/// A name for the shelf, cased when the file name had no capital letter at all: "libbet and the magic floor" is
/// "Libbet and the Magic Floor", "concentration room" is "Concentration Room". A name with any capital letter stays as
/// it is ("Super mario" is not changed), and so does a word with a digit ("sf2ce", "2x"). Only the display uses this;
/// the matching of games keeps the name as the file gives it (`display_name`).
pub fn title_case(name: &str) -> String {
    if name.chars().any(char::is_uppercase) {
        return name.to_string();
    }
    let mut words = Vec::new();
    let mut phrase_start = true;
    for token in name.split_whitespace() {
        let bare = token.trim_matches(|c: char| !c.is_alphanumeric());
        let cased = if token.chars().any(|c| c.is_ascii_digit()) {
            token.to_string()
        } else if !phrase_start && TITLE_SMALL_WORDS.contains(&bare) {
            token.to_string()
        } else if TITLE_ROMAN.contains(&bare) {
            token.to_uppercase()
        } else {
            capitalise(token)
        };
        phrase_start = token == "-" || token.ends_with(':');
        words.push(cased);
    }
    words.join(" ")
}

/// `token` with its first letter capitalised, and the first letter after each hyphen ("spider-man" → "Spider-Man").
fn capitalise(token: &str) -> String {
    let mut out = String::with_capacity(token.len());
    let mut word_start = true;
    for c in token.chars() {
        if word_start && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            word_start = false;
        } else {
            out.push(c);
            if c == '-' {
                word_start = true;
            } else if c.is_alphabetic() {
                word_start = false;
            }
        }
    }
    out
}

/// A version at the end of a name, the way homebrew and scene files carry it: "Pipes.v1.02" is ("Pipes",
/// Some("v1.02")). Taken off after a dot, underscore or dash ("_v1.2", "-v3"), and after a space when it has a dot
/// ("Game V1.0") or a small v ("Game v2"); "Robo V2" (a capital V and a whole number after a space) may be the title
/// and stays, and a name that is nothing but a version stays as it is.
pub fn split_version(name: &str) -> (&str, Option<&str>) {
    let Some(v) = name.rfind(['v', 'V']) else { return (name, None) };
    let number = &name[v + 1..];
    let digits_and_dots = !number.is_empty()
        && number.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    let Some(sep) = name[..v].chars().next_back() else { return (name, None) };
    let taken = digits_and_dots
        && match sep {
            '.' | '_' | '-' => true,
            ' ' => number.contains('.') || name[v..].starts_with('v'),
            _ => false,
        };
    let head = name[..v].trim_end_matches([' ', '.', '_', '-']);
    if !taken || !head.chars().any(char::is_alphanumeric) {
        return (name, None);
    }
    (head, Some(&name[v..]))
}

/// What `display_name` leaves out of a name, in the order it stands: the version ("v1.02") and the tags, each with
/// its brackets ("(Europe)", "[b1]"). Two games of a system with the same name get back what tells them apart
/// (`tell_apart`).
pub fn name_extras(stem: &str) -> Vec<String> {
    let stem = strip_inner_ext(stem);
    let (mut tags, mut plain) = (Vec::new(), String::new());
    let (mut depth, mut tag) = (0, String::new());
    for ch in stem.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                tag.push(ch);
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                tag.push(ch);
                if depth == 0 {
                    tags.push(std::mem::take(&mut tag));
                }
            }
            _ if depth == 0 => plain.push(ch),
            _ => tag.push(ch),
        }
    }
    let plain = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: Vec<String> = split_version(&plain).1.map(str::to_string).into_iter().collect();
    out.extend(tags.into_iter().map(|t| t.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|t| t.len() > 2));
    out
}

pub fn strip_inner_ext(stem: &str) -> &str {
    [".nkit", ".NKIT", ".p8"].iter().find_map(|e| stem.strip_suffix(e)).unwrap_or(stem)
}
