//! How long until "the case knows" has its line and pictures on a device (docs/CASE-KNOWS.md, "Cost"):
//!
//!     case_knows_bench <work dir> <system> <rom> [identify checksums.tsv]
//!
//! The reading thread's `Reading::progress` and `Reading::art`, timed: cold (nothing known: the ROM read through for
//! its checksum unless the identify cache is given, and again for the places of its pictures), after a restart (the
//! caches in <work dir>), and once more (nothing changed). Writes only into <work dir>.
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 4 {
        eprintln!("usage: case_knows_bench <work dir> <system> <rom> [identify checksums.tsv]");
        return;
    }
    let (work, system, rom) = (std::path::PathBuf::from(&a[1]), a[2].clone(), std::path::PathBuf::from(&a[3]));
    std::fs::create_dir_all(work.join("identify")).unwrap();
    if let Some(sums) = a.get(4) {
        std::fs::copy(sums, work.join("identify/checksums.tsv")).unwrap();
    }
    let round = |r: &mut pocket_ui::case_knows_read::Reading| {
        let t = Instant::now();
        let p = r.progress(&system, &rom);
        let line = t.elapsed();
        let t = Instant::now();
        let art = p.as_ref().and_then(|p| r.art(&rom, p));
        (p, art, line, t.elapsed())
    };
    let mut r = pocket_ui::case_knows_read::Reading::new(&work);
    let (p, art, line, pics) = round(&mut r);
    r.keep();
    println!("line: {:?}", p.as_ref().map(pocket_ui::case_knows_read::line));
    println!("pictures: {} team, {} badges", art.as_ref().map_or(0, |a| a.team.len()), art.as_ref().map_or(0, |a| a.badges.len()));
    println!("cold: line {line:?}, pictures {pics:?}");
    let mut r = pocket_ui::case_knows_read::Reading::new(&work);
    let (_, _, line, pics) = round(&mut r);
    println!("restart: line {line:?}, pictures {pics:?}");
    let (_, _, line, pics) = round(&mut r);
    println!("again: line {line:?}, pictures {pics:?}");
}
