//! Part of the pocket-ui binary: PortMaster ports that need the player's own game files. Before such a port
//! starts, files waiting in its drop folder on the share are moved into place, and when its files are surely
//! missing (`port_files::check`) the "Copy your game files" page opens instead of a start into nothing.

use std::path::Path;

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::port_files::{self, FilesPage, Share, Toggle, Verdict};
use pocket_ui::ports;
use pocket_ui::wifi;

use super::App;

/// A tiny catalog for `--snapshot setup:ports` when POCKETUI_DATA has no ports.json (real entries, shortened).
const DEMO_CATALOG: &str = r#"{"ports":{
"2048.zip":{"name":"2048.zip","items":["2048.sh","2048/"],"attr":{"title":"2048","desc":"Join the numbers and get to the 2048 tile.","genres":["puzzle"],"rtr":true,"arch":["aarch64"]},"source":{"size":83788}},
"cceleste.zip":{"name":"cceleste.zip","items":["Celeste Classic.sh","cceleste/"],"attr":{"title":"Celeste Classic","desc":"The original PICO-8 Celeste, a tight little platformer.","genres":["platformer"],"rtr":true,"arch":["aarch64"]},"source":{"size":2400000}},
"sprout.zip":{"name":"sprout.zip","items":["SPROUT.sh","sprout"],"attr":{"title":"-SPROUT-","desc":"-SPROUT- is a mini precision-platformer built for speed-running. Run, jump and dash your way through 100 levels across five unique lands, and restore your world to its former glory.","inst":"Purchase the game from Steam. Download the game and place the game files in sprout/assets.","genres":["platformer"],"rtr":false,"arch":["aarch64"]},"source":{"size":8159843}},
"apondbeyond.zip":{"name":"apondbeyond.zip","items":["A Pond Beyond.sh","apondbeyond"],"attr":{"title":"A Pond Beyond","desc":"A short frog adventure.","inst":"Download A Pond Beyond.zip (August 16, 2024) from https://npjarcade.itch.io/a-pond-beyond. Copy the data.win from the zip over to apondbeyond/assets.","genres":["adventure"],"rtr":false,"arch":["aarch64"]},"source":{"size":5100000}},
"counter-strike.zip":{"name":"counter-strike.zip","items":["Counter-Strike.sh","counter-strike"],"attr":{"title":"Counter-Strike","desc":"The classic team shooter on the Xash3D engine.","inst":"1. Buy the game on Steam https://store.steampowered.com/app/10/CounterStrike.\n\n2. Copy your valve folder into: /roms/ports/counter-strike/valve\n\n3. Copy your cstrike folder into: /roms/ports/counter-strike/cstrike\n\n4. Do not overwrite existing port files when copying game data. If prompted, choose Skip / Keep existing files.\n\n5. Launch Counter-Strike from PortMaster.","genres":["fps"],"rtr":false,"arch":["aarch64"]},"source":{"size":61000000}}
},"utils":{}}"#;

/// The catalog the browser snapshot shows: POCKETUI_DATA's cached ports.json, else [`DEMO_CATALOG`].
pub(crate) fn snapshot_catalog(data_dir: &Path) -> Vec<ports::Port> {
    let cached = std::fs::read(data_dir.join("ports.json")).map(|b| ports::parse_catalog(&b)).unwrap_or_default();
    if cached.is_empty() {
        ports::parse_catalog(DEMO_CATALOG.as_bytes())
    } else {
        cached
    }
}

impl App {
    /// Before a port starts: true when it is not to start, because its files are missing; the page then says
    /// what to copy where. Ready-to-run ports, and ports whose files may well be there, start as before.
    pub(crate) fn port_needs_files(&mut self, launcher: &Path) -> bool {
        let ports_dir = Path::new(ports::PORTS_DIR);
        let Some(info) = ports::installed_port(ports_dir, launcher) else {
            return false;
        };
        if info.rtr {
            return false;
        }
        let port = ports_dir.join(&info.dir);
        let expect = port_files::expect(&info.instructions, &info.dir);
        // Files the player put into the share's drop folder go where the port looks for them.
        if let Some((dest, rule)) = port_files::drop_target(&port, &expect) {
            let drop = Path::new(port_files::DROP_DIR).join(&info.dir);
            match port_files::take_dropped(&drop, &dest, &rule) {
                Ok(n) if n.moved + n.kept > 0 => println!("ports: {} moved from {} to {}, {} left there", n.moved, drop.display(), dest.display(), n.kept),
                Ok(_) => {}
                Err(e) => eprintln!("ports: moving {} to {}: {e}", drop.display(), dest.display()),
            }
        }
        let played = self.library.get(launcher).sessions > 0;
        let Verdict::Missing(folder) = port_files::check(&port, info.written, &expect, played) else {
            return false;
        };
        println!("ports: {} not started, its files are missing ({})", info.title, folder.display());
        let page = FilesPage::new(&info.title, &info.dir, &info.instructions, ports_dir).blocked();
        if page.drop.is_some() {
            port_files::make_drop(Path::new(port_files::DROP_DIR), &info.dir);
        }
        let share = Share {
            on: wifi::sharing_enabled(),
            ip: wifi::ip(),
            password: wifi::share_password(),
        };
        self.port_files = Some((page, share));
        true
    }

    pub(crate) fn port_files_input(&mut self, b: Button) {
        if let Some((page, _)) = self.port_files.as_mut() {
            if page.press(b) {
                self.port_files = None;
            }
        }
    }

    /// The page over the shelf, in the appearance in use; its hints can be tapped.
    pub(crate) fn paint_port_files(&mut self, c: &mut Canvas) {
        let Some((mut page, share)) = self.port_files.take() else { return };
        page.paint(c, &mut self.text, &share, &Toggle::InSettings);
        let hints = page.hints(&share, &Toggle::InSettings);
        self.paint_hint_row(c, &hints);
        self.port_files = Some((page, share));
    }

    /// `--snapshot port-files[:on|:off][:<catalog name>[:<rows scrolled>]]`: the page as a start of the port
    /// without its files opens it (-SPROUT- by default, from POCKETUI_DATA's ports.json or the demo catalog), with
    /// file sharing off or on (made-up address and password).
    pub(crate) fn port_files_snapshot(&mut self, state: &str) {
        let parts: Vec<&str> = state.split(':').collect();
        let on = parts.get(1) == Some(&"on");
        let name = parts.get(2).copied().unwrap_or("sprout.zip");
        let port = snapshot_catalog(&self.data_dir).into_iter().find(|p| p.name == name).unwrap_or_default();
        let mut page = FilesPage::new(&port.title, &port.dir, &port.instructions, Path::new(ports::PORTS_DIR)).blocked();
        let share = Share {
            on,
            ip: Some("192.0.2.23".into()),
            password: on.then(|| "maple-river-stone-482".into()),
        };
        // One paint to know how far the instructions run, then the scroll.
        let rows: usize = parts.get(3).and_then(|n| n.parse().ok()).unwrap_or(0);
        if rows > 0 {
            let mut buf = vec![0u32; 1240 * 1080];
            page.paint(&mut Canvas::new(&mut buf, 1240, 1080), &mut self.text, &share, &Toggle::InSettings);
            for _ in 0..rows {
                page.press(Button::Down);
            }
        }
        self.port_files = Some((page, share));
    }
}
