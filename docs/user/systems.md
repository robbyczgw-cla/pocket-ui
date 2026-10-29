# Systems

pocket-ui shows every system ROCKNIX knows about that has at least one game in its folder. Each game
is started with one of three kinds of emulator:

- **pocket-ui's own emulator host** (pocket-core). It is built for this device: whole-number
  scaling, a full pause menu with eight save slots and options, shortcuts, and continuing where you
  left off. In our measurements it needed less processor time than RetroArch on most systems, for
  example about half on Game Boy Advance and SNES. Its sound plays with a short delay: about 45 ms
  from the emulator to the sound output in our measurements.
- **RetroArch**, ROCKNIX's standard emulator frontend, with the emulator ROCKNIX picks for that
  system.
- **Standalone emulators** that ROCKNIX ships for larger systems, such as PlayStation 2, GameCube
  or 3DS.

pocket-ui uses ROCKNIX's default emulator for every system that is not on its own host, unless you choose
another one: press START on a game's card or on a console page, or open Options in the pause menu, and pick it under
[Runs on](options.md#runs-on), for one game or for the whole console. A choice made earlier in ROCKNIX's
EmulationStation (its "Emulator" setting per system or per game) is used too, and a choice made in pocket-ui shows
up there.

Most consoles also have a [console page](playing.md#console-pages) (X on the system) with their facts, and from
there [Credits & links](playing.md#credits--links): who designed and made them, their chips, and links to the
consoles before and after them and to the ones with the same chips that you have games for.

## On pocket-ui's own host

| System | Folders | Emulator core |
|---|---|---|
| Game Boy | `gb`, `gbh` | Gambatte |
| Game Boy Color | `gbc`, `gbch` | Gambatte |
| Game Boy Advance | `gba`, `gbah`, `gbav` | mGBA |
| NES / Famicom | `nes`, `famicom`, `nesh` | Nestopia |
| Famicom Disk System | `fds` | Nestopia (needs a BIOS, see below) |
| SNES / Super Famicom | `snes`, `sfc`, `snesh` | Snes9x |
| Mega Drive / Genesis | `megadrive`, `megadrive-japan`, `genesis`, `genh`, `megadriveh` | Genesis Plus GX |
| Master System | `mastersystem` | Gearsystem |
| Game Gear | `gamegear`, `ggh` | Gearsystem |
| SG-1000 | `sg-1000` | Gearsystem |
| PC Engine / TurboGrafx-16 | `pcengine`, `tg16` | Beetle PCE Fast |
| PC Engine SuperGrafx | `supergrafx` (HuCards, see below) | Beetle SuperGrafx |
| WonderSwan, WonderSwan Color | `wonderswan`, `wonderswancolor` | Beetle WonderSwan |
| Atari 2600 | `atari2600` | Stella |
| Virtual Boy | `virtualboy` | Beetle VB |
| PlayStation | `psx` | PCSX ReARMed |
| Nintendo 64 | `n64` | Mupen64Plus-Next |
| PSP | `psp` | PPSSPP |
| Dreamcast | `dreamcast` | Flycast |
| Nintendo DS | `nds` | melonDS DS |
| Sega Saturn | `saturn` | YabaSanshiro (needs a Saturn BIOS, see below) |
| DOS | `pc` | DOSBox Pure |
| ScummVM | `scummvm` (see below) | ScummVM |
| PICO-8 | `pico-8` | FAKE-08 (unless you added the official PICO-8, see below) |
| TIC-80 | `tic-80` | TIC-80 |

Only these folder names use the own host; games in other folders start with ROCKNIX's default emulator. Games
packed as `.zip` work (with one game inside: see [a zip file that won't start](troubleshooting.md#a-zip-game-says-it-holds-several-games-is-damaged-or-cant-be-opened)); games packed as `.7z` start with RetroArch. The folders ending in `h` (`nesh`, `snesh`, `gbh`,
`gbch`, `gbah`) are ROCKNIX's folders for hacks and translations, `gbav` is for Game Boy Advance Video cartridges.

Apart from the Saturn and the Famicom Disk System, none of these systems needs a BIOS file. Dreamcast uses
`roms/bios/dc/dc_boot.bin` when it is there, as RetroArch does, and starts without it too.

**Famicom Disk System**: put the Disk System's BIOS, `disksys.rom`, into `roms/bios`. Without it a disk game doesn't
start, here or in RetroArch, and a card on the shelf names the file. Disks as `.fds`. Many disk games ask you to turn
the disk over: press **L1** to turn it to the other side (R1 ejects it). The Controls page in the pause menu shows
both. What a game saves to its disk is kept next to the disk file, where RetroArch keeps it too.

**PC Engine SuperGrafx**: HuCards (`.pce`, `.sgx`) run on the own host. CD games in the same folder (`.cue`, `.ccd`,
`.chd`) need a System Card BIOS and start with RetroArch, like games in `roms/pcenginecd`.

**Virtual Boy**: the picture is one eye's image in red on black, the way the Virtual Boy looked with one eye closed;
3D needs the goggles, which this screen can't be. The left D-pad is the D-pad (or the left stick), the right D-pad is
the right stick, or L2 (up), R2 (left), L3 (down) and R3 (right). Under Options → Emulator, **Palette** changes the
red to white or another colour, **Anaglyph preset** shows both eyes in two colours for red/blue or red/cyan 3D glasses,
and **3D mode** set to side-by-side puts both eyes next to each other (small, for cross-eyed viewing; it applies at the
next start).

**Sega Saturn**: the own host needs one of these files in `roms/bios`: `saturn_bios.bin`, `sega_101.bin` or
`mpr-17933.bin`. Without one, Saturn games start with ROCKNIX's standalone YabaSanshiro as before, which has a
built-in replacement BIOS; nothing else changes and no message appears. Games as `.cue`, `.chd` or `.iso`. The
Saturn pad on the Mini V2: A, B, X, Y on the face buttons (Saturn A is the bottom button, B the right one, X the
left one, Y the top one), C on L1, Z on R1, Saturn L and R on L2 and R2, and the left stick works as the D-pad. The
Controls page in the pause menu shows these names and can change them. In-game saves are not shared with
standalone YabaSanshiro yet: a game saved there doesn't see that save here, and the other way round.

**PlayStation**: the own host takes games as `.cue`, `.chd`, `.m3u` and `.pbp`. The left stick and the right
stick go to the game as analogue sticks (see [Controls](options.md#controls)). A game on several discs changes discs
in the pause menu ([below](#games-on-several-discs)).

**Nintendo 64, PSP and Dreamcast**: both sticks go to the game as analogue sticks. Game saves are shared
with the emulators ROCKNIX used for these systems before: RetroArch's N64 saves, the PSP save data in
`roms/psp/PSP/SAVEDATA` and each Dreamcast game's memory card, so a game saved there continues here and
the other way round.

**Nintendo DS**: both screens are shown one above the other, as large as they fit. R3 switches to side by
side and to the top screen alone. Touch the bottom screen with a finger, as you would with the stylus; the top screen takes no touch. Or use the right stick
as a pointer on the touch screen and R2 to touch; the left stick stays the D-pad. Game saves are the same `.sav` files the standalone melonDS uses.

**DOS**: a game can be a `.zip`, a `.dosz`, an `.exe`, `.com` or `.bat` in `roms/pc`, or a folder in `roms/pc`
with the game's files; each shows up as one game. DOSBox Pure starts the game if there is only one program to
start, otherwise it shows its own start menu to pick one. The touch screen works like a laptop's touchpad: drag
to move the mouse pointer, tap to click, tap with two fingers for the right button. **L3** (press the left stick)
opens DOSBox Pure's on-screen keyboard; tap its keys, or use the D-pad and A. Its tabs also hold the start menu
and the controller mapper. The left stick goes to the game (joystick or keys, as DOSBox Pure maps it for that
game), the right stick moves the mouse. The picture is 4:3, as on a monitor of the era. Save states work, and
DOSBox Pure keeps what a game writes to its disk in `roms/pc/<game>.pure.zip`, as with RetroArch.

**ScummVM**: point-and-click adventures. pocket-ui lists the games ROCKNIX's "Scan ScummVM Games" tool found
(its list in `/storage/.config/scummvm/games`) and, without scanning, every other folder in `roms/scummvm`:
put each game into a folder of its own there. Tap where you want to click; A clicks, B is the right mouse
button, L1 skips a line of dialogue, Y is Escape (skips a scene), X is F5 (the game's menu in many games),
SELECT opens ScummVM's on-screen keyboard, and START
opens ScummVM's own menu. The sticks and the D-pad move the pointer, R2 slows it down. ScummVM has no save
states: Save and Load in the pause menu only say so. Save in the game, or with START → Save; those saves are in
the same place standalone ScummVM keeps them, so a game saved there continues here.

**PICO-8 and TIC-80**: carts as `.p8` or `.p8.png` (PICO-8) and `.tic` (TIC-80). On PICO-8, A is the X
button and B the O button. On TIC-80 a tap on the screen is the mouse. Neither has working save states: the
emulators' states don't keep the game itself, so Save and Load in the pause menu only say so, and a game
starts from the beginning each time. Games that save on their own (high scores, progress) still do.
If you put the official PICO-8 for Linux (`pico8_64`) into `roms/pico-8/aarch64` (or `roms/pico-8`), PICO-8
games start with it, as ROCKNIX does, with Continue, Options, Quick settings and Quit only in the pause menu. Without it, a cart that
doesn't start on the own host goes to RetroArch's FAKE-08.

Pause menu: Continue, Save, Load, Options, Quick settings and Quit, with Achievements when you are signed in to
RetroAchievements. ScummVM, PICO-8 and TIC-80 have no save states, so their Quit has no moment to keep. See
[The pause menu](pause-menu.md).

<!-- TODO: Master System, Game Gear, SG-1000, WonderSwan and Atari 2600 were checked only with
     self-made test ROMs, not real games (docs/HOST.md). Famicom Disk System, SuperGrafx, Virtual Boy
     and the second folders (famicom, sfc, the hacks folders) were checked in the virtual device only
     (docs/HOST.md "More systems"). Sega CD, 32X, PC Engine CD, Neo Geo Pocket, Lynx, Atari 7800,
     Pokemon mini, FBNeo arcade, NAOMI and Atomiswave are wired in src/hostsys.rs but not verified,
     so they stay on their default emulator. Update the table as systems are verified. -->

### Games on several discs

A PlayStation game that comes on several discs is one game on the shelf when its discs are listed in an `.m3u`
playlist, or when it is a single multi-disc `.pbp` file. Then the pause menu has a **Disc** row: Left/Right choose a
disc, A puts it in and goes back to the game ([Changing discs](pause-menu.md#changing-discs)). The same goes for other
disc consoles on the own host whose emulator can change discs.

To make an `.m3u`, put the game's discs in a folder of their own inside `roms/psx` and add a plain text file next to
them that lists their file names, one per line, in order:

```
roms/psx/Final Fantasy VII/
    Final Fantasy VII (Disc 1).chd
    Final Fantasy VII (Disc 2).chd
    Final Fantasy VII (Disc 3).chd
    Final Fantasy VII.m3u
```

`Final Fantasy VII.m3u` holds:

```
Final Fantasy VII (Disc 1).chd
Final Fantasy VII (Disc 2).chd
Final Fantasy VII (Disc 3).chd
```

For `.cue` discs, list the `.cue` files (not the `.bin` files). The shelf then shows one game, named after the
folder, and hides the single discs. Without an `.m3u` each disc is a game of its own and the menu has no Disc row.

A file name written all in lower case gets capitals on the shelf: "libbet and the magic floor" shows as "Libbet and the Magic Floor". A name with any capital letter is shown as the file has it.

### When a game does not start on the own host

If pocket-core cannot start a game, a card at the top of the shelf says why for about eight
seconds.

When the game failed within its first 30 seconds without you quitting it, pocket-ui starts it again
right away with ROCKNIX's default emulator for that system and says so, for example: "This game didn't
start on pocket-core, so it runs with RetroArch from now on." (for a Saturn game: "…with YabaSanshiro…", for
ScummVM "…with ScummVM…", ROCKNIX's standalone ScummVM).
The game is remembered and starts with that emulator from then on; the other games of that system stay
on the own host. Choosing pocket-core for it again under [Runs on](options.md#runs-on) gives it another try.

A missing BIOS file is the exception: RetroArch would need it as well, so pocket-ui does not switch.
The card names the files it looks for in `roms/bios`. The Saturn is different again: without its BIOS,
its games go to standalone YabaSanshiro from the start (see above).

## On RetroArch

Every other system runs on RetroArch, for example Sega CD, 32X, PC Engine CD, Neo Geo Pocket, Lynx,
Atari 7800 and arcade boards. So do `.7z` games, SuperGrafx CD games, and games the safety net above has sent to
RetroArch.

Arcade games need a game ZIP made for the core that plays them, and often a parent or BIOS ZIP: see
[Arcade games](arcade.md). Translations and hacks that come as `.ips`, `.bps` or `.ups` patch files, and MSU-1 games, are
in [Patches, translations and MSU-1](patches-msu1.md). pocket-core applies a patch that sits next to the game with its name.

<!-- TODO: confirm each system's default emulator against es_systems.cfg on the device; the
     examples above come from the developer notes (docs/PER-CONSOLE.md, docs/HOST.md). -->

Pause menu: Continue, Save, Load, Options, Quick settings, Quit. Options opens [Runs on](options.md#runs-on): the
emulator, and RetroArch's aspect ratio, integer scaling, smoothing, shader, rewind, run-ahead and auto save, which
RetroArch reads the next time the game starts.

## PlayStation 2, GameCube, Wii and 3DS

PlayStation 2 runs on ARMSX2, GameCube and Wii on Dolphin, 3DS on Azahar, all ROCKNIX's standalone
emulators.

Pause menu: Continue, Save, Load, Options, Quick settings, Quit (it follows Settings →
[Remember where I quit](settings.md#remember-where-i-quit)).

- **Save** and **Load** use the emulator's own slots 1 to 8, plus Auto and Before load (on 3DS: Azahar's one quick
  slot, the one ROCKNIX's HOME + R1 and HOME + L1 use). See
  [Save slots](pause-menu.md#save-slots-auto-and-before-load).
- **Options** sets resolution, renderer, aspect ratio and more for the game; see
  [Options](options-big.md).
- **Quit** saves the moment, then closes the emulator ("Saving and quitting…"). This takes a few
  seconds; pocket-ui waits up to 35 seconds before it stops the emulator. On 3DS the game comes back
  to the screen for the moment it saves, then closes.
- The next start of the game continues from the moment you quit. The game boots for a moment
  first.

Not every game file gets the full menu: Wii channels (`.wad`) and 3DS games in `.cia` or in Azahar's
compressed formats (`.zcci`, `.zcxi`, `.zcia`) have Continue, Options (Runs on), Quick settings and Quit only.

Wii games are played with the controller standing in for a Wii Remote, as ROCKNIX sets it up. Games
that need the pointer or motion (aiming at the screen, shaking, tilting) are hard or impossible to
play this way; that does not change with pocket-ui.

On 3DS both screens are shown one above the other, as ROCKNIX sets it up; the touch screen is the
lower one.
<!-- TODO: Wii and 3DS Save/Load/Quit/continue are implemented but not yet confirmed on the device
     (docs/BIG-EMULATORS.md, "Wii and 3DS"). -->

The first time you play each system, pocket-ui sets the emulator up for this screen:

- PlayStation 2: Vulkan, internal resolution 2×, widescreen patches off.
- GameCube: Vulkan, internal resolution 2×, shaders that never pause the game (Dolphin's asynchronous mode), and
  GameCube A on the right button (both only if nothing was chosen
  in ROCKNIX's settings; change them in the pause menu, Options → More…). Wii gets the same shaders.

This happens once, so changes you make later in the emulator or in ROCKNIX's settings stay. A few
settings the pause menu depends on are set on every start; for example, on PlayStation 2 HOME is
not also the controller's Analog button.

GameCube and Wii games started with Dolphin's other version (Qt) have Continue, Options (Runs on), Quick settings and
Quit only.

## Xbox

*Experimental.* Original Xbox games go into `roms/xbox` as `.iso` disc images (XISO, no BIOS or hard disk image needed).
pocket-ui plays them with **hakuX** in RetroArch; the core is part of the pocket-ui install and sits in
`/storage/pocket-ui/hakux`. Without it, ROCKNIX's xemu stays the default. Pick the emulator under
[Runs on](options.md#runs-on).

- **Pause menu:** Continue, Options, Quick settings, Quit. HOME opens it over the game. [Options](options-big.md#xbox-experimental): Resolution
  (Original 480p or 2x 960p) and Screen scaling (Aspect, Fullscreen, Integer); both apply the next time the game starts.
- **Saves:** the game's own saves work and stay in pocket-ui's folder, never in `roms/xbox`. **Save states do not exist yet**
  (no Save, Load or "continue where I left off"); the rows appear when the core can do it.
- Not every game runs, and the speed varies; sound effects of some games may be missing. This is a young port. Please report
  what you find.
- Credits: hakuX by rfandango (GPL-3.0), Linux/libretro port by portare-ch (PortareOS), PR #9 commit 2de4e9c (it includes an audio fix of ours). The licence
  and source link are in `/storage/pocket-ui/licences/NOTICE.md` (Settings → System → About names the folder).

## On standalone emulators

Other large systems, for example Wii U, PlayStation 3, PlayStation Vita, and WiiWare, use standalone
emulators without pocket-ui's save support. Wii U (Cemu), PlayStation 3 (RPCS3) and PlayStation Vita (Vita3K) are experimental and exist on
the SM8550 handhelds only (AYN Odin 2 family, Retroid Pocket 6/Nova): HOME opens pocket-ui's pause menu over the game, and
Quit closes the emulator cleanly. So do the NAOMI and Atomiswave arcade boards, on ROCKNIX's standalone
Flycast; they need MAME's BIOS sets in `roms/bios/dc` (`naomi.zip`, `naomi2.zip` for NAOMI 2 games, `awbios.zip`).

PlayStation Vita (Vita3K) needs the console's firmware, which you install yourself: put the `.PUP` files in `roms/bios/vita3k`, run
the tool **Start Vita3K** once (Settings → Games → [Tools](extras.md#tools)), and then add your games to `roms/psvita` (`.psvita`, or `.pkg` with a `.txt`
holding its key, or `.zip`). pocket-ui shows the games with their titles, covers and the small blue-topped Vita case.

Pause menu: Continue, Options (Runs on), Quick settings and Quit. These emulators keep their own saves and settings; use their
in-game save feature before quitting. Continuing where you left off is not available for them.

Nintendo 64, PSP, Dreamcast and DS moved to pocket-ui's own host, and the Saturn does when its BIOS is there; it measured at least as well as
ROCKNIX's emulators for them (docs/HOST.md). A game that does not start there still starts with
ROCKNIX's emulator for its system.

## Folder names

Games go into `roms/<folder>`, one folder per system. The folder names are ROCKNIX's; the ones above
are the most common. The full list is the set of folders on the SD card under `roms`.
<!-- TODO: add a complete table of folder names and file types, generated from es_systems.cfg. -->
