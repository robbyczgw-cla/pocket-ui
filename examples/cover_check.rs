//! Which cover each game would get, without downloading anything.
//!   cargo run --release --example cover_check < roms.txt
//! roms.txt: one `system/file name` per line (as under /storage/roms).
use pocket_ui::covers;
use std::collections::HashMap;
use std::io::BufRead;

fn main() {
    let mut indexes: HashMap<&str, Vec<String>> = HashMap::new();
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let Some((sys, file)) = line.split_once('/') else { continue };
        let stem = std::path::Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or(file);
        let Some(list) = covers::playlist(sys) else {
            println!("{sys:<12} {stem:<55} -> (no libretro playlist)");
            continue;
        };
        let names = indexes.entry(list).or_insert_with(|| covers::fetch_index(list));
        match covers::best_match(stem, names) {
            Some(name) => println!("{sys:<12} {stem:<55} -> {name}"),
            None => println!("{sys:<12} {stem:<55} -> MISSING ({} names in {list})", names.len()),
        }
    }
}
