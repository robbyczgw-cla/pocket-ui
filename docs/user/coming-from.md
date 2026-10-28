# Coming from ROCKNIX or Android

You already play on this handheld, or on another one, and have saves, favourites and settings you want to keep. This
page says what carries over, what needs a copy, and what does not come with you. Where something has not been tried on
a real device, it says "should" and why.

## The short answer

| You have | What happens |
|---|---|
| **In-game saves** (the kind a cartridge keeps: `.srm`, `.sav`) | Work as they are when the same emulator core plays the game, and the file sits next to the game with the same name. |
| **Memory cards** of PlayStation 2, GameCube, Wii and 3DS | Work as they are on ROCKNIX; from Android they should work if you copy them to the right place (below). |
| **Save states** | Belong to one emulator. They do not move between RetroArch and pocket-ui's own host, or from Android. Use the in-game save. |
| **Favourites and ratings** from EmulationStation | pocket-ui offers them: Settings → Games → **From EmulationStation**. Nothing is taken over by itself. |
| **Names, descriptions, box art** from a `gamelist.xml` | Read as they are, and never changed. |
| **Settings, play time, recent games, achievements** | Do not come over. You set them again; the first start helps. |

## Before you switch

1. **Save in the game itself**, with the game's own menu, not only in a save state. The in-game save is the file that
   moves between emulators. A state does not.
2. **Quit the old emulator the normal way**, so it writes its saves. Do not copy files while it still runs.
3. **Copy your saves before you wipe anything**, and keep that copy until you have started each game once and seen your
   progress.
4. **Keep the file names.** A save belongs to its game by name: `Pokemon Emerald.gba` and `Pokemon Emerald.srm`. If
   you rename a game, rename its save the same way.
5. **Favourites from an Android launcher** (Daijishō, the Retroid launcher) have no file pocket-ui can read. Write the
   list down. ES-DE keeps favourites in `gamelist.xml`, which pocket-ui does read (below).

## Coming from stock ROCKNIX (EmulationStation and RetroArch)

### On the same card

pocket-ui replaces ROCKNIX's menu and nothing else. The package for a card that already runs ROCKNIX puts pocket-ui on
it and keeps games, saves and settings; see the [install guide](../INSTALL-DUAL-BOOT.md#install-on-a-rocknix-card-you-already-use).
It was tried against a made-up system and in the virtual device, not yet on a real card. Your `roms` folder stays where it
is, so everything in the table is already in place.

| What | Where | File |
|---|---|---|
| In-game save | `roms/<system>/`, next to the game | `<game>.srm`, for Nintendo DS `<game>.sav` (DraStic's `<game>.dsv` is taken over once) |
| Cartridge clock (Pokémon Gold, Silver, Crystal) | next to the save | `<game>.rtc` |
| RetroArch's save state | `roms/savestates/<system>/` | `<game>.state`, and `<game>.state.auto` for where you quit |
| pocket-ui's own host: states | `roms/savestates/<system>/` | `<game>.pstate` to `.pstate7` (slots 1 to 8), `.pstate.auto` |
| Screenshots (HOME + A) | `roms/screenshots/<system>/` | `<game> <date> <time>.png` |
| Backups of pocket-ui's settings | `roms/backup/pocket-ui/` | one folder per backup |

pocket-ui's own host keeps its battery saves in the same place and format RetroArch uses, so a save made by one is
read by the other. This was tried on the device with a Nintendo 64 game saved in RetroArch; the other systems use the same code, so they
should work the same way ([Continuing where you left off](playing.md#continuing-where-you-left-off)).

### Which emulator now plays what

Most of the common systems start on **pocket-ui's own host** (pocket-core) with the **same core** ROCKNIX uses by
default, so the saves stay compatible. The full list is on [Systems](systems.md).

| System | ROCKNIX played it with | pocket-ui's own host plays it with | Your save |
|---|---|---|---|
| Game Boy, Game Boy Color | RetroArch · Gambatte | Gambatte | `.srm` works |
| Game Boy Advance | RetroArch · mGBA | mGBA | `.srm` works |
| NES, Super Nintendo | RetroArch · Nestopia, Snes9x | the same | `.srm` works |
| Mega Drive, Master System, Game Gear | RetroArch · Genesis Plus GX, Gearsystem | the same | `.srm` works |
| PC Engine | RetroArch · Beetle PCE Fast | the same | `.srm` works |
| Nintendo 64 | RetroArch · Mupen64Plus-Next | the same | `.srm` works |
| PlayStation | RetroArch · PCSX ReARMed (32-bit) | PCSX ReARMed (64-bit) | should work, see below |
| PSP | PPSSPP | PPSSPP | `roms/psp/PSP/SAVEDATA`, shared |
| Dreamcast | Flycast | Flycast | each game's memory card, shared |
| Nintendo DS | melonDS | melonDS DS | `.sav`, shared |
| Sega Saturn | YabaSanshiro | YabaSanshiro, if a Saturn BIOS is in `roms/bios` | **not shared**, see below |

- **PlayStation:** ROCKNIX's default core is the 32-bit PCSX ReARMed, pocket-ui's host runs the 64-bit one. The same
  core family should read the same save, but a card made by the 32-bit core was not tried.
- **Nintendo DS:** the host reads and writes `<game>.sav`, the file standalone melonDS uses. If a game has only a
  `.srm` (RetroArch's name), the host reads it the first time and keeps the save as `.sav` from then on.
  If it has only a DraStic `.dsv`, that is taken over into `.sav` the first time ([DraStic](#standalone-emulators)).
- **Sega Saturn:** saves made in standalone YabaSanshiro are not seen by the host, and the other way round.
- **A game that fails on the host** starts again with ROCKNIX's emulator by itself and says so
  ([Systems](systems.md#when-a-game-does-not-start-on-the-own-host)). Its save stays where it was.

### Runs on: RetroArch

Every other system, `.7z` games and anything you choose yourself plays on RetroArch. START on a game opens
[Runs on](options.md#runs-on), where you can pick RetroArch for one game or a whole console, or go back to what ROCKNIX
used. This is the same setting EmulationStation has, in the same file, so a choice you made there is used here.

- A game on RetroArch keeps using its own files: the save next to the game, the state `<game>.state` in
  `roms/savestates/<system>/`, and the moment you quit (RetroArch's `.state.auto`). It continues where you left off.
- RetroArch has **one** save in the pause menu. The eight slots, Options, run-ahead and the fast game switch belong to
  the own host.
- **Changing the emulator of a game** (RetroArch to the own host or back) keeps the in-game save and does **not** keep
  the state: the game starts from its in-game save ([Runs on](options.md#runs-on)). RetroArch's `.state` and
  `.state.auto` are different files from the host's `.pstate` and the host does not read them. They stay on the card,
  unused, until you go back to RetroArch (Storage's clean-up of old automatic states can remove a `.state.auto` that
  nobody has used for months, see [Storage](storage.md)).

### The newer save wins

When the own host starts a game it continues from the moment you quit. If the in-game save on the card differs from the
one inside that moment, the game starts from the in-game save instead and says **In-game save is newer: not resumed**.
That happens after you played the game in RetroArch, in standalone melonDS or on another device in between. The
contents decide, not the file dates. So the older moment does not write over your newer in-game save
([HOST.md](../HOST.md#pause-menu-features)).

### PlayStation 2, GameCube, Wii and 3DS

These stay on ROCKNIX's standalone emulators (ARMSX2, Dolphin, Azahar), with pocket-ui's save slots on top. Their
saves live in the emulators' own folders, so they carry over on the same card:

- **PlayStation 2:** `roms/ps2/Mcd001.ps2` and `Mcd002.ps2`.
- **GameCube:** `roms/bios/GC/MemoryCardA.USA.raw` and the like, or the `.gci` folders next to them.
- **Wii:** `/storage/.config/dolphin-emu/Wii/title`.
- **3DS:** `roms/3ds/azahar/sdmc/Nintendo 3DS`.

The slots of the pause menu are states in `roms/savestates/ps2`, `gamecube`, `wii` and `3ds`; the 3DS has one slot.

The full list of state files is in [Where the states are](pause-menu.md#where-the-states-are-for-players-coming-from-android).
PlayStation 2 on **AetherSX2** (ROCKNIX has it as another choice for PS2) and the other standalone emulators keep their
own saves and settings; save in the game before you quit.

### Favourites, ratings and names from EmulationStation

EmulationStation keeps favourites, ratings, descriptions and picture paths in each system's `gamelist.xml`. On the same
card the files are already there.

- pocket-ui reads them and **never changes them**. Cryptic file names show their real names, the game card shows the
  description, developer and genre, and a game without box art shows the picture the gamelist names.
- Settings → Games → **From EmulationStation** counts your favourites and ratings. **A** adds the favourites to
  Favourites, or uses the ratings as My rating (ES's 0.1 to 1.0 becomes one to five stars). It only adds: what is in
  Favourites stays, and a rating you gave here is kept.
- Play counts, "last played" dates and hidden flags in a gamelist are not read.

More: [Your EmulationStation gamelists](library.md#your-emulationstation-gamelists).

## Coming from Android

Nothing on Android is touched ([What it does not touch](index.md#what-it-does-not-touch)), and Linux cannot see its
storage. So you copy files over, from a backup on your computer.

### Getting the files onto the Pocket

| Way | Good for | Not for |
|---|---|---|
| **File sharing** (Settings → Games → Add games, X) | Games and saves that sit in `roms`: `.srm`, `.sav`, `.rtc` next to a game, PS2 memory cards in `roms/ps2` | `bios`, `savestates`, `ports` and `PortMaster` folders are hidden |
| **Remote access (SSH)** (Settings → Connections) | Everything, including the hidden folders: GameCube cards in `roms/bios/GC`, states | needs a computer that can log in over the network |
| **SD card in a computer** | Everything, but STORAGE uses Linux's ext4, which Windows and macOS cannot write | Windows and macOS without extra software |
| **Sync** (Syncthing) | Keeping saves in step with a computer or another handheld, after the first copy ([Sync](sync.md)) | a first copy out of Android's storage: copy to your computer first |
| **Web companion** | Games and BIOS files | saves: it only takes game, BIOS and PortMaster files |

Copy the games first and start each one once, so the folders exist. Use the folder names of ROCKNIX, not your
launcher's: `roms/gba`, `roms/snes` and so on ([Add games](getting-started.md#5-add-games)). Add games names a folder
that no system reads. Settings → Games → Add games shows each system's folder.

### RetroArch on Android

- **Saves usually work.** RetroArch names a save `<game>.srm`. Copy it into the game's folder on the Pocket, beside the
  game, with the same name as the game file. It works when the same core played the game, which is the case for the
  systems in the table above that say "the same". Find them where RetroArch's Directory settings point for saves;
  if RetroArch sorts them into a folder per core, take the file out of that folder.
- **States do not work.** RetroArch states (`.state`) from Android are not read by pocket-ui's own host. On a RetroArch
  on the Pocket they only load when the core is the same version, and a state from a newer core may not load on an older
  one ([Sync](sync.md#what-is-shared)). Load the game on Android, save in the game, and copy that save.
- **Playlists, history, overlays, shaders and core options** are not read. pocket-ui's own host has its own
  [Options](options.md). On RetroArch games, the settings in
  [Runs on](options.md#runs-on) and RetroArch's own menu apply.

### Standalone emulators

- **AetherSX2, NetherSX2, ARMSX2 (PlayStation 2):** bring the memory card files (`.ps2`). Copy one to `roms/ps2` as
  `Mcd001.ps2` (keep a copy of the card that is there first); ROCKNIX's ARMSX2 keeps its cards there. The format should be the same as PCSX2's, but a card from
  AetherSX2 was not tried. States (`.p2s`) depend on the emulator version and may not load.
- **Dolphin (GameCube, Wii):** bring the GameCube memory cards and the Wii save data. The cards belong in
  `roms/bios/GC`, Wii saves in Dolphin's `Wii/title` folder, laid out as on a computer. Not tried from Android. States
  do not carry over: Dolphin here names them after the game file.
- **Azahar, Citra, Lime3DS (3DS):** bring the save data of each game. It belongs in `roms/3ds/azahar/sdmc/Nintendo 3DS`.
  Not tried from Android; the folders inside have to match.
- **DraStic (Nintendo DS):** copy each game's `<game>.dsv` next to the game in `roms/nds`, with the game's name. The first
  time the game starts on pocket-ui's own host (melonDS DS) with no `<game>.sav` yet, the save is taken over into
  `<game>.sav` and the screen says "Took over the DraStic save". Your `.dsv` stays untouched. A `.sav` that is already
  there is never overwritten, and a game that already has a `.srm` keeps that. A `.dsv` that is not a DS save (an odd
  size) is left alone and the game starts without a save. Tried with built test files, not with a save from a real
  Android phone.
- **PPSSPP (PSP):** the `SAVEDATA` folders go to the same place as here: `roms/psp/PSP/SAVEDATA`.
- **Flycast (Dreamcast):** each game's memory card is kept here as `roms/bios/dc/<game id>_vmu_save_A1.bin`.

### Favourites and your collection

- **ES-DE** on Android: copy its `gamelists` and `downloaded_media` folders to `ES-DE/` on the Pocket (see
  [Coming from EmulationStation](getting-started.md#coming-from-emulationstation)), then use **From EmulationStation**
  for the favourites.
- **Daijishō and the Retroid launcher:** their lists are not read. Mark your favourites again; pocket-ui's
  [Curator](curator.md#curator) and the search help with a large collection.

## What does not come over

- **Save states** from any other emulator or core, and the "moment you quit" of an emulator you used before.
- **Settings:** controller layouts, shaders, overlays, core options, per-game settings, bezels. pocket-ui has its own
  [Options](options.md) and [Controls](controls.md) pages.
- **Play time, play counts, recent games and favourites** kept by a launcher or by RetroArch's history. Only favourites
  and ratings in a `gamelist.xml` can be taken over.
- **RetroAchievements:** sign in again on the Pocket ([Achievements](achievements.md)). Your unlocks stay with your
  account.
- **Wi-Fi networks** of Android.
- **The apps themselves.** pocket-ui plays games with the emulators ROCKNIX ships. An emulator that ROCKNIX does not
  have is not there.

## If a save does not show up

1. Check the save's file name against the game file. They match up to the dot: `Game.gba` and `Game.srm`.
2. Check the folder. The save goes where the game is, in its `roms/<system>` folder, not in `savestates`.
3. Look at [Runs on](options.md#runs-on) (START on the game). A save from RetroArch needs the same core.
4. If the game starts from the beginning and says nothing, the save is not found. Quit, copy again, and start it.
5. When you are unsure, keep the copy you made before you switched, and ask for help with the game and the emulator you
   came from ([Troubleshooting](troubleshooting.md)).
