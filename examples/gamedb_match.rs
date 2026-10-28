//! Matches ROM file names to gamedb entries with the device's own rules (`GameDb::lookup`), for build
//! scripts that need the same answer as the game card (scripts/build-curatordb.py uses it to put the
//! games of a library first).
//!
//! ```text
//! cargo run --release --example gamedb_match < list.txt
//! ```
//! Input: one `system/file name` per line. Output: `system<TAB>qid<TAB>work` per matched line (nothing
//! for a line without a match), so no file name leaves this program.

use std::io::BufRead;

fn main() {
    let mut db = pocket_ui::gamedb::GameDb::embedded();
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let Some((system, file)) = line.trim().split_once('/') else { continue };
        let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
        if let Some(g) = db.lookup(system, stem) {
            println!("{system}\t{}\t{}", g.qid, g.work());
        }
    }
}
