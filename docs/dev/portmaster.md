# PortMaster ports

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you work on PortMaster ports.*

## PortMaster ports that need the player's files

TrimUI's optional runtime and native launcher: [TRIMUI-PORTMASTER.md](../TRIMUI-PORTMASTER.md).
`update::payload` exposes the updater transport to `--download-asset` before UI/audio startup; the card's
`portmaster` helper stages it outside `app/`. Snapshot states `setup:ports-get`, `-checking`, `-update`,
`-offline`, `-download` and `-error` use the existing Ports step; `portmaster_runtime_offer` in `nav_explorer`
checks both fonts/appearances, Large DE/ES text and B/back/reopen.
`setup:extras-cheats-<state>` and `setup:extras-portmaster-<state>` paint the real Extras row mapper with
deterministic local facts. `tests/cheats_snapshots.rs` checks missing, downloading, checking, saving, installed,
update, offline, low-space, error and removal states at 640×480 Large in EN/DE/ES, dark and light. The navigation
explorer reaches every row, enters Starter pack and returns to the same Extras row with B.
The TrimUI PortMaster row reads `ports::runtime_space` on a worker and uses `ports::remove_runtime` only after
X offers removal and A confirms. Removal keeps games, HOME, engine runtimes and config; its completion refreshes
free space on a worker before another Get. The browser's X still opens the selected game's own-file instructions.
`optional_data::lock(&update::Cancel)` serializes optional pack workers, including cheats, until checkpointing
finishes; it returns `None` when a queued request is cancelled. Never acquire it on the UI thread.
`ports::runtime_space()` returns `RuntimeSpace { needed_kib, available_kib }` from a worker for translated
capacity notices. `ports::remove_runtime(report, cancel)` uses the same lane and preserves engines/config,
port folders and HOME saves; its caller must obtain explicit removal consent first.

About half of PortMaster's arm64 ports are not ready to run (`attr.rtr: false`): they are engines for games the
player owns. `ports.rs` keeps them (`Port::needs_files`, `Port::instructions` = `attr.inst`); only experimental
ones are left out. The browser (`setup.rs`, `Step::Ports`) shows them as a second list, and `Step::PortFiles`
is the "Copy your game files" page (`port_files::FilesPage`, also used by the shelf in `app_ports.rs`).

PortMaster says where the files go only in prose. `port_files::expect` reads it:

- **Folders**: paths in the text that name the port's folder (`sprout/assets`, `/roms/ports/counter-strike/valve`),
  also under another spelling of it after `ports/` (`ports/alientdeathmob/assets` for `aliendeathmob`); failing
  that, phrases ("into the assets folder", "this port's gamedata folder", "the port folder"). What follows
  "optional", "you can also", "if you want" is left out.
- **Files**: names with a game-data extension (`data.win`, `Balatro.love`, `arco.exe`, `pak0.pak`) after a verb that
  puts them somewhere, not in an "alternatively" sentence. Names with spaces come out as their last word ("Left.exe"
  for "Pengu Never Left.exe"); everything that looks for them matches the end of a name.
- On the October 2026 catalog (511 such ports): 378 with one folder, 10 with several, 47 with named files only,
  76 where the text says neither (README only, "the port folder" without names). The unit tests in `port_files.rs`
  hold about 50 of the real texts.

**The start guard** (`App::port_needs_files`, before every start of a `ports` game): it finds the port by its script
in the `items` of `roms/ports/*/port.json` (harbourmaster writes that file last when it installs). Ports whose
port.json says `rtr: true`, or has no `rtr`, start as always. For the others `port_files::check` says "missing"
only when all of these hold, and the page opens instead of the start:

1. pocket-ui has never seen the port run for 60 s or more (`library_meta` sessions);
2. no file in the port's folder, other than `log.txt`/`*.log`, is more than two minutes newer than port.json: a port
   that prepared its data on a first start (patched `data.win`, unpacked an installer, emptied `gamedata` again)
   has written something, so it is left alone;
3. the first folder the text names holds only placeholders (empty files, dotfiles, `*.txt`, `*.md`, "place … here"),
   and none of the named files is anywhere in the port; or that folder is missing and the text named it with the
   port's own folder; or the text names the port folder itself and none of the named files is anywhere in it.

Anything else (no clear folder, any real file in it, a folder of more than 20 000 entries) starts the port.

**Where the files go.** Straight into the port's own folder: the page shows `roms/ports/<port>/<folder>` and its
address on the share (the port folder itself when the text names several folders or none), and makes the folder when
the text names it with the port's own folder (`port_files::make_place`), handed to the share user. The web companion
writes to the same place (`web::files::port_target`). pocket-ui moves nothing at a start. `roms/port-files`, the drop
folder up to 0.1.8, is no longer read or made; what a player left there stays, and `library::NOT_STRAYS` keeps it out
of the stray folders.
