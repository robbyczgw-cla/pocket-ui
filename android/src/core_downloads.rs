//! One background job at a time; the Activity transports bytes, the shared store checks and installs.
use super::bridge;
use pocket_ui::android_cores::{self, CorePackage};
use std::sync::mpsc;

pub struct Run { result: mpsc::Receiver<Result<(), String>>, pub core: String }
impl Run {
    pub fn start(offer: CorePackage, catalog_url: String) -> Self {
        let core = offer.core.clone();
        let (tx, result) = mpsc::channel();
        std::thread::spawn(move || {
            let done = (|| {
                if catalog_url.is_empty() { return Err("The system download channel is not available yet.".into()); }
                let data = pocket_ui::platform::data_dir();
                let read = |url: &str| -> Result<String, String> {
                    let text = bridge("coreRead", &[url]);
                    text.strip_prefix("OK\n").map(str::to_string).ok_or_else(|| android_cores::download_failure(text.strip_prefix("ERR\n").unwrap_or("")))
                };
                let json = read(&catalog_url)?;
                let signature = read(&format!("{catalog_url}.sig"))?;
                let packages = android_cores::verified_catalog(json.as_bytes(), &signature, &catalog_url, &data).map_err(|e| e.to_string())?;
                let p = packages.into_iter().find(|p| p.core == offer.core).ok_or("This system is not in the download channel.")?;
                // The size painted in the consent must still be the size we fetch.
                if p.package.sha256 != offer.package.sha256 || p.package.size != offer.package.size {
                    return Err("The system list has changed. Update the app and try again.".into());
                }
                let url = pocket_ui::store::resolve(&catalog_url, &p.package.url);
                let answer = bridge("coreFetch", &[&url, &p.package.sha256, &p.package.size.to_string()]);
                let path = answer.strip_prefix("OK\n").ok_or_else(|| android_cores::download_failure(answer.strip_prefix("ERR\n").unwrap_or("")))?;
                let archive = std::fs::read(path).map_err(|e| e.to_string())?;
                let result = android_cores::install(&data, &p, &archive).map_err(|e| e.to_string());
                // A failed checksum must start afresh; network errors keep the partial above.
                // This archive is disposable after verification, never a game or installed core.
                let _ = std::fs::remove_file(path);
                result
            })();
            let _ = tx.send(done);
        });
        Self { result, core }
    }
    pub fn poll(&self) -> Option<Result<(), String>> { self.result.try_recv().ok() }
}
