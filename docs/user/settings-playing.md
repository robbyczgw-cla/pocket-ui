# Settings: Playing and Games

The **Playing** and **Games** sections of [Settings](settings.md): startup and quit behaviour, overlays, performance,
achievements, adding games, box art, shelf space and Curator. For Look and how Settings opens, see
[Settings](settings.md#open-settings).

## Playing

Settings → **Playing** controls how games launch, pause, save and run.

| Entry | What it holds |
|---|---|
| **On start** | What opens when you power on: **Home** (default) or **Last game** (jumps straight back into your last game). Toggle with A or Left/Right. See [On start](#on-start) |
| **Continue where I quit** | Whether Quit saves your exact moment for next time: **On** (default) or **Off** (in-game saves only). Toggle with A or Left/Right. See [Continue where I quit](#continue-where-i-quit) |
| **Console boot screen** | Play the original console boot logo on a fresh start (GB, GBC, GBA, PS1, Dreamcast, Saturn, Mega-CD, PC Engine CD; requires your own BIOS files): **Off** (default) or **On**. Toggle with A or Left/Right. See [Console boot screen](#console-boot-screen) |
| **Remind me where I was** | Returning to a game after 2+ weeks? Shows when you last played, play time and your note while it loads: **Off** (default) or **On**. Toggle with A or Left/Right. See [Remind me where I was](#remind-me-where-i-was) |
| **Screen frames** | Automatic default frame when you haven't picked one: **Off** (default), **Handhelds** or **Every console**. Cycle with A or Left/Right. See [Screen frames](#screen-frames) |
| **Over the game** | On-screen stats while playing: **Off** (default), **FPS**, **FPS + sensors**, or **Pixel Path** (when enabled in Curator). See [Over the game](#over-the-game) |
| **Zoom in the pause menu** | Pin Zoom onto the first page of the pause menu: **Off** (default) or **On** (always available under Options, and auto-shown with Big text). Toggle with A or Left/Right. See [Zoom in the pause menu](#zoom-in-the-pause-menu) |
| **Performance & fan** | Default CPU speed, game and menu fan curves, Smooth fan and live temperatures. See [Performance and fan](performance.md) |
| **RetroAchievements** | Sign in to retroachievements.org to unlock achievements in pocket-core games. See [RetroAchievements](#retroachievements) |

### On start

Settings → Playing → **On start** chooses what greets you on power-on:

- **Home** (default): opens to your shelf.
- **Last game**: drops straight back into the last game you played, just like waking a console from standby (the shelf
  flashes briefly before fading into your saved frame).

To avoid crash loops, **Last game** only triggers on a clean boot after a normal exit; if a game errored out, pocket-ui
restarted after a crash, power cut out unexpectedly or you just updated pocket-ui, you land safely on Home instead.

Leaving a game via **Power off** in the power menu always reopens that game next boot regardless of this setting
([Sleep and power](quick-settings.md#sleep-and-power)).

### Continue where I quit

Settings → Playing → **Continue where I quit** controls whether **Quit** in the pause menu saves an auto-state (toggle
with A):

- **On** (default): Quit saves your exact spot ("Saving and quitting…"), so your next launch (and **Last game** under
  [On start](#on-start)) picks right up there.
- **Off:** Quit flushes only in-game saves and memory cards ("Quitting…"). In pocket-core, your next start boots fresh
  (your previous auto-state stays under **Load → Where I quit** until overwritten). On PS2, GameCube, Wii and 3DS, the
  next launch still resumes from your last saved Quit/sleep moment. See [The pause menu](pause-menu.md#the-pause-menu-home)
  for per-emulator details.

Applies to pocket-core plus PS2, GameCube, Wii and 3DS. **RetroArch games always auto-save on Quit** regardless of
Continue where I quit (ROCKNIX configures RetroArch that way). Even with this Off, putting your handheld to sleep,
using Power off or switching through [Recent games](playing.md#recent-games) still saves your spot safely.

### Console boot screen

Settings → Playing → **Console boot screen** (**Off** by default; toggle with A) plays the real console boot animation
on a fresh start (Game Boy, GBC and GBA Nintendo logos, the PlayStation startup, Dreamcast swirl, Saturn and Mega-CD
intros, and the PC Engine CD System Card), right after the TV or handheld screen switches on.

You supply your own dumped BIOS files inside `roms/bios` (pocket-ui ships none), matching these exact files and sizes:

| Console | File | Size |
|---|---|---|
| Game Boy (and non-Color games) | `gb_bios.bin` | 256 bytes |
| Game Boy Color (Color-capable titles) | `gbc_bios.bin` | 2304 bytes |
| Game Boy Advance | `gba_bios.bin` | 16384 bytes |
| PlayStation | `scph5501.bin` or another standard PS1 BIOS (`scph1001.bin`, `scph5500.bin`, …) | 512 KB |
| Dreamcast (Flycast) | `dc/dc_boot.bin` (inside `roms/bios/dc/`) | 2 MB |
| Saturn | `saturn_bios.bin`, `sega_101.bin` or `mpr-17933.bin` (Beetle Saturn uses the last two) | 512 KB |
| Mega-CD | `bios_CD_E.bin`, `bios_CD_U.bin` or `bios_CD_J.bin` matching disc region (`(Europe)`, `(USA)`, `(Japan)`) | 128 KB |
| PC Engine CD | `syscard3.pce` | 256 KB |

Missing a BIOS? The game still launches normally (where possible) and briefly tells you which filename it looked for.
Consoles without a boot screen (SNES, Mega Drive, N64, …) launch straight in as usual. Dreamcast, Saturn, Mega-CD and
PC Engine CD always use your real BIOS once present (Mega-CD and PC Engine CD require one); turning this On stops any
built-in HLE shortcut from skipping the logo. Nintendo DS isn't included because DS emulators either boot direct or stop
at the DS touchscreen menu.

On Android's Handheld view, this switch works identically using the BIOS folders you set up (Game check copies them in).

Boot screens only play on a **fresh start**, never when resuming a saved state. Want to boot straight into your last
game through its classic startup logo every time? Turn **Continue where I quit** **Off** and set [On start](#on-start)
to **Last game**; your in-game battery and memory card saves stay intact as usual.

### Remind me where I was

Settings → Playing → **Remind me where I was** (**Off** by default; toggle with A) shows a quiet recap card whenever you
resume a game you haven't touched in over two weeks: when you last played, total play time and your own game-card note.
In pocket-core, the game holds still on its resumed frame until you press any button to jump right in. See
[Where was I?](continue.md#where-was-i).

### Over the game

Settings → Playing → **Over the game** chooses the live overlay drawn during gameplay: **Off** (default), **FPS**,
**FPS + sensors**, or **Pixel Path** (available when Settings → Games → Curator → **Pixel Path in the pause menu** is
On; enabled by Enthusiast). See [Over the game](pause-menu.md#over-the-game) for what each displays. Cycle with
Left/Right or A. Per-game or per-console choices under Options → Speed, rumble, fan take priority, and **HOME + Y**
cycles levels live mid-game. The [Easy](easy.md) layout keeps overlays hidden without changing your setting.

### Zoom in the pause menu

Settings → Playing → **Zoom in the pause menu** (**Off** by default; toggle with A or Left/Right) adds **Zoom** right to
the first page of pocket-core's [pause menu](pause-menu.md) above Quit. Even when Off, Zoom stays accessible under
Pause menu → Options and via the optional **HOME + X** [shortcut](options.md#shortcuts), and appears automatically on the
first page whenever [Big text](big-text.md#big-text) is On. See [Zoom](big-text.md#zoom).

### Screen frames

Settings → Playing → **Screen frames** picks which systems get an automatic
[screen frame](options-screen.md#screen-frame) when you haven't chosen one manually (cycle with A or Left/Right):

- **Off** (default): clean gameplay on black bars.
- **Handhelds**: handheld consoles (Game Boy, GBA, Game Gear, …) default to Auto frames; home consoles stay clean.
- **Every console**: every system starts with Auto: monitor surrounds for home consoles and the DS frame for Nintendo DS.

Any frame saved for a specific game or console under Pause menu → Options → Picture always wins. Applies on your next
game launch.

### Performance and fan

Settings → Playing → **Performance & fan** sets default CPU clocks, game and menu fan speeds, **Smooth fan** and live
hardware readouts. Full guide: [Performance and fan](performance.md).

### RetroAchievements

Settings → Playing → **RetroAchievements** links your free retroachievements.org account so pocket-core games track and
unlock achievements. Full guide: [Achievements](achievements.md).

1. Select **Sign in**, press A, enter your username (START), then your password (START). Requires Wi-Fi.
2. Once signed in, the page shows **Signed in as** your username. Your password is never stored; only the returned login
   token.
3. **Log out** asks twice and clears the token from your Pocket; all unlocks remain on retroachievements.org.

In the Playing menu, the row shows your username when connected or "Off" when signed out.

## Games

Settings → **Games** manages your library, box art, visible consoles, Shelf space and Curator.

| Entry | What it holds |
|---|---|
| **Add games** | System grid showing folders, file extensions and BIOS needs, plus File sharing, From your phone (Y) and unreadable-file notices. See [Getting started](getting-started.md#the-systems-grid) and [From your phone](phone-upload.md) |
| **Box art & downloads** | Box art, Pictures downloads and Starter pack, plus PortMaster games (ROCKNIX/TrimUI) and optional Cheats (TrimUI). *(Previously called Extras.)* See [Box art & downloads](extras.md) |
| **Game check** | Scans your card and reports missing BIOS files, bad zips, broken disc sheets or duplicates in plain English (press A to hide duplicates). See [Game check](game-check.md) |
| **Systems on the shelf** | Show or hide individual consoles on Home (displays "1 hidden" etc. when any console is unchecked). See [Systems on the shelf](#systems-on-the-shelf) |
| **Shelf space** | **Off** (default), **10**, **20** or **50**; limits your active shelf to a curated handful while tucking the rest into a crate underneath. Cycle with A or Left/Right. See [Shelf space](#shelf-space) |
| **Curator** | Individual switches for Lens, smart shelves, Just Play, Echoes, Versions, Pixel Path and Play history. See [Curator](#curator) |
| **Screenshots** | Browse screenshots taken with HOME + A grouped by day (A views full screen, X deletes). Top row **My shelf as a picture** renders a shareable 1440×1440 square portrait of up to 20 games (favourites, stickered and boxed titles first) with trophy, plant and a small pocket-ui mark. See [Looking at your screenshots](playing.md#looking-at-your-screenshots) |
| **From EmulationStation** | Imports favourites and star ratings from existing `gamelist.xml` files without touching them (shows "No gamelist" if none exist). See [Your gamelists](library-files.md#your-gamelists) |
| **Android games** | pocket-ui for Android only: pick Android apps to place on an Android shelf beside your consoles (shared with Mobile view) |
| **RomM** | Connect your self-hosted RomM server via QR code (5.0+) or token code (4.8/4.9) so remote games stand on your shelf and download on first launch. Shows game count or "Not paired", plus Refresh and Unpair. See [Games from your RomM server](romm.md) |

### Systems on the shelf

Settings → Games → **Systems on the shelf** lists every console that has games on your card, all checked by default.
When Moonlight is installed, **PC streaming** sits unchecked at the bottom; tick it to put
[PC streaming](pc-streaming.md) on Home.

- Press **A** to hide a system from Home, recent games and the game switcher (SELECT); press A again to bring it back.
- Nothing is deleted. **Starred games stay in Favourites** even if their console is hidden, and recent history returns
  immediately when you re-enable the console.
- At least one console must stay checked.
- Saves immediately to `settings.json` and survives reboots and updates. Press **B** to return.

First-start setup leaves every system checked by default. If your card has no games yet, the page explains how systems
appear once games are copied over.

### Shelf space

Settings → Games → **Shelf space** cures choice paralysis: keep just **10**, **20** or **50** games out on your shelf
while the rest sit neatly in the **crate**, as cardboard boxes on the floor underneath, one per console. **Off** (default)
shows your whole library on the shelf.

- **Turning it on:** choose whether to stock your shelf with **The games I played last** or **My favourites** (filled
  alphabetically to your limit; Cancel leaves it Off).
- **Home** adds a **Crate** entry before your consoles ("12 games in the crate, the shelf is full"; appears as a
  cardboard box on the shelf in the Shelf layout). Console rows show only shelved games and hide consoles whose games
  are all boxed up; Search still finds every game.
- **Opening the crate:** browse boxes with Left/Right as each highlighted case rises out with its cover. Press **A** to
  pull it onto the shelf. If the shelf is full, it lifts into view with your least-recently-played game selected to swap
  out (Left/Right picks another, **A** swaps them into the box and onto the shelf, **B** cancels).
- **From a game card:** pick **Into the crate** (above Delete from the card) to box a game and free up a shelf slot.
- Changing between 10, 20 and 50 preserves your current shelf (topping up open slots with recent games). Turning it
  **Off** puts everything back on the shelf while remembering your curated selection if you turn it back on later.
- Newly copied games land in the crate; deleting a shelved game frees its spot. An empty crate simply shows an empty box
  confirming everything is on the shelf.

Works across Shelf, Split and Cabinet layouts (Easy displays only your shelved games). Saved in `shelf_space.json`.

### Curator

Settings → Games → **Curator** toggles each piece of [Curator](curator.md#curator) individually (press A or Left/Right;
changes apply live):

| Row | Default | What it does |
|---|---|---|
| **Lens & smart shelves** | Off | Adds Curator (with Lens and pinned smart shelves) to Home. Starts On in Enthusiast or on upgraded pre-switch setups |
| **Just Play on Home** | On | Puts Just Play on Home: A launches a smart pick immediately, X previews it with a reason ([Just Play](curator.md#just-play)). Stays Off after upgrading only if Lens was previously Off |
| **Echoes on the game card** | Off | Shows "More like this" recommendations on every game card |
| **Merge versions in mixed lists** | Off | Combines multi-console copies of the same game into one entry in Favourites, smart shelves and Echoes, launching your preferred version |
| **Games not on this device in Echoes** | Off | Lets Echoes suggest a few matching titles you don't own yet (names only, no store) |
| **Pixel Path in the pause menu** | Off | Adds [Pixel Path](pause-menu.md#pixel-path) to the pause menu and unlocks the Pixel Path overlay under [Over the game](#over-the-game) (falling back to FPS when Off) |
| **Play history** | On | Powers Curator → History and Year in review on-device ([Play history](curator.md#play-history)). Turning it Off pauses recording without deleting past history |

Turning any piece off simply hides it; your smart shelves and preferred versions wait safely until you switch it back on.
