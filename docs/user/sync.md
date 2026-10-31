# Sync saves with Syncthing

Settings → Connections → **Sync** keeps your saves and save states (and, if you switch it on, your [library data](#library-data)) the same on your Pocket, your computer, your phone or a second
handheld. It uses [Syncthing](https://syncthing.net), which ROCKNIX already contains. Nothing goes through a
company's server, and nothing leaves the Pocket until you pair another device.

> **Status:** built and checked on a Mini V2 (the switch, the folders, the web interface login, the scan after a
> game). Syncing with a second device has **not** been tried yet, because no outside device may be paired during
> development. The first pairing is yours; please tell us what happens. [Two versions of a save](#two-versions-of-a-save)
> has only been tried on a computer with made-up files, not with a real second device. [Library data](#library-data)
> too has only been tried in tests on a computer, with two made-up devices.

## What is shared

Each row of Settings → Connections → **Sync** is a folder:

| Row | Folder | What is in it |
|---|---|---|
| **Saves** | `/storage/roms` (only save files) | The `.srm`, `.rtc` (the cartridge clock of games such as Pokémon Crystal), `.sav`, `.eep`, `.fla`, `.sra`, `.mcr`, `.mcd` and `.dsv` files next to your games. The games themselves are **not** shared |
| **Save states** | `/storage/roms/savestates` | The states of every system, with their small pictures. Not the [Earlier](pause-menu.md#earlier) states: they stay on the Pocket that played them |
| **PS2 memory cards** | `/storage/roms/ps2` (only the cards) | `Mcd001.ps2` and `Mcd002.ps2`, the two memory cards every PlayStation 2 game saves on. Your PS2 games are **not** shared |
| **GameCube & Wii saves** | two folders: `/storage/roms/bios/GC` (only the cards) and Dolphin's Wii save data | The GameCube memory cards (`MemoryCardA.USA.raw` and so on, or the `.gci` files of each game) and the save data of each Wii game. Not the GameCube BIOS next to the cards, not the games |
| **3DS saves** | `/storage/roms/3ds/azahar/sdmc/Nintendo 3DS` (only save data) | The save data and extra data of each 3DS game. Not the games, nor any updates or downloadable content installed there |
| **Library data** | `/storage/.config/pocket-ui/library-sync` | Your favourites, play status, notes, ratings, stickers, hidden games, smart shelves and play history, so a second Pocket (or this one after a fresh SD card) knows them too. See [Library data](#library-data). **Off by default** |
| **BIOS files** | `/storage/roms/bios` | BIOS files. They are copyrighted: share them only between devices that are yours. **Off by default** |

Every row but **Library data** and **BIOS files** is on by default.

The three rows for PlayStation 2, GameCube & Wii and 3DS appear in Syncthing as folders of their own ("Pocket PS2
memory cards", "Pocket GameCube memory cards", "Pocket Wii saves", "Pocket 3DS saves"). On a computer you can put
each one straight into that emulator's own folder there (PCSX2's memory cards, Dolphin's `GC` and `Wii/title`
folders, Azahar's `sdmc/Nintendo 3DS`). A folder only appears once its emulator has run on the Pocket: Dolphin and
Azahar make their folders when they first start, and pocket-ui adds them after that game ends.

**Never while the game runs.** While any game is running, the **Saves** and **Save states** folders are paused in
Syncthing, and so is the folder of a PlayStation 2, GameCube, Wii or 3DS game's emulator: nothing is sent from them
and nothing arrives in them, so the running game can't overwrite a save that comes in in the meantime. When the game ends,
the folders carry on and the new saves go out at once (with **Sync after playing**, below, or within about ten
seconds otherwise). A folder you paused yourself in Syncthing's web interface stays paused.

**Earlier states stay here.** The states the pause menu's [Earlier](pause-menu.md#earlier) keeps by itself (up to 1 GB, a
new one every 5 minutes of play) are left out of **Save states** by its `.stignore` (`*.pstate.earlier*`,
`*.pstate.started*`); pocket-ui adds those lines to a folder that was shared before, after the next game. To share them
anyway, write your own `.stignore` in `/storage/roms/savestates` (one that doesn't start with pocket-ui's first line);
pocket-ui then leaves it alone.

Not covered: the saves of the experimental systems (Wii U, PS3, Xbox and others keep them in their own folders), and
RetroArch's own `saves` folder. Save states only work between devices with the same emulator versions; a state from a
newer core may not load on an older one.

## Turn it on

1. Connect to Wi-Fi (Settings → Connections → Wi-Fi).
2. Settings → Connections → **Sync**, select **Syncthing**, press **A**. It takes a few seconds. The setting survives a restart:
   Syncthing starts with the Pocket until you switch it off here.
3. The page now shows:
   - **Status**: *Not connected* until a device is paired, then *Up to date* or *Syncing 3 files*.
   - **Device ID** and a **QR code** of it.
   - The **web interface** address, with the user and password made for this Pocket.

## Pair a computer

You do this once per device.

1. Install Syncthing on the computer (syncthing.net) and open its page.
2. On the computer: **Add Remote Device**, type the Pocket's **Device ID** (or scan the QR code with the Syncthing
   phone app). Save.
3. On a computer on the same Wi-Fi, open the Pocket's **web interface** address from the Sync page, for example
   `https://<device-ip>:8384`. The browser warns about the certificate: it is Syncthing's own, accept it once.
   Log in with the user and password from the Sync page.
4. The page shows *"Device … wants to connect"*: **Add Device**.
5. Still in the Pocket's page, open the folder **Pocket saves** → **Edit** → **Sharing** and tick the computer. Do
   the same for **Pocket save states** and the other "Pocket …" folders you want on the computer.
6. On the computer, accept the folders that are offered, and choose where they go. To use the same saves in a
   second Pocket's emulators the folders belong where that Pocket expects them: `/storage/roms` and
   `/storage/roms/savestates`.

After that the Sync page says *Up to date* whenever the computer is reachable. The two devices need to be on at the
same time to exchange saves, unless one of them is always on (a home server, a Raspberry Pi), or you use Syncthing's
relays, which carry the data end-to-end encrypted.

## Sync after playing

**Sync after playing** (on by default, once Syncthing is on) asks Syncthing to look for new saves the moment a game
ends. A small line appears at the top of the shelf for a few seconds:

| Line | Meaning |
|---|---|
| **Saves synced** | The new saves were found and the connected device has them |
| **Syncing saves** | Found, and the other device is still copying them |
| **Sync waiting for a device** | Found, but no other device is connected right now. They go over when one comes back |
| **Syncthing is not running** | Sync is switched on here, but Syncthing is not answering |

It never makes the shelf wait: you are back on Home first, and the line appears when Syncthing has answered. With
this row off, Syncthing still finds new saves on its own (within about ten seconds of a change), only the line is gone.

## Two versions of a save

If you play the same game on two devices before they have synced, both have a newer save. Syncthing keeps both: the
save stays with one version, and the other is put next to it as a copy named `….sync-conflict-…`. pocket-ui looks for
these copies a little after it starts, after every game, when you open Settings → Connections → **Sync**, and every
few minutes while you are on the shelf. It never looks while a game runs, and shows nothing then.

When it finds one:

- The shelf says it once, for a few seconds, and where to choose: **A save has two versions: Pokémon Emerald – open the
  game's card to choose** for one game's save. For a memory card, a save no game of your shelf owns, or several at
  once it says **… – choose in Settings → Connections → Sync** (**A memory card has two versions: Mcd001.ps2 – …**,
  **… and 2 more – …**).
- The game's [card](library.md#what-the-card-shows) has a row under "Your game": **Two versions of this game's save.
  Choose one…** Press **A** on it.
- Settings → Connections → **Sync** has the row **Two versions** with how many wait ("2 to choose"). **A** lists them;
  A on one opens its versions. With nothing to choose the row says **Nothing waiting** and A does nothing.

The page shows each version with the time it was last changed and its size: **In use now** is the one the game loads
today, **Other version** the copy Syncthing kept. The one changed last is marked **Newer**. Select the one you want and
press **A** (**Keep this one**):

- The version you chose is the game's save from now on. Syncthing sends it to the other devices too.
- The other version is **not deleted**. It moves to a folder with today's date, `/storage/roms/sync-backups/<date>`
  (`roms/sync-backups/<date>` in file sharing), in the same subfolder as before, for example
  `sync-backups/2026-10-25/gba/Pokémon Emerald.srm`. The page names that folder before and after you choose. A file of
  the same name already there (you chose twice in one day) gets a number: `Pokémon Emerald (2).srm`. To go back, copy
  it over the save by hand.
- This folder stays on the Pocket: Syncthing does not share it.

B goes back without changing anything. A save state's small picture goes along with its state.

**Memory cards** are one file for many games: a PS2 card (`Mcd001.ps2`), a GameCube card (`MemoryCardA.USA.raw`) or a
PlayStation card (`.mcd`, `.mcr`) that several games use. They show on the card of every game of that console, and the
page says so: **This is the whole memory card. The version you keep holds the saves of every game on it.** You choose
the whole card, with all its games. If both versions hold progress you want (one game on each device), keep the one
with more and copy the other game's save across from the backup folder in the emulator's memory card manager on a
computer. Let the devices sync before you switch, and this does not happen.

Wii, 3DS and GameCube `.gci` saves have no game name pocket-ui can match. They appear only in the list under
Settings → Connections → Sync, with their file name.

With Syncthing off, or when nothing has two versions, there is no row on the card, no line on the shelf, and the
Sync page says **Nothing waiting**.

## Library data

Saves tell a game where you were. **Library data** carries what you said about your games: the stars of your
favourites, the play status, notes and ratings, your stickers, the games you hid, your smart shelves and your play
history. Switch the row on on every Pocket that should share them, and share the folder **Pocket library data** with
those devices in Syncthing, as in [Pair a computer](#pair-a-computer). On a second Pocket the folder belongs at the
same place, `/storage/.config/pocket-ui/library-sync`; a computer only passes it on and can keep it anywhere.

- **Both Pockets can change things.** Each one writes only its own files in the folder and reads the others'. When you
  star a game on one and write a note on the other, both have both. When you change the same thing on both (the same
  game's status), the change made last counts, by the Pockets' clocks.
- **Removing counts too.** A star you take off, a note you clear, a game you show again or a shelf you delete goes away
  on the other Pockets, unless someone changed it there later.
- **Games are matched by system and file name**, `gba/Pokemon Emerald (USA).gba`, not by where the file is: a game in
  another folder on the other Pocket still gets its notes. Notes for a game that isn't on this Pocket wait until it
  is. If two of your games in one system have the same file name (in two subfolders), those two are left out.
- **Play history** adds up: Curator → History and the Year in review count the time played on every Pocket, each session
  once. The game card's time played still counts this Pocket only. With Play history off (Settings → Games → Curator) no
  history is shared.
- **When it happens.** A little after the start, after every game, when you open this page, and whenever Syncthing
  brought something new (checked every minute). **Never while a game runs**: what comes in in the meantime waits until the
  game ends. You see the changes where they belong (the star, the status, the shelf); there is no notice.
- **A fresh SD card.** Put pocket-ui on, switch Syncthing and this row on, pair the other device again: everything comes
  back, play history included.
- **Something damaged from another device** (a file cut off on the way, a file of a newer pocket-ui) is left aside.
  Your own notes on this Pocket stay as they are, and nothing is shown on screen; the log says which file it was.
- **Switched off**, nothing is sent or read any more. What already came stays; History shows this Pocket's own play
  again. Changes made while it was off go over when you switch it back on.

Not shared: the recent games list, "Wrong game? Choose another", the preferred versions, Options and cheats of each
game, and the order of your stickers and shelves (new ones go at the end). How it works inside is in
[SYNC-CURATION.md](../SYNC-CURATION.md).

## The web interface is protected

ROCKNIX's own switch opens Syncthing's web interface to **everyone on the Wi-Fi** with **root's password**, which is
`rocknix` on a Pocket where nobody changed it. Anyone on the network could read and change every file on the Pocket.
pocket-ui does not use that setup. With the switch on here:

- the login is the user `pocket` with a password made for this Pocket (like the file sharing and remote access ones),
- the interface uses https,
- usage reports, crash reports and automatic upgrades are off.

If Syncthing was switched on some other way and the page shows a red line about "ROCKNIX's login", switch it off and
on again here. See [SECURITY-HARDENING.md](../SECURITY-HARDENING.md#syncthing-settings--sync-implemented-checked-on-the-device-22102026) for the details.

## Good to know

- **Two devices, one game:** if you play the same game on two devices without syncing in between, Syncthing keeps both
  saves and names one `….sync-conflict-…`. Nothing is lost: the game's card and the Sync page let you
  [choose one](#two-versions-of-a-save), and the other goes to a backup folder.
- **While a game runs** saves and states are not synced (see [Never while the game runs](#what-is-shared)). A newer
  save sent from the computer in the meantime is not overwritten by the running game; it arrives after the game, and if
  both sides changed it, you get the [two versions](#two-versions-of-a-save) described above.
- **Memory cards are one file for many games.** A PS2 memory card (and a GameCube `.raw` card) holds the saves of
  every game on it, and Syncthing copies the whole file. If you save on the card on two devices before they have
  synced, the card written last wins, and the other one is kept as a `Mcd001.sync-conflict-….ps2` next to it. The
  saves made on that other device are only in that copy: choose the card to keep as described in
  [Two versions of a save](#two-versions-of-a-save). Let the devices sync before you switch, and this does not happen.
  GameCube `.gci` files, Wii and 3DS saves are one folder per game, so only the same game played on both conflicts.
- **On the other device**, Syncthing doesn't know the Pocket's choice of files: point the folders at the matching
  emulator folders, or at empty folders, not at a whole games folder.
- **Switching Syncthing off** stops it and keeps ROCKNIX from starting it at boot. Your pairings and the device ID
  stay; switching it on again picks up where it was.
- **Folders you switch off** disappear from Syncthing; the files stay where they are, on the other devices too.
- **Clocks.** For Library data the change made last counts. A Pocket whose clock is wrong (after a flat battery, before
  Wi-Fi set it) can lose a change or win with an old one: let it get on Wi-Fi first.
