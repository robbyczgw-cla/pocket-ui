# Box art & downloads

Settings → Games → **Box art & downloads** is home to every optional download. First start only points here, and nothing is
forced. Pick a row with Up/Down, check the action on the right and description below, press A to run, or B to return to
Games.

**Box art**, **Pictures** and **Starter pack** appear on every handheld; **PortMaster games** appears on ROCKNIX and
TrimUI; **Cheats** appears on TrimUI (ROCKNIX includes cheats built in). Offline, downloads show **Needs Wi-Fi**, while
already-downloaded pictures and cheats keep working offline. No account needed.

For fast-forward, rewind, rumble and fan during play, see [Options](options.md#speed-rumble-fan).

## Starter pack

Pick **Starter pack** to browse free homebrew games shared by their creators. Grab one or install the whole set with A,
or press B to cancel cleanly. The same pack is also offered on Home while your card has no games yet
([Getting started](getting-started.md#nothing-on-the-sd-card-yet-the-starter-pack)).

## Cheats

On TrimUI handhelds, **Cheats** offers **Get · 30 MB**: press A to download cheat codes for thousands of games (used via
HOME → Options → Cheats, which offers the exact same button if missing; opening either page never downloads anything by
itself). Shows live download, verification and save progress. Press B to pause/cancel safely; retrying resumes from your
saved checkpoint (free-space checks reserve room for it too).

**Offline?** The row offers **Look on the card**: grab the `cheats-….pack` file from pocketui.xyz/cheats, drop it onto
the root of your SD card or inside the `pocket-ui` folder, and press A. pocket-ui verifies its size and checksum,
installs it and removes the installer file.

Once installed, cheats work completely offline. Press A twice to uninstall the database (your per-game cheat toggles stay
saved and return if you reinstall). Future app updates show **Update** when a newer database ships (A updates, X
removes); your current codes stay active until the update finishes verifying, and games/saves are never touched.

## Box art

Fetches front box covers automatically from a free community archive, with no account required.

### By itself

Unless **Pictures** is [Off](#pictures), pocket-ui downloads missing covers automatically in the background whenever
you're online:

- right after first start (if you left Box art enabled on Your games),
- on boot once Wi-Fi connects (waits up to three minutes),
- after refreshing your library with newly added games (such as via Settings → Games → Add games).

A quiet little dot beside the clock is the only sign it's working as covers fade onto your shelf. It pauses immediately
when you launch a game and resumes once you return. Games not found in the archive aren't retried endlessly, while
interrupted lookups retry automatically on next boot.

### Without Wi-Fi

On offline handhelds (RG28XX, RG35XX 2024, R36S), Box art reads **From a computer**: scrape covers on your PC (for
example with Skraper) and drop them into a `covers` folder inside each console folder with matching filenames
(`roms/gba/covers/Golden Sun (USA).png` for `roms/gba/Golden Sun (USA).gba`). Folders named `boxart`, `box2d` and
`images` work too ([Your gamelists](library-files.md#your-gamelists)). PortMaster games reads **From the card**
([PortMaster games without Wi-Fi](#portmaster-games-without-wi-fi)); Starter pack shows **No Wi-Fi**.

### By hand

1. Highlight **Box art** and press A ("Download"). If background fetching is already running, this shows its live
   progress instead of duplicating work.
2. pocket-ui checks every game missing a cover (plus missing screenshots/title screens when **Pictures** is set to Box
   art and screenshots), showing live progress ("Looking up 3 of 120 covers" / "… pictures") and finishing with "Found
   42 new covers" or "No new covers found". If the server is unreachable ("Couldn't reach the picture server. Try again
   later."), games aren't marked missing and retry automatically next boot.

How matching works:

- **Checksum first:** for cartridge consoles (GB through DS, NES, SNES, N64, Mega Drive, Master System, Game Gear, PC
  Engine and other handhelds), pocket-ui hashes each ROM once in the background (for `.zip` and `.7z`, just its fast
  header table) and looks it up in the console's cached release list (~a few hundred KB downloaded once per console).
  Even cryptic filenames like `0902 - MM&B (U).gba` resolve cleanly to `Mega Man & Bass (USA)`.
- **Title fallback:** unknown hashes, discs on Saturn/Dreamcast/PSP/GameCube and files over 64 MB match by cleaned
  filename (ignoring region/revision tags and scene numbers; No-Intro naming matches best, preferring no cover over a
  wrong one).
- **PlayStation 1 & 2 discs:** identified directly from the serial inside `SYSTEM.CNF` (`SLES_556.73`), matching cover,
  screenshots and facts to that exact release ([PlayStation 1 and 2 discs](library-files.md#playstation-1-and-2-discs));
  multi-disc sets show "(Disc 2)".
- Missing matches are remembered so reruns stay fast, and newly checksum-matched games auto-recheck once their console
  list arrives.
- Got a wrong cover? Open the game card, press Y and choose **Wrong pictures? Look up again**
  ([the game card](library.md#wrong-pictures-look-up-again)).
- Covers cache as PNGs in `/storage/.config/pocket-ui/covers/<system>/<game file name>.png`; drop your own PNG there to
  override any game.
- Box art shows in every layout and also prints onto cartridges and discs in Shelf and Cabinet.

### Pictures

All artwork comes from the free libretro thumbnail server (no account; only game names are sent, never your ROMs, saves
or personal info). **Pictures** chooses what downloads automatically and what's displayed (cycle with A or Left/Right):

- **Box art and screenshots** (default): fetches box art first, then an in-game screenshot and title screen while online
  and idle.
- **Box art:** downloads and shows covers only (existing screenshots/titles stay on your card if you switch back later).
- **Off:** stops all automatic artwork downloads while keeping existing covers visible; manual **Box art** downloads and
  **Wrong pictures? Look up again** still work whenever you ask.

*(First start offers a simple Box art toggle; come here for the three-way Pictures control.)*

Where screenshots and title screens appear:

- **Split:** lingers ~2.5 seconds on box art, then gently cycles through gameplay screenshot → title screen → box art
  once before resting (stays on box art when [Motion](settings.md#motion) is Off).
- **Game card** ([What the card shows](library.md#what-the-card-shows)): shown under the facts as "In the game" and
  "Title screen".
- **Web companion:** displayed read-only under "Pictures".

Want your own screenshot on a game card? Open your [screenshots](playing.md#looking-at-your-screenshots) and press **Y**
to set any shot as that game's "In the game" picture (revert anytime via
[Wrong pictures? Look up again](library.md#wrong-pictures-look-up-again)).

Good to know:

- Cached at `/storage/.config/pocket-ui/snaps/<system>/<game file name>.png` and `.../titles/<system>/…`; custom PNGs
  drop right in (oversized files are ignored). Nothing reads off disk while scrolling fast; images load once you pause.
- Typical storage footprint: ~450 KB per cover, plus ~20 KB per screenshot/title for 8/16-bit, ~180 KB for GBA and
  ~400 KB for PS1 (~40 MB for a mixed 330-game library, under 10 MB for 8/16-bit, ~140 MB for 330 PS1 titles).

<!-- TODO: the covers folder is not inside the shared roms folder, so adding your own covers needs
     another way to reach the SD card (SSH, or the SD card in a computer that reads ext4). -->

## PortMaster games

An empty Ports shelf offers **Get PortMaster** straight to this page (B returns to the shelf).

PortMaster brings indie and classic PC ports to handhelds. pocket-ui filters automatically to stable ports built for your
chip across two tabs:

- **Ready to play:** complete free games ready to launch right after installing.
- **Bring your own game files:** free engines and wrappers for PC games you own (Steam, GOG, itch.io, …). pocket-ui
  tells you exactly where to drop your files and never downloads commercial assets.

1. Highlight **PortMaster games** and press A ("Browse"); the catalogue refreshes at most once per day.
2. Switch tabs with Left/Right, browse with Up/Down or L1/R1, and preview art, genres, description and download size (or
   **Installed**) on the right.
3. Press A to **Install** ("… is installed. Find it under Ports.") or A on an installed port to **Remove** it.
4. When a port has an update, its row (and game card under Speed) quietly notes **Update available**; press A to update
   or Y to remove. Nothing updates behind your back.

Installed ports live under **Ports** on Home with PortMaster cover art and colour-matched spines (refreshing cleanly in
the background). On ROCKNIX, PortMaster initializes itself on your first install.

On the standalone TrimUI card, accept the one-time **Get PortMaster (~300 MB, one time)** download first (requires Wi-Fi
and at least 700 MiB free; B cancels while keeping checkpoints; pauses during games; survives pocket-ui updates and offers
**Update PortMaster** when updated). On the original TrimUI Brick, staging writes gently to protect the SD card (~2.5
minutes for 277 MB; **Preparing PortMaster…** shows live MB progress, B still cancels). Controls belong to each port
(original Brick has no sticks; MENU opens the pause menu).

Port saves live in `Roms/ports/<game>` or `pocket-ui/storage/home` on TrimUI. Back up both folders when backing up ports
(ports use in-game saving rather than universal save states). On TrimUI, press **X** on the PortMaster row (then A) to
remove just the PortMaster runtime offline while leaving all installed games, game files and settings intact.

Press HOME in any running port for the [pause menu](pause-menu.md#portmaster-ports) (Continue, Options, Quit), or press
START on its card for per-port **Performance & fan**, including one-tap Battery mode for busy-core ports
([PortMaster ports](performance.md#portmaster-ports)).

### PortMaster games without Wi-Fi

No Wi-Fi on your handheld (RG28XX, RG35XX 2024, R36S), or just keeping Wi-Fi off? You can install PortMaster games
straight from your SD card, following PortMaster's offline setup:

1. On a computer, download the game zip from [portmaster.games](https://portmaster.games) and **leave it zipped**.
2. If the game needs a runtime (Godot, Mono, …), grab it there under **Runtimes** too: either the single `.squashfs`
   file or a bundle (**All Runtimes** / **Popular Runtimes**). Make sure to pick the **aarch64** versions.
3. Copy the files into a `ports/autoinstall` folder on the card:
   - **RG28XX and RG35XX 2024:** on the **GAMES** drive ([The GAMES drive](games-drive.md)).
   - **Other pocket-ui OS cards:** on the card's small first partition, or on your second games card (TF2, as on the
     R36S).

Put the card back in, open **PortMaster games** (or press A on an empty Ports shelf), and press A to **Install**.
Runtimes install into PortMaster first, then each game sets up just like it came from the online catalogue: it shows up
under **Ports** with controls, the pause menu and its own game files page. Each file you copied (game zip, runtime or
runtime bundle) is removed from the card once it is installed.

The screen lists every file it found and explains if anything can't be installed yet. Skipped files stay on the card:

| Note | What to do |
|---|---|
| **Needs a runtime** | Download the named runtime from **Runtimes**, copy it into `ports/autoinstall`, and press X ("Look again"), or A when nothing else is ready. |
| **Not for this handheld** | Built for PCs or a different handheld: download the **aarch64** version instead. |
| **Damaged** | The copy is incomplete or damaged: copy the file onto the card again. |
| **Not a game** | The zip isn't a PortMaster game or runtime pack. |

### Games that need your own files

In **Bring your own game files**, press **X** ("How to copy"), or finish installing, to open **Copy your game files**:

- **On this Pocket:** exact destination folder (e.g. `/storage/roms/ports/sprout/assets`), parsed from PortMaster's
  instructions.
- **From a computer:** copy straight into `ports` over File sharing (press X right on this screen to toggle File sharing
  and view credentials):
  - Windows: `\\<address>\games-roms\ports\sprout\assets`
  - macOS / Linux: `smb://<address>/games-roms/ports/sprout/assets`
- **From a browser:** upload directly via the [web companion](web-companion.md) over home Wi-Fi or Tailscale.
- **PortMaster says:** full official instructions (scroll with Up/Down).

If instructions call for multiple subfolders (like `valve` and `cstrike` for Counter-Strike), drop them into the game's
root folder shown. Files left in `port-files` from older versions stay untouched; move them into the game folder.

### When the files are missing

Try launching a port whose required commercial files are clearly missing and pocket-ui opens **Copy your game files**
("… can't start yet") instead of a black screen. pocket-ui only intervenes when it's certain (empty/placeholder target
folder, untouched since install, never ran for a minute); otherwise it launches normally. Note that many ports unpack or
convert your files on first launch, so taking a few minutes and sometimes emptying the drop folder afterwards is completely
normal.

## Tools

Settings → Help & tools → **Tools** launches ROCKNIX's built-in utilities and installed package tools directly:

- **Emulator:** native menus for **RetroArch**, **Dolphin**, **PPSSPP**, **Azahar**, **ARMSX2**, DuckStation, melonDS,
  Flycast, ScummVM and more.
- **Controls:** **Test gamepad**, **Calibrate gamepad** (fixes stick drift) and **Test touch screen**.
- **System:** **File manager**, **Terminal** (USB keyboard needed), Mount NFS, ScummVM/id Tech/Heroic scanners,
  Steam/Heroic installers, Remove `._` files and cloud backup/restore.
- **Store:** store-installed utilities (output shown right on the page).
- **Unknown:** custom scripts in ROCKNIX's tools folder.

Press **A** to launch (destructive or network tools ask for a second A in red; **X** rescans). Exit a tool via its own
Quit menu, **L1 + SELECT + START** (where supported), or by holding **HOME for 1 second** and releasing. Anything that
could flash the bootloader, update the OS or wipe partitions (like ROCKNIX's installer) is intentionally hidden to keep
your Pocket bootable.

Changes made inside standalone emulator menus affect those standalone emulators only (not pocket-core). Calibrate
gamepad temporarily pauses menu inputs while running, and **Scan ScummVM games** restarts pocket-ui cleanly when done.
