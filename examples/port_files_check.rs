//! What pocket-ui reads out of the instructions of every port that needs the player's own files: the folder
//! the files go into, and the files named.
//!   cargo run --release --example port_files_check -- ports.json [--all]
//! ports.json: PortMaster-New's catalog (https://github.com/PortsMaster/PortMaster-New/releases/latest/download/ports.json).
//! Without --all it prints only the summary and the ports where nothing was found.
use pocket_ui::port_files;
use pocket_ui::ports;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: port_files_check <ports.json> [--all]");
        return;
    };
    let all = args.any(|a| a == "--all");
    let catalog = ports::parse_catalog(&std::fs::read(&path).expect("read the catalog"));
    let own: Vec<_> = catalog.iter().filter(|p| p.needs_files).collect();
    let (mut sub, mut several, mut named, mut none, mut optional, mut sure) = (0, 0, 0, 0, 0, 0);
    for p in &own {
        let e = port_files::expect(&p.instructions, &p.dir);
        match e.folders.len() {
            1 => sub += 1,
            0 if e.root && !e.files.is_empty() => named += 1,
            0 => none += 1,
            _ => several += 1,
        }
        optional += usize::from(e.optional);
        sure += usize::from(e.sure);
        if all || (e.folders.is_empty() && !(e.root && !e.files.is_empty())) {
            println!(
                "{:<28} folders {:?} root {} files {:?} optional {} | {}",
                p.dir,
                e.folders,
                e.root,
                e.files,
                e.optional,
                p.instructions.replace('\n', " / ")
            );
        }
    }
    println!(
        "\n{} ports, {} need files: {sub} with one named folder, {several} with several ({sure} of all named with the port's own folder), {named} with named files only, {none} naming neither, {optional} optional",
        catalog.len(),
        own.len()
    );
}
