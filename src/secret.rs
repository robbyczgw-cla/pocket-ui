//! Per-device passwords that are shown on screen and typed on a computer: file sharing (src/wifi.rs)
//! and remote access over SSH (src/remote.rs) use the same kind, e.g. `maple-river-stone-482`.
//! Three words from a list of 256 and three digits: about 34 bits, enough for a login that is only
//! reachable on the local Wi-Fi and only while the user has switched it on.

use std::io::{Read, Write};
use std::path::Path;

/// Short, common, easy to spell; no two look alike. Exactly 256, so one random byte picks one.
const WORDS: [&str; 256] = [
    "acorn", "alarm", "amber", "anchor", "apple", "apron", "arrow", "atlas", "attic", "badge", "bagel", "baker", "bamboo", "banjo", "barn",
    "basil", "beach", "beacon", "bean", "bear", "bell", "bench", "berry", "bike", "birch", "bison", "blade", "blaze", "bloom",
    "board", "boat", "boot", "brick", "bridge", "brook", "brush", "bucket", "bugle", "cabin", "cable", "cactus", "camel", "candle",
    "canoe", "canyon", "cargo", "carrot", "castle", "cedar", "chalk", "cherry", "chess", "cider", "cinema", "circle", "clay", "cliff",
    "clock", "cloud", "clover", "coast", "cobalt", "comet", "coral", "cotton", "crane", "crayon", "creek", "crown", "cube", "daisy", "delta",
    "denim", "desert", "dingo", "dock", "dolphin", "donut", "dove", "dragon", "drum", "dune", "eagle", "easel", "echo", "elbow", "ember",
    "fable", "falcon", "fern", "ferry", "fiddle", "field", "fig", "flame", "flute", "forest", "fossil", "fox", "frost", "galaxy", "garden",
    "garlic", "gecko", "giant", "ginger", "glacier", "globe", "goose", "grape", "gravel", "guitar", "hammer", "harbor", "harp", "hazel",
    "heron", "hill", "honey", "igloo", "island", "ivory", "jacket", "jaguar", "jelly", "jungle", "kayak", "kettle", "kiwi", "koala",
    "ladder", "lake", "lamp", "lantern", "lava", "lemon", "lily", "lime", "lizard", "llama", "lobster", "lotus", "magnet", "mango", "maple",
    "marble", "meadow", "melon", "mesa", "mint", "mirror", "mocha", "moose", "moss", "mule", "museum", "nectar", "needle",
    "nest", "noodle", "nova", "oasis", "ocean", "olive", "onion", "orbit", "otter", "owl", "oyster", "paddle", "palm", "panda", "paper",
    "parrot", "peach", "pearl", "pebble", "pepper", "piano", "pilot", "pine", "planet", "plum", "pocket", "pony", "poppy", "prism", "pumpkin",
    "puzzle", "quartz", "quill", "rabbit", "radar", "radio", "rain", "raven", "reef", "ribbon", "river", "robin", "rocket", "rose", "ruby",
    "saddle", "salmon", "sand", "satin", "saturn", "scarf", "shell", "silver", "sketch", "sled", "sloth", "snow", "socket", "sofa", "spark",
    "spider", "spoon", "spruce", "squid", "stamp", "star", "stone", "storm", "sugar", "summit", "sunset", "swan", "table", "tango", "teapot",
    "tiger", "timber", "toast", "tomato", "torch", "tower", "trail", "tulip", "tunnel", "turtle", "valley", "velvet", "violin", "walnut",
    "whale", "willow", "window", "winter", "wizard", "wool", "yacht", "zebra", "zipper",
];

/// `word-word-word-NNN` from a source of random bytes. Uniform: one byte per word, digits by rejection.
pub fn readable(mut byte: impl FnMut() -> u8) -> String {
    let words: Vec<&str> = (0..3).map(|_| WORDS[byte() as usize]).collect();
    let digits = loop {
        let v = u16::from_be_bytes([byte(), byte()]);
        if v < 64_000 {
            break v % 1000;
        }
    };
    format!("{}-{digits:03}", words.join("-"))
}

/// A fresh password from the kernel's random source.
pub fn generate() -> std::io::Result<String> {
    let mut urandom = std::fs::File::open("/dev/urandom")?;
    let mut failed = None;
    let pw = readable(|| {
        let mut b = [0u8; 1];
        if let Err(e) = urandom.read_exact(&mut b) {
            failed = Some(e);
        }
        b[0]
    });
    match failed {
        Some(e) => Err(e),
        None => Ok(pw),
    }
}

/// The password kept at `path`, created on first use. Readable by root only.
pub fn load_or_create(path: &Path) -> std::io::Result<String> {
    if let Some(pw) = load(path) {
        return Ok(pw);
    }
    let pw = generate()?;
    write_private(path, pw.as_bytes())?;
    Ok(pw)
}

/// The password at `path`, if one was created; never creates one.
pub fn load(path: &Path) -> Option<String> {
    let pw = std::fs::read_to_string(path).ok()?.trim().to_string();
    (!pw.is_empty()).then_some(pw)
}

/// Writes `data` with mode 0600 under a temporary name and renames it into place.
pub fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("new");
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn word_list_is_256_distinct_plain_words() {
        let mut seen = std::collections::HashSet::new();
        for w in WORDS {
            assert!(seen.insert(w), "{w} twice");
            assert!(w.len() >= 3 && w.len() <= 7 && w.bytes().all(|b| b.is_ascii_lowercase()), "{w}");
        }
    }

    #[test]
    fn readable_password_has_three_words_and_three_digits() {
        let mut n = 0u8;
        let pw = readable(|| {
            n = n.wrapping_add(37);
            n
        });
        let parts: Vec<&str> = pw.split('-').collect();
        assert_eq!(parts.len(), 4, "{pw}");
        assert!(parts[..3].iter().all(|w| WORDS.contains(w)));
        assert!(parts[3].len() == 3 && parts[3].bytes().all(|b| b.is_ascii_digit()));
        // Digits by rejection: 0xFFFF is skipped, the next pair is used.
        let mut bytes = [0u8, 1, 2, 0xff, 0xff, 0x00, 0x07].into_iter();
        assert_eq!(readable(|| bytes.next().unwrap()), "acorn-alarm-amber-007");
    }

    #[test]
    fn kept_once_per_device_and_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TestDir::new("secret");
        let path = dir.join("sub/password");
        assert_eq!(load(&path), None);
        let first = load_or_create(&path).unwrap();
        assert_eq!(load_or_create(&path).unwrap(), first, "the same password every time");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_ne!(generate().unwrap(), generate().unwrap());
    }
}
