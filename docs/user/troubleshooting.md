# Troubleshooting and FAQ

## Known limits

**The SD card image is not published.** It builds and passes its offline checks and was started once on a fresh
card on a Mini V2 (2026-10-27, which found a Wi-Fi bug in the first-start setup, since fixed); a complete first start
as a new player has not been repeated. The test device was set up by hand (see [Getting started](getting-started.md#1-write-the-sd-card) and
the [install guide](../INSTALL-DUAL-BOOT.md)). Tested on one device only.

**Which system starts is a setting of Retroid's start menu.** With **Boot mode: Loader** the Pocket starts pocket-ui
from the SD card, with **Boot mode: Android** it starts Android (see
[Getting started](getting-started.md#2-start-pocket-ui)). If pocket-ui is not installed as the start screen,
ROCKNIX's own menu (EmulationStation) appears instead.

**Sleep wakes fast, but its battery use is not known yet.** Waking the Pocket from deep sleep has been timed on a
Mini V2: the screen and the game are back well under a second. How much battery sleep uses over a night has not been
measured, and Power off and Restart in the power menu have not been tried on a real device. Save your game before you
put the device away (see [Settings](quick-settings.md#sleep-and-power)).

**The touch screen is not confirmed on the real panel.** An early version put taps in the wrong place in a Nintendo DS
game; the mapping has been rotated since, and nobody has checked it again. If a tap lands in the wrong place (mirrored or
rotated), please report it; developers can rotate the mapping with `POCKETUI_TOUCH_ROTATE`
([DEVELOPING.md](../DEVELOPING.md)).

**Playing on a TV has not been tried yet.** Nothing has been plugged into a Mini V2 so far; sound follows the TV
even with headphones in, and sleep leaves the TV on (see [Playing on a TV](playing.md#playing-on-a-tv)).

**System updates are untested on a device**, and no public update source exists yet (see
[Updates](updates.md)).

**Options, shortcuts and run-ahead exist only on pocket-ui's own host**, and save slots there and on PlayStation 2,
GameCube, Wii and 3DS. The systems are listed in [Systems](systems.md#on-pocket-uis-own-host).

**PlayStation 2, GameCube, Wii and 3DS show the game booting** for a moment at the next start,
before they jump to the moment you quit.

**GameCube and Wii: leave Dolphin's save slot alone.** Dolphin has its own shortcuts for changing
its save slot. If you change the slot with them, the pause menu's Save, Load and continuing use a
different slot than they should.

**3DS homebrew share one save.** Azahar names its quick save after the game's title ID, and `.3dsx`
homebrew has none, so all `.3dsx` games share the one quick save (Save and Load in the pause menu).
Continuing where you left off is kept per game file.

**A PlayStation game on several discs has no Disc row.** The pause menu changes discs only for a game started
from an `.m3u` playlist or a multi-disc `.pbp`; each disc started on its own is a game of its own. See
[Games on several discs](systems.md#games-on-several-discs).

**Sync has not been tried with a second device.** The switch, the shared folders, the protected web interface and
the scan after a game were checked on a Mini V2, but no outside device may be paired during development. The rows
for PlayStation 2, GameCube & Wii and 3DS saves are new and not yet checked on a Mini V2 (see
[Sync](sync.md#what-is-shared)).

**Bluetooth pairing has not been tried with a real controller yet.** The page, the search and the pairing steps were
checked with stand-ins for ROCKNIX's Bluetooth tools; which controllers pair and reconnect on a Mini V2 is still to
be confirmed (see [Bluetooth](bluetooth.md)).

**PC streaming has not been tried with a real PC** for the same reason. Starting it, the lookup and the way back
are checked; pairing on the Pocket was checked with a stand-in for Moonlight (see
[PC streaming](pc-streaming.md#pair-once)).

**Smooth fan has had a first test on a Mini V2.** In eight minutes with a game that keeps one core busy, the fan
started once where the system's own control started it twice, and was off far less of the time (see
[Smooth fan](performance.md#smooth-fan)). How quiet it sounds, its lowest speed and the 78 °C limit on the real
hardware are not checked yet. If the fan behaves oddly, switch Settings → Playing → Performance & fan →
**Smooth fan** off: the system's own fan control is back at once.

**Settings → Help & tools → Tools, the Wi-Fi country and Backup & reset are new and not yet confirmed on a Mini V2.** They work in
the virtual device (which has no Wi-Fi chip and none of the big standalone emulators).

## Questions

**The start picture (the "pocket-ui" bar) stays and the shelf doesn't come.**
After a minute it says "Still starting…". The first start after setting up a card, or after a system update, can take
a few minutes: give it time. If nothing changes for several minutes, hold Power until the Pocket switches off, and
switch it on again. If it happens again, start Android instead (Volume Down + Power, **Switch boot mode**) and ask for
help with the card in hand; [Report a problem](#report-a-problem) works from the next start that reaches the shelf.

**The starter pack says the Pocket isn't online, or the download didn't finish.**
The games come over the internet, so Wi-Fi has to be on (Settings → Connections → Wi-Fi). Nothing is added by a download that
was cut off; press A again and it starts that game anew. "Not enough space on the SD card" means the system's folder is on a full
SD card: free some space first. A game that is already in its folder is never replaced.

**The SD card is full.**
A game on pocket-ui's own host then says "Your progress couldn't be saved to the SD card" at the bottom of the screen, and
Quit asks first ([When your progress can't be saved](pause-menu.md#when-your-progress-cant-be-saved)). Settings
→ System → About shows the free space. Make space from a computer through the file share or
[Remote access (SSH)](settings-system.md#remote-access-ssh), while the game waits in the pause menu or after quitting: save
states you don't need any more (`roms/savestates/<system>/`, about 40 MB each for PSP and other 3D games), screenshots,
games you don't play. Delete no `.srm`, `.rtc` or `.sav` file: those are your in-game saves. Once there is space, the next
try (within 10 seconds) writes the save and the line goes away. An SD card that "can't be written to right now" has turned
read-only, usually after an SD card error: quitting and restarting the Pocket often clears that; if it comes back, copy
your saves off and replace the SD card.

**A tool from Settings → Help & tools → Tools doesn't let me out.**
Hold HOME for a second and let go: pocket-ui stops the tool and Settings → Help & tools → Tools comes back. Tools that support it
also close with L1 held and SELECT + START.

**A tool I installed from the Store was gone after a restart.**
Older versions put Store tools into ROCKNIX's tools folder, which ROCKNIX empties at every start. They now go to
pocket-ui's own folder; a tool that was lost that way shows **Install** again in the Store.

**I changed a setting and want the old ones back.**
Settings → System → Backup & reset lists the backups, including the one made before every reset and restore. Restore one (A,
then A again).

**My device does not show up in the Bluetooth search.**
It has to be in pairing mode while the search runs: most controllers flash their light fast then. A controller that
was paired with a phone or a console before often tries to reconnect there first; switch Bluetooth off on that device,
or start pairing mode again. Keep it close to the Pocket.

**My device was paired, but it does not reconnect.**
Switch it on (controllers: press the home button); give it a few seconds after the Pocket wakes up, since Bluetooth starts again
after sleep. If it still does not connect, select it in Settings → Connections → Bluetooth and press A. If that fails too, forget it
(X, X) and pair it again.

**My games don't show up.**
Check that each file is in the right system folder under `roms` (directly, or in one subfolder; nothing deeper is
read) and has a file type that system accepts. A folder that isn't one of the systems' names (a "Sega Genesis" next to
`megadrive`, or "GBA" instead of `gba`) is not read either: Settings → Games → Add games names such a folder under its
title, with the folder its games go in when it can tell. A disc game whose `.cue` (or `.gdi`) names a file that isn't
there is left off the shelf, because it could not start; Add games names it and the missing file. The list is read
when pocket-ui starts and when you leave a step opened from Settings. A system appears on Home only once its folder
holds at least one game.

**A symbol in a game's name is missing.**
The Pocket's font has no ticks, stars or emoji, so a game whose file name has one shows without it. Japanese,
Chinese and Korean names are shown with the system's Japanese font. Letters that neither has show as "?". The file
itself is not changed.

**Load is greyed out in the pause menu.**
There is no save to load yet. On pocket-ui's own host this means the selected slot is empty: press
Left/Right to choose another slot, or choose Save first.

**Quit in the pause menu takes a moment.**
pocket-ui waits for the game to save and close. Emulators that ignore the request are stopped after
about five seconds. PlayStation 2, GameCube, Wii and 3DS need a few seconds to save and shut down;
pocket-ui waits up to 35 seconds for them.

**A game didn't continue where I left off.**
Continuing works for pocket-ui's own host and for RetroArch. PlayStation 2, GameCube, Wii and 3DS
continue only when you quit through the pause menu's Quit. Other standalone emulators (for example
Wii U) start fresh; use the game's own save. A game on the own host continues from the moment you last
quit it, not from a crash. With Settings → Playing → **Remember where I quit** off, Quit leaves the last moment as it was, on
purpose (see [Remember where I quit](settings.md#remember-where-i-quit)).

**A game now starts with RetroArch instead of pocket-ui's own host.**
It did not start on the own host once, so pocket-ui switched it to RetroArch (see
[Systems](systems.md#when-a-game-does-not-start-on-the-own-host)). To try the own host again, press START on the
game's card, choose pocket-core under [Runs on](options.md#runs-on) and save it (for this game or this console). The
list of these games is `fallback.json` in pocket-ui's data folder.

**A game starts with a different emulator than before, or doesn't start at all.**
Someone chose another emulator for it or its console, in pocket-ui or earlier in ROCKNIX's EmulationStation. Press
START on its card: [Runs on](options.md#runs-on) shows what plays it and where that comes from. "Not installed"
means ROCKNIX can't start that choice on this Pocket; pick another one, or Save changes → Restore defaults.

**An arcade game does not start, or starts wrong.**
Arcade ZIPs have to match the core that plays them, and a clone needs its parent ZIP and often a BIOS ZIP such as
`neogeo.zip`. Check the set version first, then the missing files: [Arcade games](arcade.md#when-a-game-does-not-start).

**A translation or hack plays as the original game.**
The patch file (`.ips`, `.bps`, `.ups`) needs exactly the game file's name, only the ending differs: `Game Name.sfc`
and `Game Name.ips`, in the same folder. For a zip it is the zip's name. Capital letters count. On a standalone
emulator the patch is not applied at all: look at [Runs on](options.md#runs-on).
See [Patches, translations and MSU-1](patches-msu1.md).

**A game with a patch says "The patch ... is for another version of the game".**
The patch was made for another version of the game (another country or revision). Find the version the patch names,
or move the patch to another folder to play the original:
[If the patch does not fit](patches-msu1.md#if-the-patch-does-not-fit).

**An MSU-1 game has no extra music.**
The `.msu` and `-1.pcm` files must have the game's name and sit next to the unzipped game in `roms/snesmsu1`. The
recipe has not been checked on the device: [MSU-1](patches-msu1.md#msu-1-extra-music-and-video-for-some-snes-games).

**A game shows "... didn't start. The file may be damaged, or the system needs a BIOS file".**
The game's emulator gave up within five seconds, with an error, and nothing else said why (pocket-core's own reasons,
such as "needs a BIOS file in roms/bios", are shown as they are). Check that the file is whole (copy it again) and, for a
system that needs one, that its BIOS is in `roms/bios`: Settings → Games → **Add games** lists the BIOS files per
system. When pocket-ui knows the system's BIOS and it is missing (Sega CD, PC Engine CD and others), the line names the
files instead: "... didn't start. Sega CD needs a BIOS file in roms/bios: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin."
A game that never started is not counted as played: it does not become Continue and is not in the switcher.
For a zip file, pocket-core says what is wrong with it: [see below](#a-zip-game-says-it-holds-several-games-is-damaged-or-cant-be-opened).

**The fan runs all the time, slowly.**
That is Smooth fan: once the Pocket is warm, a slow, steady fan is quieter than one that switches on and off. It
stops when the Pocket has been clearly cooler for most of a minute (below 46 °C with Quiet). For a fan that turns less
often, choose **Quiet** in Settings → Playing → Performance & fan; for the system's on-and-off steps, switch **Smooth fan** off.

**The fan suddenly runs at full speed.**
Some sensor reached 78 °C: the fan runs at full speed until everything is below 72 °C, whatever you chose. The page's
**Now** part then says "full speed, hot". A demanding game in a warm room can do that; Performance mode makes it more
likely.

**The fan doesn't follow my choice.**
Settings → Playing → Performance & fan → **Now** says who drives the fan. "smooth, …" is pocket-ui; "starts at … °C" is the
system's own control, which takes over while the Pocket sleeps, while a game has its own cooling profile in
EmulationStation set to "custom", and for ten minutes when the system kept changing the fan back (pocket-ui then gives
way rather than fight over it). `pocket-ui.log` says which, in lines starting with `fan curve:`.

**The menus make no sound.**
Settings → Sound & feel → Sounds must be on, and the console's volume must be above 0 and not muted. The clicks never play while
a game runs, and not while the device sleeps (see [Playing](playing.md#menu-sounds)).

**Wi-Fi says "Wrong password. Try again."**
The password is shown on screen while you type; check upper and lower case. X switches to capitals
for one letter, Y to symbols. The keyboard opens again by itself; a refused password is not kept, so just type it again.

**Wi-Fi says "… isn't letting this Pocket in."**
The Pocket has been trying a saved network for a minute without getting in, most often because its password changed.
Press A on the network and type the current password. Until a network has given the Pocket an address, the Wi-Fi page
says **Connecting…**, not Connected, just like the status corner, which shows no Wi-Fi symbol yet.

**Wi-Fi is connected, but "this network wants a sign-in".**
Hotel, train, airport and café networks often show a login or "I agree" page before they let anything through. The
Wi-Fi page shows it as its first row, checks again by itself, and lists the other ways online, with the Pocket's
Wi-Fi address for the front desk: see [Hotel and train Wi-Fi](hotel-wifi.md). How pocket-ui notices: after you join,
when the Wi-Fi page opens while connected, and when an update, the store or the PortMaster catalogue fails, pocket-ui
sends one plain web request to `nmcheck.gnome.org`, the page NetworkManager (the system's network service) itself
uses for this question; it answers "NetworkManager is online". A redirect or a page of its own instead means a
sign-in. While the Wi-Fi page shows a sign-in, the same request is sent every 15 seconds until the network lets the
Pocket in or you leave the page; otherwise the answer is kept for 20 seconds. It is never sent from a game, and it
carries no account, no name and nothing about the Pocket; no Google address is contacted. Not every message knows
about this yet: RetroAchievements and the starter pack still say that the Pocket is offline.

**After a start, Wi-Fi finds no networks.**
The Pocket's system blocks the Wi-Fi radio for a few seconds while it starts. If Wi-Fi was on when you last used it,
pocket-ui switches the radio on again by itself, once, and searches. If you had switched Wi-Fi off, or the radio will not
come on, Settings → Connections → Wi-Fi says **Wi-Fi is off. Press A to turn it on.**, and the first row turns it on;
when the radio is released by itself, the page notices within a few seconds and searches again. **Couldn't search for networks. Try again (X).** means the
search itself failed; **No networks found** is only said after a search that worked. Quick settings' Wi-Fi row shows
the same state. If switching Wi-Fi on does not work, a short line says so and the row stays Off; the setting that Wi-Fi
should be on is only stored after it really came on.

**Wi-Fi dropped and stayed off.**
pocket-ui brings a dropped connection back by itself: after 30 seconds offline it connects to a saved
network in range (your main network first, otherwise the strongest other saved one, such as a phone
hotspot when you are out), then tries again after 1, 2 and every 5 minutes, as long as Wi-Fi is switched
on. It never joins a network you haven't connected to before, and it waits while a connection is already
being made. A saved network whose
password had gone missing from the system's network settings is repaired at the next start, so you aren't
asked for it again. It works the other way round too: when EmulationStation emptied the system's copy of the
password, pocket-ui puts it back from the saved network at its next start.

**Cover art found nothing for some games, or the wrong picture.**
Cartridge games are recognised by their checksum, other games by file name ([Extras](extras.md#cover-art)). For one
game, open its card, press Y and choose **Wrong pictures? Look up again**
([the game card](library.md#wrong-pictures-look-up-again)): it deletes that game's pictures and looks it up afresh.
Games without a match are otherwise skipped on later runs. A disc game or a homebrew game the list doesn't know is
found by its name: renaming the file to the usual name of the release (for example "Tetris (World) (Rev 1)"),
then Look up again, usually helps.

**An update failed and the previous version came back.**
That is the automatic way back working as intended: a new version that does not start, or keeps stopping, is replaced
by the last version that worked. Your games and settings are not affected.

**pocket-ui restarted by itself.**
If pocket-ui crashes, it starts again, up to three times in a row. If it keeps stopping, the last version that worked
comes back, and after that pocket-ui opens in **safe mode**, a page to restart, go back a version, reset its settings
or switch off ([Safe mode](safe-mode.md)). ROCKNIX's own menu (EmulationStation) only opens by itself if pocket-ui
cannot start at all; the next start of pocket-ui then says why. A card on the next screen says that something went
wrong. The details are in `crash.log` in pocket-ui's data folder and in the logs (see
[What to send when something crashes](#what-to-send-when-something-crashes)). Games and settings are written so that a
crash or a power cut leaves the old or the new content, not a half-written file.

**The Pocket shows "Safe mode".**
pocket-ui stopped several times in a row, and the last version that worked did too. The page says what happened.
Choose Restart normally first; if safe mode comes back, try Use previous version or Reset pocket-ui settings (nothing is
deleted, see [Safe mode](safe-mode.md#safe-mode)), and send the logs.

**The screen says "The device shut down unexpectedly last time."**
The last time the Pocket ran, it did not shut down the normal way: it froze and restarted by itself, the battery ran
out, the power was cut, or the power button was held until it switched off. pocket-ui notices because the logs of that
run have no mark that it ended properly. `crash.log` then has a line with the time of the last sign of life and the
last messages of the system; the logs have the rest (see below). Nothing in your games or settings is lost by this
message.

## A zip game says it holds several games, is damaged or can't be opened

pocket-core opens a `.zip` itself for most 8-bit and 16-bit systems and checks it first. It then says why it did not
start the game, on the shelf, and does not hand the game to RetroArch. A readme, a picture or a checksum file next to
the game does not matter.

- **"... holds 2 game files."** The zip has more than one game for this system, for example two regions of the same
  game. pocket-core does not pick one by the order in the file. Unpack the zip on a computer, keep the game you want and
  copy that file (or a zip with only that file) to the SD card.
- **"... is damaged: the game inside doesn't match its checksum."** The game was changed or cut short on the way, often
  by a bad copy or a full SD card. Copy the file to the SD card again.
- **"... is damaged or not complete."** The zip's end is missing: a copy or a download stopped before it was done.
  Copy the file to the SD card again (download it again first if it came from the internet).
- **"... is password-protected, and pocket-core can't open it."** Unpack it on a computer and copy the game file itself.
- **"... is packed in a way pocket-core can't open."** The zip uses a newer compression method than the plain one, such as
  bzip2, LZMA or Zstandard. Pack it again as a normal zip file, or copy the game file itself.
- **"There is no game file in ..." / "... is empty."** The zip holds only texts or pictures, or nothing. Check that it
  is the right file.

A zip that pocket-core can't read otherwise (a rare kind of zip) is still tried with RetroArch, as before. `.7z` games
always go to RetroArch.

## Report a problem

When something goes wrong, Settings → Help & tools → **Report a problem** (also in About, and in the question of
SELECT + START held on the shelf) puts what we need to find it into one
file. Press A once. When a game doesn't start, the message on the shelf says how to get there. A few seconds later the Pocket shows the file's name, for example `report-2026-10-26-164007.zip`
(the date and time on the Pocket's clock, as for screenshots).

**Where it is.** In the `reports` folder of the games share, the same place as `screenshots`: `roms/reports` on the SD
card. Copy it to your computer over [file sharing](getting-started.md#over-wi-fi-file-sharing), or with the SD card in a computer.

**What to do with it.** Open a bug report on pocket-ui's GitHub page (Issues → New issue → Bug report), say what
happened, and attach the zip. Nothing is sent by the Pocket itself.

**What is inside.** All plain text, so you can look through it first:

| File | What is in it |
|---|---|
| `README.txt` | What the other files are and what was left out |
| `system.txt` | pocket-ui and pocket-core version, ROCKNIX version, device, uptime, memory, battery and temperatures, free space on each part of the SD card, the last crash |
| `emulators.txt` | The cores and emulators on this Pocket, with their versions where known |
| `settings/` | pocket-ui's settings files (look, sleep, sounds, lights, performance and similar) |
| `logs/` | The end of the logs below, and the same logs from the start before; if a hotel or café sign-in page couldn't be done on the Pocket, that page (`portal-last.html`, what the network served) |

**What is left out.** Before anything is written, these are replaced by markers such as `[ip]` or `[removed]`:
Wi-Fi network names and passwords, IP and MAC addresses, Tailscale names and keys, your RetroAchievements user name
and token, Syncthing device IDs, the passwords of file sharing, remote access and the web companion, user names in
folder paths, e-mail addresses, and Bluetooth devices named after a person ("Alex's AirPods"). Your game list, play history, favorites, notes and saves are not in it.
The names ROCKNIX gives things itself stay, because they help find the problem: the host name it starts with (the
chip's name, `SM8250`), a Wi-Fi profile called `wifi`, and version numbers such as `0.8.1.1`. A host name you chose
yourself is replaced.

Game names are replaced by the system and a short code, for example `[game snes 1a2b3c4d]`. The same game always gets
the same code, so the log can still be followed. If a game matters for the problem, name it in your report.

If the SD card is full or can't be written to, the Pocket says so and nothing is saved. Make some room (old
screenshots, for example) and try again.

## What to send when something crashes

pocket-ui keeps its logs on the SD card, so they are still there after a freeze or a restart. (The system's own log
lives in memory and is gone after a restart.) They are in `/storage/.config/pocket-ui/logs/`, and Settings → Help & tools → About
shows the folder.

| File | What is in it |
|---|---|
| `pocket-ui.log` | What pocket-ui and the game programs wrote, each line with the seconds since the system started |
| `kernel.log` | The system's messages (what `dmesg` shows), from the start. Warnings and errors are written to the SD card at once, the rest every few seconds |
| `sensors.log` | A line every 5 seconds: the hottest temperature, GPU clock, CPU mode and clock for each cluster, fan speed and battery |
| `net.log` | What the network programs (NetworkManager, iwd, Tailscale) said since the start, and the names of the saved Wi-Fi networks at the start and whenever they change (names only, never passwords; passwords that show up in a line are blanked). It answers why a saved network was missing after a restart |
| `shutdown.log` | Each time the Pocket was switched off or restarted from its own menus: which one, the last lines of the log and of the system, the clock alarm (it is switched off first so it cannot restart the Pocket), the command and how it ended. Written line by line, so it is there after a restart or a power cut. At most 128 KB for a start |
| `portal-last.html` | The last Wi-Fi sign-in page the Pocket couldn't do ("This page needs a full browser"), as the network served it, at most 200 KB. Report a problem takes it along so the page can be looked at on a computer |
| `../crash.log` | One line for each crash of pocket-ui or a game, and for each start after an unexpected shutdown |

The last three starts of the system are kept: the folder holds the current one, and `boot-…` folders (named by the
date and time of the start) the two before it. Each file is at most 5 MB (`.old` is the half before the current half); `shutdown.log` at most 128 KB.
If the Pocket froze, the run that froze is the newest `boot-…` folder, and the line in `crash.log` names the time.
If "Power off" restarted the Pocket instead, the newest `boot-…` folder is the start before: its `shutdown.log` ends with the command that was run and how it ended, and Report a problem has it as `logs/previous-boot/shutdown.log` (the start before that as `logs/boot-before-that/`, with that start's other logs too). If your Pocket still switches itself off after 3 hours asleep, you (or an earlier version) have **Switch off after sleeping** set to 3 h: Settings → System → Sleep & power, set the row to Off. New settings start with Off. Each start also writes one line there with the state of the clock alarm it found; an alarm left over from before is cleared.

**What to send:** the newest `boot-…` folder (or the whole `logs` folder), `crash.log`, the game and the system it was
(for example "PlayStation 2, Quit mid-race"), and what you were doing. `sensors.log` shows whether the device was hot and
what the CPU and GPU were doing just before; `kernel.log` ends with the last thing the system said.

**How to get the files off the Pocket.** The file sharing over the network only shares the games (`roms`), so the logs
are not in it. Use remote access: Settings → Connections → [Remote access (SSH)](settings-system.md#remote-access-ssh), then on your computer

```
scp -r root@<address>:/storage/.config/pocket-ui/logs ./pocket-logs
scp root@<address>:/storage/.config/pocket-ui/crash.log ./pocket-logs/
```

with the password the screen shows. Or switch the Pocket off and put the SD card in a computer that reads ext4 (Linux; the
storage part of the SD card is ext4, see [Getting started](getting-started.md)) and copy the same folder.

The logs contain the names of your games and may contain network details such as addresses. Look through them
before you post them in public, or use [Report a problem](#report-a-problem), which leaves those out.

**I lost the file sharing or remote access password.**
Both are shown on screen: Settings → Games → Add games, and Settings → Connections → Remote access (SSH). Each Pocket has its own
password; it does not change.

**How do I get to ROCKNIX's standard menu (EmulationStation)?**
On the shelf, hold SELECT + START for one second and choose **Open EmulationStation**, or Settings → Help & tools → About → **Open
EmulationStation…**. Read the warning first: do not use Updates or Factory reset there. Restart the Pocket to come back
([Safe mode and EmulationStation](safe-mode.md#opening-emulationstation)).

**Where does pocket-ui keep its data?**
In `/storage/.config/pocket-ui/` on the SD card:

| File or folder | Contents |
|---|---|
| `settings.json` | Your look, appearance, motion, On start, the systems on the shelf, and whether the first start is done |
| `recents.json` | Recent games and play times |
| `game-settings/` | The pause menu's Options (picture, emulator, controls, shortcuts, extras) per console and per game |
| `slots/` | The save slot each game used last |
| `fallback.json` | Games that start with RetroArch because they did not start on the own host |
| `covers/`, `snaps/`, `titles/` | Cover art, and the screenshots and title screens beside it ([Pictures](extras.md#pictures)) |
| `favorites.json` | Starred games (if you use Favourites) |
| `library.json` | Play status, notes, sessions and stickers ([Your library](library.md#where-it-is-kept)) |
| `curator.json` | Curator's switches and your smart shelves |
| `wikipedia/` | Game descriptions loaded from Wikipedia |
| `retroachievements.json`, `achievements/` | The RetroAchievements sign-in key and what the Pocket keeps for offline play ([Achievements](achievements.md#good-to-know)) |
| `battery-history.bin` | The last 48 hours of battery readings ([Battery](settings-system.md#battery)) |
| `battery-games.json` | What each game used of the battery while you played it ([Battery in this game](pause-menu.md#battery-in-this-game)) |
| `port-cpu.json` | Which PortMaster ports keep a processor core busy ([PortMaster ports](performance.md#portmaster-ports)) |
| `play-log.bin` | Your play history: when each game started and ended ([Play history](curator.md#play-history)). Not part of a backup |
| `search.json` | Your last five searches (Search on Home) |
| `crash.log` | A short report of every crash of pocket-ui or pocket-core, and of every start after an unexpected shutdown |
| `logs/` | pocket-ui's log, the system's messages and a sensor sample every 5 s, for the last three starts of the system ([What to send](#what-to-send-when-something-crashes)) |
| `share/`, `remote/` | The passwords for file sharing and remote access |
| `tailscale.json` | The name you gave the Pocket on your Tailscale network (the sign-in is ROCKNIX's, in `/storage/.cache/tailscale`) |
| `web.json`, `web/` | The web companion's switches, and the browsers paired with it (only a fingerprint of each, readable by the system alone) ([Web companion](web-companion.md)) |
| `performance.json` | Settings → Playing → Performance & fan: what games start with, the fan outside games, and Smooth fan |
| `store/`, `fonts/`, `frames/` | What the Store installed |

Saves are kept with the games: in-game (battery) saves next to the game file in
`roms/<system>/`, save states in `roms/savestates/<system>/`. On pocket-ui's own host the eight
slots are `<game>.pstate` and `<game>.pstate1` to `<game>.pstate7`, the moment you quit is
`<game>.pstate.auto`, and each has a `.png` preview next to it.

**How do I run the first-start setup again?**
Each step is in Settings: Look (Look), Wi-Fi (Connections), Pictures & extras and Add games (both under Games).
<!-- TODO: if a full reset of the first start is wanted, document it (today: set "onboarded" to
     false in settings.json). -->

**Does pocket-ui change anything on Android?**
No. See [What it does not touch](index.md#what-it-does-not-touch).
