//! The spine colour every cover in a folder gets, the way the Cabinet works it out (`media::cover_colors` of the
//! cover without its system's strip), and how many of them come out pale (white, grey or black).
//!   cargo run --profile quick --example spine_colour_survey -- <covers dir> [out.tsv]
//! The covers dir holds one folder per system (`gba/`, `snes/`, ...) of PNG or JPEG box art.
use pocket_ui::{image, media};
use std::io::Write;
use std::path::Path;

/// No clear colour: (max - min) / max under a fifth, or nearly white or black.
fn pale(c: u32) -> bool {
    let (r, g, b) = (c >> 16 & 0xff, c >> 8 & 0xff, c & 0xff);
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    (hi - lo) * 5 < hi.max(1)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = Path::new(args.get(1).expect("covers dir"));
    let mut out: Box<dyn Write> = match args.get(2) {
        Some(p) => Box::new(std::fs::File::create(p).expect("out file")),
        None => Box::new(std::io::sink()),
    };
    let mut systems: Vec<_> = std::fs::read_dir(root).expect("covers dir").flatten().filter(|e| e.path().is_dir()).collect();
    systems.sort_by_key(|e| e.file_name());
    let (mut all, mut all_pale) = (0, 0);
    for sys in systems {
        let name = sys.file_name().to_string_lossy().into_owned();
        let game = media::Medium::of(&name).game();
        let mut files: Vec<_> = std::fs::read_dir(sys.path()).unwrap().flatten().map(|e| e.path()).collect();
        files.sort();
        let (mut n, mut p) = (0, 0);
        for f in files {
            let Some(img) = image::load_image(&f) else { continue };
            let Some((main, accent)) = media::cover_colors(&media::trim_system_band(&img, game).px) else { continue };
            n += 1;
            if pale(main) {
                p += 1;
            }
            let file = f.file_name().unwrap().to_string_lossy();
            writeln!(out, "{name}\t{file}\t{main:06x}\t{accent:06x}\t{}", pale(main) as u8).unwrap();
        }
        println!("{name:<14} {p:>4} / {n:<4} pale");
        all += n;
        all_pale += p;
    }
    println!("{:<14} {all_pale:>4} / {all:<4} pale", "all");
}
