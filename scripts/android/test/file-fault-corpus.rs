//! Host JVM tests consume the exact same policy as the Rust reader rule.
#[path = "../../../tests/common/data_faults.rs"]
mod data_faults;
fn main() {
    let root=std::path::PathBuf::from(std::env::args_os().nth(1).expect("output directory"));
    if let Some(seeds)=std::env::args_os().nth(2) {
        for entry in std::fs::read_dir(seeds).unwrap() {
            let entry=entry.unwrap();if !entry.file_type().unwrap().is_file(){continue;}
            let dir=root.join(entry.file_name());std::fs::create_dir_all(&dir).unwrap();
            let mut labels=Vec::new();
            for (label,bytes) in data_faults::faults(&std::fs::read(entry.path()).unwrap()) {
                if let Some(bytes)=bytes {std::fs::write(dir.join(&label),bytes).unwrap();}
                labels.push(label);
            }
            std::fs::write(dir.join("labels"),labels.join("\n")).unwrap();
        }
        return;
    }
    for size in [128<<10,512<<10,2<<20] {
        let dir=root.join(size.to_string()); std::fs::create_dir_all(&dir).unwrap();
        let mut labels=Vec::new();
        for (label,bytes) in data_faults::faults(&vec![0x5a;size]) {
            if let Some(bytes)=bytes { std::fs::write(dir.join(&label),bytes).unwrap(); }
            labels.push(label);
        }
        std::fs::write(dir.join("labels"),labels.join("\n")).unwrap();
    }
}
