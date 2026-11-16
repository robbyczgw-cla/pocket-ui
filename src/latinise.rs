//! Letters no font of ours can draw: what the player sees instead of "?" and empty boxes.
//!
//! Barlow, Departure Mono and Pixelify have no kana, hangul or kanji. The system's Noto Sans CJK fills in where it is
//! installed (ROCKNIX, `text_fallback`); everywhere else (a computer, the Brick, a phone's own fonts missing) a name like
//! "ニンテンドーゲームキューブ" used to be painted as "???????????", which reads as a bug. Two things happen now, both
//! without a font of CJK glyphs in the binary:
//!
//! * **The painter** (`Text::translated`) runs every string with such letters through [`fold`]: kana and hangul become
//!   Latin letters (Hepburn and Revised Romanization, "Fainaru Fantajii VI"), full-width letters and CJK punctuation
//!   their ASCII twins, and the letters nothing can say in Latin (kanji, hanzi, Thai, ...) are left out. A name of
//!   nothing but those is painted as "…", never as a gap. Strings that every font can draw are not touched or copied.
//! * **The content** that has a better way to say nothing leaves such text out before it is painted: a console's
//!   other names (`consoles`), a description, developer or genre that is not readable ([`readable`], [`shows`]).
//!
//! [`base_covers`] and [`default_covers`] are the content's view of "can be drawn" (every embedded face, plus the
//! fallback font of this system); the painter asks its own faces (`Text::covers`), which follow Settings → Look.

use std::borrow::Cow;
use std::sync::OnceLock;

use ttf_parser::Face;

/// The embedded faces every style paints with: a letter is "ours" when all of them have it.
fn faces() -> &'static [Face<'static>] {
    static FACES: OnceLock<Vec<Face<'static>>> = OnceLock::new();
    FACES.get_or_init(|| {
        let bytes: [&'static [u8]; 5] = [
            crate::font_style::BARLOW_TITLE,
            crate::font_style::BARLOW_BODY,
            crate::font_style::DEPARTURE,
            include_bytes!("../assets/fonts/PixelifySans-SemiBold.ttf"),
            include_bytes!("../assets/fonts/PixelifySans-Regular.ttf"),
        ];
        bytes.iter().filter_map(|b| Face::parse(b, 0).ok()).collect()
    })
}

/// Whether every embedded face (any style of Settings → Look) has a glyph for `ch`.
pub fn base_covers(ch: char) -> bool {
    ch.is_ascii() || faces().iter().all(|f| f.glyph_index(ch).is_some_and(|g| g.0 != 0))
}

/// `base_covers`, or the system's fallback font (`text_fallback::path`) has `ch`; the font is opened only for a letter
/// that the embedded faces lack.
pub fn default_covers(ch: char) -> bool {
    base_covers(ch) || crate::text_fallback::path().and_then(|p| crate::text_fallback::open(&p)).is_some_and(|f| f.has(ch))
}

/// A letter or digit that `covers` says can't be drawn: the ones that were painted as "?".
fn lacks(ch: char, covers: &mut dyn FnMut(char) -> bool) -> bool {
    !ch.is_ascii() && ch.is_alphanumeric() && !covers(ch)
}

/// Whether `s` has a letter or digit that `covers` can't draw.
pub fn needs_fold(s: &str, covers: &mut dyn FnMut(char) -> bool) -> bool {
    s.chars().any(|c| lacks(c, covers) || is_cjk_mark(c) && !covers(c))
}

/// Whether `s` has no letter or digit that the embedded faces and this system's fallback font can't draw: it is
/// painted as written.
pub fn paintable(s: &str) -> bool {
    !needs_fold(s, &mut default_covers)
}

/// Whether `s` says something when painted: it has a letter or digit after [`fold`]. For a fact (developer, genre)
/// whose row would otherwise be an empty line.
pub fn shows(s: &str) -> bool {
    shows_with(s, &mut default_covers)
}

pub fn shows_with(s: &str, covers: &mut dyn FnMut(char) -> bool) -> bool {
    fold(s, covers).chars().any(char::is_alphanumeric)
}

/// Whether most of `s` (at least nine in ten of its letters) can be drawn as written: a description in the player's
/// language is shown, one in Japanese without a font is left out rather than painted as Latin letters of its kana
/// and nothing of its kanji.
pub fn readable(s: &str) -> bool {
    readable_with(s, &mut default_covers)
}

pub fn readable_with(s: &str, covers: &mut dyn FnMut(char) -> bool) -> bool {
    let (mut all, mut lost) = (0usize, 0usize);
    for c in s.chars().filter(|c| c.is_alphanumeric()) {
        all += 1;
        lost += usize::from(lacks(c, covers));
    }
    lost * 10 <= all
}

/// Punctuation and spaces of CJK text that a Latin face lacks (they are not letters, so `lacks` is blind to them).
pub(crate) fn is_cjk_mark(c: char) -> bool {
    matches!(c, '\u{3000}'..='\u{303f}' | '\u{30a0}' | '\u{30fb}' | '\u{ff01}'..='\u{ff5e}' | '\u{ff61}'..='\u{ff65}')
}

/// `s` with the letters `covers` can't draw made drawable: kana and hangul in Latin letters, full-width forms as ASCII,
/// CJK punctuation as its Latin twin, anything else that is a letter left out; the same string (no copy) when there is
/// nothing to do. A result without a letter of a string that had some is "…".
pub fn fold<'a>(s: &'a str, covers: &mut dyn FnMut(char) -> bool) -> Cow<'a, str> {
    if !needs_fold(s, covers) {
        return Cow::Borrowed(s);
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    // The next syllable's first consonant is doubled (a small "っ"), a new word starts with a capital.
    let (mut double, mut word_start) = (false, true);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        if c.is_ascii() || covers(c) || !(c.is_alphanumeric() || is_cjk_mark(c)) {
            word_start = c.is_whitespace() || matches!(c, '-' | '(' | '[' | '/');
            out.push(c);
            continue;
        }
        let said = if let Some(h) = hangul(c) {
            Some(h)
        } else if let Some(k) = kana(c) {
            Some(syllable(k, &chars, &mut i, &mut double, &out))
        } else {
            None
        };
        match said {
            Some(mut word) => {
                if double && !word.is_empty() {
                    let first = if word.starts_with("ch") { 't' } else { word.chars().next().unwrap_or('t') };
                    if !"aiueo".contains(first) {
                        word.insert(0, first);
                    }
                    double = false;
                }
                if word_start {
                    word = capital(&word);
                }
                word_start = false;
                out.push_str(&word);
            }
            None => match c {
                // Marks.
                '\u{3000}' | '\u{30fb}' | '\u{ff65}' => {
                    out.push(' ');
                    word_start = true;
                }
                '\u{3001}' | '\u{ff0c}' => out.push_str(", "),
                '\u{3002}' => out.push_str(". "),
                '\u{300c}' | '\u{300d}' | '\u{300e}' | '\u{300f}' | '\u{201c}' | '\u{201d}' => out.push('"'),
                '\u{ff01}'..='\u{ff5e}' => {
                    let ascii = char::from_u32(c as u32 - 0xfee0).unwrap_or(' ');
                    out.push(ascii);
                    word_start = false;
                }
                // Anything else a Latin face can't say (kanji, hanzi, Thai, ...) is left out.
                _ => {}
            },
        }
    }
    let done = tidy(&out);
    if done.chars().any(char::is_alphanumeric) || !s.chars().any(|c| lacks(c, covers)) { Cow::Owned(done) } else { Cow::Owned("…".to_string()) }
}

/// Spaces collapsed, brackets that nothing is left in, and separators at the ends, dropped.
fn tidy(s: &str) -> String {
    let mut s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    loop {
        let before = s.len();
        for pair in ["()", "[]", "( )", "[ ]", "\"\""] {
            s = s.replace(pair, " ");
        }
        s = s.split_whitespace().collect::<Vec<_>>().join(" ");
        s = s.replace(" ,", ",").replace(" .", ".").replace(" )", ")").replace("( ", "(");
        if s.len() == before {
            break;
        }
    }
    s.trim_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':' | '-' | '/')).to_string()
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
}

// ---- Korean: Revised Romanization of a syllable on its own (no sound changes between syllables) ----------------

const INITIAL: [&str; 19] = ["g", "kk", "n", "d", "tt", "r", "m", "b", "pp", "s", "ss", "", "j", "jj", "ch", "k", "t", "p", "h"];
const VOWEL: [&str; 21] = ["a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we", "wi", "yu", "eu", "ui", "i"];
const FINAL: [&str; 28] = ["", "k", "kk", "k", "n", "n", "n", "t", "l", "k", "m", "p", "t", "t", "p", "l", "m", "p", "p", "t", "t", "ng", "t", "t", "k", "t", "p", "t"];

fn hangul(c: char) -> Option<String> {
    let n = (c as u32).checked_sub(0xac00).filter(|n| *n < 11172)? as usize;
    Some(format!("{}{}{}", INITIAL[n / 588], VOWEL[n % 588 / 28], FINAL[n % 28]))
}

// ---- Japanese: Hepburn ------------------------------------------------------------------------------------------

/// A kana as its hiragana (katakana is hiragana + 0x60); `ー` and `ッ`/`っ` stay as they are.
fn kana(c: char) -> Option<char> {
    match c as u32 {
        0x3041..=0x3096 | 0x309d..=0x309e | 0x30fc => Some(c),
        0x30a1..=0x30f6 => char::from_u32(c as u32 - 0x60),
        0x30fd..=0x30fe => char::from_u32(c as u32 - 0x60),
        _ => None,
    }
}

fn base(c: char) -> Option<&'static str> {
    Some(match c {
        'あ' => "a", 'い' => "i", 'う' => "u", 'え' => "e", 'お' => "o",
        'か' => "ka", 'き' => "ki", 'く' => "ku", 'け' => "ke", 'こ' => "ko",
        'が' => "ga", 'ぎ' => "gi", 'ぐ' => "gu", 'げ' => "ge", 'ご' => "go",
        'さ' => "sa", 'し' => "shi", 'す' => "su", 'せ' => "se", 'そ' => "so",
        'ざ' => "za", 'じ' => "ji", 'ず' => "zu", 'ぜ' => "ze", 'ぞ' => "zo",
        'た' => "ta", 'ち' => "chi", 'つ' => "tsu", 'て' => "te", 'と' => "to",
        'だ' => "da", 'ぢ' => "ji", 'づ' => "zu", 'で' => "de", 'ど' => "do",
        'な' => "na", 'に' => "ni", 'ぬ' => "nu", 'ね' => "ne", 'の' => "no",
        'は' => "ha", 'ひ' => "hi", 'ふ' => "fu", 'へ' => "he", 'ほ' => "ho",
        'ば' => "ba", 'び' => "bi", 'ぶ' => "bu", 'べ' => "be", 'ぼ' => "bo",
        'ぱ' => "pa", 'ぴ' => "pi", 'ぷ' => "pu", 'ぺ' => "pe", 'ぽ' => "po",
        'ま' => "ma", 'み' => "mi", 'む' => "mu", 'め' => "me", 'も' => "mo",
        'や' => "ya", 'ゆ' => "yu", 'よ' => "yo",
        'ら' => "ra", 'り' => "ri", 'る' => "ru", 'れ' => "re", 'ろ' => "ro",
        'わ' => "wa", 'ゐ' => "i", 'ゑ' => "e", 'を' => "o", 'ん' => "n", 'ゔ' => "vu",
        'ゕ' => "ka", 'ゖ' => "ke",
        _ => return None,
    })
}

fn small_vowel(c: char) -> Option<char> {
    Some(match c {
        'ぁ' => 'a', 'ぃ' => 'i', 'ぅ' => 'u', 'ぇ' => 'e', 'ぉ' => 'o',
        _ => return None,
    })
}

fn small_y(c: char) -> Option<char> {
    Some(match c {
        'ゃ' => 'a', 'ゅ' => 'u', 'ょ' => 'o',
        _ => return None,
    })
}

/// The Latin letters of the kana `k` and the small kana that follow it (`i` moves past them), in `out`'s context
/// (a long-vowel mark repeats the vowel before it).
fn syllable(k: char, chars: &[char], i: &mut usize, double: &mut bool, out: &str) -> String {
    match k {
        'っ' => {
            *double = true;
            return String::new();
        }
        'ー' => return out.chars().last().filter(|c| "aiueo".contains(*c)).map_or("-".into(), |v| v.to_string()),
        'ゝ' | 'ゞ' => return String::new(),
        _ => {}
    }
    let Some(mut s) = base(k).map(str::to_string) else { return String::new() };
    // Small kana after it: "ふぁ" fa, "てぃ" ti, "きゃ" kya, "しゃ" sha, "ちぇ" che.
    while let Some(next) = chars.get(*i).copied().and_then(kana) {
        if let Some(v) = small_vowel(next) {
            // The consonant of the syllable stays, its vowel is the small one (u alone: "wi", "we").
            s = match s.as_str() {
                "u" => format!("w{v}"),
                _ => format!("{}{v}", &s[..s.len() - 1]),
            };
        } else if let Some(v) = small_y(next) {
            s = match s.as_str() {
                "shi" | "chi" | "ji" => format!("{}{v}", &s[..s.len() - 1]),
                _ => format!("{}y{v}", &s[..s.len() - 1]),
            };
        } else {
            break;
        }
        *i += 1;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No font beyond ours: every non-ASCII letter that Barlow, Departure Mono and Pixelify lack is "missing".
    fn bare(c: char) -> bool {
        base_covers(c)
    }

    fn f(s: &str) -> String {
        fold(s, &mut bare).into_owned()
    }

    #[test]
    fn text_every_face_can_draw_is_not_touched() {
        for s in ["Super Mario World", "Pokémon Rubis", "Ärger über Öl, ñandú", "Wandā Suwan", "Tetris – 1989 “classic” …", ""] {
            assert!(matches!(fold(s, &mut bare), Cow::Borrowed(_)), "{s}");
        }
    }

    #[test]
    fn kana_become_hepburn() {
        assert_eq!(f("ニンテンドーゲームキューブ"), "Nintendoogeemukyuubu");
        assert_eq!(f("ゲームキューブ"), "Geemukyuubu");
        assert_eq!(f("ファイナルファンタジーVI"), "FainarufantajiiVI");
        assert_eq!(f("スーパーマリオワールド"), "Suupaamariowaarudo");
        assert_eq!(f("クロノ・トリガー"), "Kurono Torigaa");
        assert_eq!(f("ポケットモンスター"), "Pokettomonsutaa");
        assert_eq!(f("ティファ"), "Tifa");
        assert_eq!(f("しゃちょう"), "Shachou");
        assert_eq!(f("ちぇ"), "Che");
        assert_eq!(f("ばっちり"), "Batchiri");
        assert_eq!(f("きっと"), "Kitto");
    }

    #[test]
    fn hangul_becomes_revised_romanization() {
        assert_eq!(f("한국어"), "Hangukeo");
        assert_eq!(f("마리오 카트"), "Mario Kateu");
        assert_eq!(f("젤다의 전설"), "Jeldaui Jeonseol");
    }

    #[test]
    fn kanji_are_left_out_and_what_stays_is_tidied() {
        assert_eq!(f("聖剣伝説2"), "2");
        assert_eq!(f("ゼルダの伝説 神々のトライフォース"), "Zerudano Notoraifoosu");
        assert_eq!(f("Final Fantasy VI (ファイナルファンタジー)"), "Final Fantasy VI (Fainarufantajii)");
        assert_eq!(f("The Legend of Zelda (任天堂)"), "The Legend of Zelda");
        assert_eq!(f("ＦＩＮＡＬ　ＦＡＮＴＡＳＹ！"), "FINAL FANTASY!");
    }

    #[test]
    fn nothing_left_is_an_ellipsis_never_a_gap() {
        assert_eq!(f("任天堂"), "…");
        assert_eq!(f("电击文库"), "…");
        assert_eq!(f("สวัสดี"), "…");
    }

    #[test]
    fn readable_means_nine_in_ten_letters() {
        let r = |s: &str| readable_with(s, &mut bare);
        assert!(r("A game about two plumbers who travel through pipes (マリオ)"));
        assert!(!r("このゲームは二人の配管工についてのゲームです"));
        assert!(r("Plain English, with ō"));
        assert!(r(""));
    }

    #[test]
    fn a_fact_shows_when_something_of_it_can_be_said() {
        let s = |t: &str| shows_with(t, &mut bare);
        assert!(s("Nintendo"));
        assert!(s("任天堂 / Nintendo"));
        assert!(s("任天堂ゲーム"));
        assert!(!s("任天堂"));
        assert!(!s("…"));
    }
}
