//! Facts in a ROM's file name (docs/CURATOR.md, layer C): regions, languages, a fan translation, the
//! revision and dump flags, for Curator's Language facet and the Versions page.
//!
//! Understands No-Intro names ("Game (USA, Europe) (En,Fr,De) (Rev 1)"), GoodTools codes ("Game (U) [!]",
//! "(JU)", "[T+Ger]", "[h1]") and the usual translation-patch tags ("[T-En by Aeon Genesis]",
//! "[T+Eng1.02_Spinner8]", "(Translated En)"). Pure string work, no allocation-heavy parsing: cheap enough
//! to run over a whole library on the index thread.
//!
//! Matching a file to a game (the title part) is `gamedb`'s and `covers`' job; this module only reads the
//! (…) and […] groups they drop.

/// What a file name says beyond the title.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RomFacts {
    /// Regions in the order the name gives them: "usa", "europe", "japan", "world", "germany" ...
    pub regions: Vec<&'static str>,
    /// Languages the name lists, ISO 639-1 ("en", "fr", "de"), in its order. Empty when it lists none (most
    /// single-language releases); see [`RomFacts::languages_or_implied`].
    pub languages: Vec<&'static str>,
    /// A fan translation patch.
    pub translation: Option<Translation>,
    /// "Rev 1", "Rev A", "v1.1".
    pub revision: Option<String>,
    pub flags: Vec<Flag>,
}

/// A fan translation patch named in the file name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Translation {
    /// ISO 639-1 code of the language it translates into ("en"), or the name's own code when unknown.
    pub language: String,
    /// GoodTools mark a translation as current ("T+") or as superseded by a newer one ("T-"). No-Intro style
    /// "[T-En by …]" uses "T-" for every translation, so this is only a hint.
    pub current: bool,
    /// Who made it, when the name says ("Aeon Genesis").
    pub by: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Flag {
    /// "[!]": a verified good dump.
    Verified,
    Beta,
    Proto,
    Demo,
    /// "(Unl)": unlicensed.
    Unlicensed,
    Pirate,
    /// "[h…]", "(Hack)".
    Hack,
    /// "[b…]": a bad dump.
    BadDump,
    /// "[a…]": an alternative dump.
    Alternate,
    /// "[f…]": fixed to run on emulators or hardware.
    Fixed,
    /// "(Virtual Console)", "(Switch Online)", "(Classic Mini)" ...: taken from a re-release.
    Rerelease,
    /// "(Aftermarket)", "(Homebrew)".
    Homebrew,
}

impl RomFacts {
    /// The languages a player gets: a fan translation's language first, then the languages the name lists,
    /// else the one its region implies ("Japan" -> ja, "USA" -> en). Empty when nothing says (Europe, World).
    pub fn languages_or_implied(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if let Some(t) = &self.translation {
            out.push(t.language.clone());
        }
        let listed: Vec<&str> = if self.languages.is_empty() {
            self.regions.iter().filter_map(|r| region_language(r)).collect()
        } else {
            self.languages.clone()
        };
        for l in listed {
            if !out.iter().any(|x| x == l) {
                out.push(l.to_string());
            }
        }
        out
    }

    /// Whether a player reading `lang` (ISO 639-1) can read this file's text, as far as the name tells:
    /// `Some(true)` / `Some(false)`, `None` when the name doesn't say.
    pub fn readable_in(&self, lang: &str) -> Option<bool> {
        let langs = self.languages_or_implied();
        (!langs.is_empty()).then(|| langs.iter().any(|l| l == lang))
    }
}

impl RomFacts {
    /// "USA, Europe · English, French" or "Japan · English (fan translation)", for the Versions page; empty when
    /// the name says nothing.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.regions.is_empty() {
            parts.push(self.regions.iter().map(|r| region_name(r)).collect::<Vec<_>>().join(", "));
        }
        match &self.translation {
            Some(t) => parts.push(format!("{} (fan translation)", language_name(&t.language))),
            None => {
                let codes = self.languages_or_implied();
                let langs: Vec<&str> = codes.iter().map(|l| language_name(l)).collect();
                if !langs.is_empty() {
                    parts.push(langs.join(", "));
                }
            }
        }
        parts.join(" · ")
    }
}

/// "USA", "UK", "Hong Kong" for the region ids of [`RomFacts::regions`].
pub fn region_name(id: &str) -> String {
    match id {
        "usa" => "USA".to_string(),
        "uk" => "UK".to_string(),
        other => other.split(' ').map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        }).collect::<Vec<_>>().join(" "),
    }
}

/// "English" for "en"; the code itself for a language without a name here.
pub fn language_name(code: &str) -> &str {
    match code {
        "en" => "English",
        "ja" => "Japanese",
        "fr" => "French",
        "de" => "German",
        "es" => "Spanish",
        "it" => "Italian",
        "nl" => "Dutch",
        "pt" => "Portuguese",
        "sv" => "Swedish",
        "no" => "Norwegian",
        "da" => "Danish",
        "fi" => "Finnish",
        "pl" => "Polish",
        "ru" => "Russian",
        "ko" => "Korean",
        "zh" => "Chinese",
        "el" => "Greek",
        "tr" => "Turkish",
        "ar" => "Arabic",
        "he" => "Hebrew",
        "hu" => "Hungarian",
        "cs" => "Czech",
        "ca" => "Catalan",
        "ro" => "Romanian",
        "hr" => "Croatian",
        "id" => "Indonesian",
        "vi" => "Vietnamese",
        "th" => "Thai",
        other => other,
    }
}

/// The groups of a name: ("(", "USA, Europe"), ("[", "!") ...
fn groups(name: &str) -> Vec<(char, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    let mut open = ' ';
    for (i, c) in name.char_indices() {
        match c {
            '(' | '[' if start.is_none() => {
                start = Some(i + 1);
                open = c;
            }
            ')' | ']' => {
                if let Some(s) = start.take() {
                    if (open == '(') == (c == ')') {
                        out.push((open, name[s..i].trim()));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

const REGIONS: [(&str, &str); 30] = [
    ("usa", "usa"),
    ("us", "usa"),
    ("u", "usa"),
    ("america", "usa"),
    ("canada", "canada"),
    ("europe", "europe"),
    ("eu", "europe"),
    ("e", "europe"),
    ("japan", "japan"),
    ("jp", "japan"),
    ("j", "japan"),
    ("world", "world"),
    ("w", "world"),
    ("germany", "germany"),
    ("g", "germany"),
    ("france", "france"),
    ("f", "france"),
    ("spain", "spain"),
    ("s", "spain"),
    ("italy", "italy"),
    ("i", "italy"),
    ("uk", "uk"),
    ("australia", "australia"),
    ("a", "australia"),
    ("korea", "korea"),
    ("k", "korea"),
    ("china", "china"),
    ("c", "china"),
    ("brazil", "brazil"),
    ("b", "brazil"),
];
/// Regions only No-Intro spells out (no one-letter GoodTools code).
const MORE_REGIONS: [&str; 10] = ["netherlands", "sweden", "russia", "taiwan", "hong kong", "asia", "scandinavia", "portugal", "denmark", "norway"];

fn region(word: &str) -> Option<&'static str> {
    let w = word.trim().to_ascii_lowercase();
    REGIONS
        .iter()
        .find(|(n, _)| *n == w)
        .map(|(_, r)| *r)
        .or_else(|| MORE_REGIONS.iter().find(|r| **r == w).copied())
}

/// GoodTools packs several regions into one code: "JU", "UE", "JUE".
fn packed_regions(code: &str) -> Option<Vec<&'static str>> {
    let ok = (2..=3).contains(&code.len()) && code.chars().all(|c| "JUEWGFSIAKCB".contains(c));
    ok.then(|| code.chars().filter_map(|c| region(&c.to_string())).collect())
}

/// The language a region implies when the name lists none; Europe and World say nothing.
fn region_language(region: &str) -> Option<&'static str> {
    Some(match region {
        "usa" | "uk" | "australia" | "canada" => "en",
        "japan" => "ja",
        "germany" => "de",
        "france" => "fr",
        "spain" => "es",
        "italy" => "it",
        "korea" => "ko",
        "china" | "taiwan" | "hong kong" => "zh",
        "brazil" | "portugal" => "pt",
        "netherlands" => "nl",
        "sweden" => "sv",
        "russia" => "ru",
        "denmark" => "da",
        "norway" => "no",
        _ => return None,
    })
}

/// A language code as No-Intro ("En", "De"), GoodTools translations ("Eng", "Ger", "Bra") or in words
/// ("English") write it -> ISO 639-1.
fn language(code: &str) -> Option<&'static str> {
    let c = code.trim().to_ascii_lowercase();
    Some(match c.as_str() {
        "en" | "eng" | "english" => "en",
        "fr" | "fre" | "fra" | "french" => "fr",
        "de" | "ger" | "deu" | "german" => "de",
        "es" | "spa" | "esp" | "spanish" => "es",
        "it" | "ita" | "italian" => "it",
        "nl" | "dut" | "nld" | "dutch" => "nl",
        "pt" | "por" | "bra" | "portuguese" | "brazilian" => "pt",
        "sv" | "swe" | "swedish" => "sv",
        "no" | "nor" | "norwegian" => "no",
        "da" | "dan" | "danish" => "da",
        "fi" | "fin" | "finnish" => "fi",
        "pl" | "pol" | "polish" => "pl",
        "ru" | "rus" | "russian" => "ru",
        "ja" | "jap" | "jpn" | "japanese" => "ja",
        "ko" | "kor" | "korean" => "ko",
        "zh" | "chi" | "zho" | "chinese" => "zh",
        "el" | "gre" | "greek" => "el",
        "tr" | "tur" | "turkish" => "tr",
        "ar" | "ara" | "arabic" => "ar",
        "he" | "heb" | "hebrew" => "he",
        "hu" | "hun" | "hungarian" => "hu",
        "cs" | "cze" | "czech" => "cs",
        "ca" | "cat" | "catalan" => "ca",
        "ro" | "rom" | "romanian" => "ro",
        "hr" | "cro" | "croatian" => "hr",
        "id" | "ind" | "indonesian" => "id",
        "vi" | "vie" | "vietnamese" => "vi",
        "th" | "tha" | "thai" => "th",
        _ => return None,
    })
}

/// "T+Eng1.02_Spinner8", "T-En by Aeon Genesis", "T+Ger" -> the translation.
fn translation(tag: &str) -> Option<Translation> {
    let (current, rest) = if let Some(rest) = tag.strip_prefix("Translated ") {
        (true, rest)
    } else if let Some(rest) = tag.strip_prefix("T+") {
        (true, rest)
    } else {
        (false, tag.strip_prefix("T-")?)
    };
    let code: String = rest.chars().take_while(char::is_ascii_alphabetic).collect();
    let lang = language(&code)?;
    let after = rest[code.len()..].trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');
    let by = after
        .strip_prefix("by ")
        .or_else(|| after.strip_prefix('_'))
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty());
    Some(Translation {
        language: lang.to_string(),
        current,
        by,
    })
}

/// "Rev 1", "Rev A", "v1.1", "V1.0", "PRG1".
fn revision(tag: &str) -> Option<String> {
    let t = tag.trim();
    let lower = t.to_ascii_lowercase();
    let is_rev = lower.strip_prefix("rev ").is_some_and(|r| !r.is_empty() && r.len() <= 4)
        || (lower.starts_with('v') && lower[1..].chars().next().is_some_and(|c| c.is_ascii_digit()) && lower[1..].chars().all(|c| c.is_ascii_digit() || c == '.'))
        || (lower.starts_with("prg") && lower[3..].chars().all(|c| c.is_ascii_digit()) && lower.len() > 3);
    is_rev.then(|| t.to_string())
}

fn flag(open: char, tag: &str) -> Option<Flag> {
    let lower = tag.to_ascii_lowercase();
    if open == '[' {
        let head = lower.chars().next()?;
        let rest_is_number = lower[1..].chars().all(|c| c.is_ascii_digit() || c == '+' || c == ' ');
        return match (head, rest_is_number) {
            ('!', _) if lower == "!" => Some(Flag::Verified),
            ('h', true) => Some(Flag::Hack),
            ('b', true) => Some(Flag::BadDump),
            ('a', true) => Some(Flag::Alternate),
            ('f', true) => Some(Flag::Fixed),
            ('p', true) => Some(Flag::Pirate),
            _ => None,
        };
    }
    let starts = |p: &str| lower == p || lower.starts_with(&format!("{p} "));
    Some(if starts("beta") {
        Flag::Beta
    } else if starts("proto") || starts("prototype") {
        Flag::Proto
    } else if starts("demo") || starts("sample") || starts("kiosk") {
        Flag::Demo
    } else if lower == "unl" || lower == "unlicensed" {
        Flag::Unlicensed
    } else if lower == "pirate" {
        Flag::Pirate
    } else if lower == "hack" || lower.ends_with(" hack") {
        Flag::Hack
    } else if lower == "aftermarket" || lower == "homebrew" {
        Flag::Homebrew
    } else if ["virtual console", "switch online", "classic mini", "evercade", "nintendo switch online", "gamecube edition"]
        .iter()
        .any(|r| lower.contains(r))
    {
        Flag::Rerelease
    } else {
        return None;
    })
}

/// Reads the facts of a ROM file name (with or without its extension).
pub fn parse(name: &str) -> RomFacts {
    let mut f = RomFacts::default();
    for (open, tag) in groups(name) {
        if tag.is_empty() {
            continue;
        }
        if let Some(t) = translation(tag) {
            f.translation.get_or_insert(t);
            continue;
        }
        if let Some(r) = revision(tag) {
            f.revision.get_or_insert(r);
            continue;
        }
        if let Some(fl) = flag(open, tag) {
            if !f.flags.contains(&fl) {
                f.flags.push(fl);
            }
            continue;
        }
        if open != '(' {
            continue;
        }
        let parts: Vec<&str> = tag.split([',', '+']).map(str::trim).filter(|p| !p.is_empty()).collect();
        // "(USA, Europe)", "(U)"
        if let Some(regions) = parts.iter().map(|p| region(p)).collect::<Option<Vec<_>>>() {
            for r in regions {
                if !f.regions.contains(&r) {
                    f.regions.push(r);
                }
            }
            continue;
        }
        // "(JU)", "(UE)"
        if parts.len() == 1 {
            if let Some(rs) = packed_regions(parts[0]) {
                f.regions.extend(rs.into_iter().filter(|r| !f.regions.contains(r)).collect::<Vec<_>>());
                continue;
            }
        }
        // "(En,Fr,De)", "(En+Ja)": only two-letter codes, so "(Disc 1)" or "(Beta)" never count.
        if parts.iter().all(|p| p.len() == 2) {
            if let Some(langs) = parts.iter().map(|p| language(p)).collect::<Option<Vec<_>>>() {
                for l in langs {
                    if !f.languages.contains(&l) {
                        f.languages.push(l);
                    }
                }
            }
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_intro_regions_languages_and_revisions() {
        let f = parse("Legend of Zelda, The - The Minish Cap (Europe) (En,Fr,De,Es,It).gba");
        assert_eq!(f.regions, ["europe"]);
        assert_eq!(f.languages, ["en", "fr", "de", "es", "it"]);
        assert_eq!(f.languages_or_implied(), ["en", "fr", "de", "es", "it"]);
        assert_eq!(f.readable_in("de"), Some(true));
        let f = parse("Super Mario Land 2 - 6 Golden Coins (USA, Europe) (Rev 2)");
        assert_eq!(f.regions, ["usa", "europe"]);
        assert!(f.languages.is_empty());
        assert_eq!(f.revision.as_deref(), Some("Rev 2"));
        assert_eq!(f.languages_or_implied(), ["en"], "USA implies English, Europe says nothing");
        let f = parse("Chrono Trigger (Japan)");
        assert_eq!(f.languages_or_implied(), ["ja"]);
        assert_eq!(f.readable_in("en"), Some(false));
        assert_eq!(parse("Tetris (World) (Rev 1)").readable_in("en"), None, "World says nothing");
        assert_eq!(parse("Pokemon - Version Rouge (France) (SGB Enhanced)").languages_or_implied(), ["fr"]);
        assert_eq!(parse("Game (Japan) (v1.1)").revision.as_deref(), Some("v1.1"));
        assert_eq!(parse("Mother 1+2 (Japan)").regions, ["japan"], "a + in the title is not a language list");
    }

    #[test]
    fn goodtools_codes_and_dump_flags() {
        let f = parse("Super Metroid (JU) [!].smc");
        assert_eq!(f.regions, ["japan", "usa"]);
        assert_eq!(f.flags, [Flag::Verified]);
        let f = parse("Contra (U) [h1][b2]");
        assert_eq!(f.regions, ["usa"]);
        assert_eq!(f.flags, [Flag::Hack, Flag::BadDump]);
        assert_eq!(parse("0902 - MegaMan & Bass (U)(Venom)").regions, ["usa"]);
        assert_eq!(parse("Game (E) [a1]").flags, [Flag::Alternate]);
    }

    #[test]
    fn translation_patches() {
        let f = parse("Mother 3 (Japan) [T-En by Tomato]");
        let t = f.translation.as_ref().expect("a translation");
        assert_eq!((t.language.as_str(), t.current, t.by.as_deref()), ("en", false, Some("Tomato")));
        assert_eq!(f.languages_or_implied(), ["en", "ja"], "the patch's language first");
        assert_eq!(f.readable_in("en"), Some(true));
        let f = parse("Seiken Densetsu 3 (J) [T+Eng1.01_Neill Corlett]");
        let t = f.translation.unwrap();
        assert_eq!((t.language.as_str(), t.current, t.by.as_deref()), ("en", true, Some("Neill Corlett")));
        assert_eq!(parse("Final Fantasy V (J) [T+Ger]").translation.unwrap().language, "de");
        assert_eq!(parse("Tales of Phantasia (J) [T+Bra]").translation.unwrap().language, "pt");
        assert_eq!(parse("Live A Live (Japan) (Translated En)").translation.unwrap().language, "en");
        assert!(parse("Tetris (World)").translation.is_none());
        assert!(parse("Teenage Mutant Ninja Turtles (USA)").translation.is_none(), "a T in the title");
    }

    #[test]
    fn a_line_for_the_versions_page() {
        assert_eq!(parse("Chrono Trigger (USA, Europe) (En,Fr,De).nds").describe(), "USA, Europe · English, French, German");
        assert_eq!(parse("Mother 3 (Japan) [T-En by Chewy v1.3].gba").describe(), "Japan · English (fan translation)");
        assert_eq!(parse("Chrono Trigger (Japan).chd").describe(), "Japan · Japanese");
        assert_eq!(parse("Tetris (World) (Rev 1)").describe(), "World", "World says no language");
        assert_eq!(parse("uCity").describe(), "");
        assert_eq!((region_name("hong kong").as_str(), language_name("xx")), ("Hong Kong", "xx"));
    }

    #[test]
    fn other_flags_and_odd_names() {
        let f = parse("Star Fox 2 (Japan) (Beta) (1995-06-20)");
        assert_eq!(f.flags, [Flag::Beta]);
        assert_eq!(parse("Game (USA) (Proto)").flags, [Flag::Proto]);
        assert_eq!(parse("Game (USA) (Demo) (Kiosk)").flags, [Flag::Demo]);
        assert_eq!(parse("Game (Asia) (Unl)").flags, [Flag::Unlicensed]);
        assert_eq!(parse("Super Mario Bros. (USA) (Virtual Console)").flags, [Flag::Rerelease]);
        assert_eq!(parse("Homebrew Game (World) (Aftermarket) (Unl)").flags, [Flag::Homebrew, Flag::Unlicensed]);
        assert_eq!(parse("Final Fantasy VII (USA) (Disc 1)").languages, Vec::<&str>::new(), "a disc is not a language");
        assert_eq!(parse("Plain Name"), RomFacts::default());
        assert_eq!(parse("Broken (USA"), RomFacts::default(), "an unclosed group says nothing");
        assert_eq!(parse("Mismatched (USA]").regions, Vec::<&str>::new());
        assert_eq!(parse(""), RomFacts::default());
    }
}
