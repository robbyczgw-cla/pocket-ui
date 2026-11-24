# Troubleshooting and FAQ

## Known limits

**Ready-made SD card images** for the Retroid Pocket Mini V2 and Anbernic H700 models live on the
[install page](https://pocketui.xyz/install.html), alongside the Ports installer, two-file card install and SSH package
([Getting started](getting-started.md#1-put-pocket-ui-on-the-card)). Tested on several handhelds, most heavily on the
Retroid Pocket Mini V2.

**Boot choice lives in Retroid's boot menu.** **Boot mode: Loader** starts pocket-ui OS from the SD card, and **Boot
mode: Android** starts Android ([Getting started](getting-started.md#2-start-pocket-ui)). If pocket-ui isn't installed
as the active UI, ROCKNIX's own menu opens instead.

**Sleep wakes fast, though overnight drain isn't measured yet.** Waking a Mini V2 from deep sleep brings your game back
in well under a second. Overnight battery drain and the Power off / Restart menu items still need more real-hardware
testing, so save before putting the handheld away ([Settings](quick-settings.md#sleep-and-power)).

**Touch alignment isn't confirmed on every panel.** An early build misrotated Nintendo DS touch input; the mapping has
been rotated since, though it still needs fresh confirmation on real hardware. Report any mirrored or rotated taps;
developers can override rotation with `POCKETUI_TOUCH_ROTATE` ([DEVELOPING.md](../DEVELOPING.md)).

**TV output (USB-C to HDMI) hasn't been tested on hardware yet.** Audio currently routes to the TV even with headphones
plugged in, and sleep leaves the TV signal active ([Playing on a TV](playing.md#playing-on-a-tv)).

**Options, shortcuts and run-ahead apply to pocket-ui's own player**, while visual save slots work there plus PS2,
GameCube, Wii and 3DS ([Systems](systems.md#on-pocket-uis-own-host)).

**PS2, GameCube, Wii and 3DS briefly boot the game** before jumping straight to your saved Continue spot.

**GameCube and Wii: don't change Dolphin's save slot manually.** Using Dolphin's internal slot hotkeys shifts which slot
the pause menu saves and loads from.

**3DS `.3dsx` homebrew shares one quick save.** Azahar keys its quick slot by title ID, which `.3dsx` files lack, so
homebrew titles share that single Save/Load slot (Continue moments still stay per game file).

**Multi-disc PlayStation games need an `.m3u` playlist or multi-disc `.pbp`** to show the pause menu's **Disc** row.
Individual disc files launch as separate games ([Games on several discs](systems.md#games-on-several-discs)).

**Syncthing pairing hasn't been tested with an external device during dev** (the toggle, folders, protected web UI and
post-game scans were verified on a Mini V2). PS2, GameCube, Wii and 3DS sync rows are new and awaiting hardware
confirmation ([Sync](sync.md#what-is-shared)).

**Bluetooth controllers have been tested on an Anbernic H700 so far.** Wake-from-sleep reconnects, multi-controller play
and specific third-party pads are still being validated; use [Test buttons](settings-sound.md#test-buttons) to verify
inputs ([Bluetooth](bluetooth.md)).

**PC streaming (Moonlight) hasn't been paired against a real PC during dev** (launch, discovery and return flow were
verified, with pairing checked against a stub; see [PC streaming](pc-streaming.md#pair-once)).

**Smooth fan has an initial Mini V2 test.** Over an 8-minute load test, the fan spun up once instead of cycling twice
([Smooth fan](performance.md#smooth-fan)). Acoustics, minimum RPM and the 78 °C ceiling still need wider hardware
checks. If your fan acts strangely, turn off Settings → Playing → Performance & fan → **Smooth fan** to restore the
system curve immediately.

**Settings → Help & tools → Tools, Wi-Fi country and Backup & reset** were verified in our virtual device and are
awaiting real Mini V2 confirmation.

## Questions

**Stuck on the "pocket-ui" boot bar and the shelf never appears.**
After one minute it updates to "Still starting…". First boot on a fresh card or right after a system update can take a
few minutes, so give it time. If nothing moves after several minutes, hold Power to force shut down and turn it back on.
If it persists, boot Android (Volume Down + Power → **Switch boot mode**) and reach out for help; once you reach the
shelf, use [Report a problem](#report-a-problem).

**Starter pack says offline, or a download cut off.**
Make sure Wi-Fi is connected (Settings → Connections → Wi-Fi). Interrupted downloads never leave partial files; press A
to retry. If you see "Not enough space on the SD card", free up room first. Existing files are never overwritten.

**The SD card is full.**
In game, pocket-ui's own player shows "Couldn't save: the SD card is full" and prompts before quitting
([When your progress can't be saved](pause-menu-saves.md#when-your-progress-cant-be-saved)). On the shelf, unsaved
settings or favourites trigger "Couldn't save your changes: the SD card is full". Check free space under Settings →
Help & tools → About, then clear space over File sharing or [Remote access (SSH)](settings-system.md#remote-access-ssh) while
paused or after quitting: old save states (`roms/savestates/<system>/`, up to ~40 MB each for 3D consoles), screenshots
or unused ROMs. **Never delete `.srm`, `.rtc` or `.sav` files**, since those hold your in-game saves. Once space is free, the next retry (within 10 seconds) saves cleanly. If the card turns
read-only ("can't be written to right now"), rebooting often clears it; if it recurs, back up your saves and replace the
card.

**"Couldn't read this game's save".**
The SD card failed to read your in-game save at launch, often an early sign of a worn card. pocket-ui protects the
original file by not writing over it (your current session won't save). Quit, copy the `.srm` or `.sav` off over File
sharing, and run Settings → System → Backup & reset. If the error returns after a reboot, replace the SD card.

**A tool from Settings → Help & tools → Tools doesn't let me out.**
Hold HOME for a second and let go: pocket-ui stops the tool and Settings → Help & tools → Tools comes back. Tools that support it
also close with L1 held and SELECT + START.

**A tool I installed from the Store was gone after a restart.**
Older versions put Store tools into ROCKNIX's tools folder, which ROCKNIX empties at every start. They now go to
pocket-ui's own folder.

**I changed a setting and want the old ones back.**
Settings → System → Backup & reset lists the backups, including the one made before every reset and restore. Restore one (A,
then A again).

**I want my saves back, or I have to flash the card again.**
Settings → System → Backup & reset → Back up saves, then copy the folder `backup/pocket-ui-saves` to a computer: flashing erases it
with the rest of the card. Afterwards copy it back to the same place and restore it on the same page. What is newer on the card stays.

**"Another game has this file name": two games share save states.**
Save states are named after the game's file, so two games of one system with the same file name in different folders share them: the
moment you quit at, the save slots and Earlier. A game only continues on its own when it was the last of them you played and each of the others is in your recent games, and
deleting one leaves the shared states on the card. Saving in a slot or quitting one of them can still overwrite what the other saved
there. To keep them apart, rename one of the files. In-game saves lie beside each game and aren't shared.

**My device does not show up in the Bluetooth search.**
It has to be in pairing mode while the search runs: most controllers flash their light fast then. A controller that
was paired with a phone or a console before often tries to reconnect there first; switch Bluetooth off on that device,
or start pairing mode again. Keep it close to the Pocket.

**A button on my controller does nothing, or the wrong thing.**
Open Settings → Controls → [Test buttons](settings-sound.md#test-buttons) and press it: if it lights up, the
controller is fine and Settings → Controls → [Controller setup](controls-settings.md#controller-setup) puts it
where you want it; if it stays dark or shows **Not recognised**, send a report (Report a problem) with that name.

**My device was paired, but it does not reconnect.**
Switch it on (controllers: press the home button); give it a few seconds after the Pocket wakes up, since Bluetooth starts again
after sleep. If it still does not connect, select it in Settings → Connections → Bluetooth and press A. If that fails too, forget it
(X, X) and pair it again.

**My games don't show up.**
Check that each file is in the right system folder under `roms` (directly, or in one subfolder; nothing deeper is
read) and has a file type that system accepts. A folder with the name another system gives (a "Sega Genesis" next to
`megadrive`, "GBA" instead of `gba`, "Game Boy Advance (GBA)") is read as that system. A folder whose name no system
has is not read: Settings → Games → Add games names such a folder under its title. A disc game whose `.cue` (or `.gdi`) names a file that isn't
there is left off the shelf, because it could not start; Add games names it and the missing file. The list is read
when pocket-ui starts and when you leave a step opened from Settings. A system appears on Home only once its folder
holds at least one game.

**A symbol in a game's name is missing.**
The Pocket's font has no ticks, stars or emoji, so a game whose file name has one shows without it. Japanese,
Chinese and Korean names are shown with the system's Japanese font where it has one. Where it has none, kana and hangul
are written in Latin letters ("Suupaa Mario"), and kanji, which cannot be, are left out; a name of kanji alone shows
as "…". Facts and descriptions that nobody could read that way are left off the game's card. Nothing shows as "?". The
file itself is not changed.

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
quit it, not from a crash. With Settings → Playing → **Continue where I quit** off, Quit keeps no new moment, on purpose: the game starts fresh and the older moment stays under Load (see [Continue where I quit](settings-playing.md#continue-where-i-quit)).

**A game now starts with RetroArch instead of pocket-ui's own host.**
After a confirmed failure before the first game picture, pocket-ui may try RetroArch once, as long as there are
no pocket-core moments. The next launch tries pocket-core again. A crash after the game showed a picture keeps
pocket-core and Continue. Older automatic switches are cleared on update; your own emulator choices stay.
Check [Runs on](options.md#runs-on) to choose an emulator yourself (see
[Systems](systems.md#when-a-game-does-not-start-on-the-own-host)).

**A game starts with a different emulator than before, or doesn't start at all.**
Someone chose another emulator for it or its console, in pocket-ui or earlier in ROCKNIX's own menu. Press
START on its card: [Runs on](options.md#runs-on) shows what plays it and where that comes from. "Not installed"
means ROCKNIX can't start that choice on this Pocket; pick another one, or Options → Restore defaults.

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
files instead: "... didn't start. Sega CD needs a BIOS file in roms/bios: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin. BIOS files come from your own console."
Neo Geo games name `neogeo.zip`, which may also sit next to the games ([Arcade games](arcade.md#when-a-game-does-not-start)).
A game that never started is not counted as played: it does not become Continue and is not in the switcher.
For a zip file, pocket-core says what is wrong with it: [see below](#a-zip-game-says-it-holds-several-games-is-damaged-or-cant-be-opened).

**A game shows "... didn't start. Its emulator crashed while loading the game".**
pocket-ui's own player stopped while it loaded the game, without a reason of its own. Start the game once more; when
it crashes again, copy the game file to the card again, as a damaged file is the usual cause.

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
ROCKNIX's settings set to "custom", and for ten minutes when the system kept changing the fan back (pocket-ui then gives
way rather than fight over it). `pocket-ui.log` says which, in lines starting with `fan curve:`.

**A game's sound crackles and the game slows down.**

High-rate cores such as ClownMDEmu now keep their original pitch and duration when converted to the device’s output rate.

PAL games running on a 60 Hz screen keep a little more sound buffered so repeated pictures do not interrupt
the audio. Returning from pause preserves that queued sound and waits for enough audio for the next frame before fading in.
Starting or continuing a game keeps its opening sound while the audio output connects, including when a Continue
moment restores a large audio buffer. If muting the device stops its audio output, the first connection wait
is limited to two seconds; the game then keeps running and discards inaudible sound. When the output runs
again, fresh sound fades back in instead of replaying the muted backlog.
Try the same scene with another available core or player in the game's **Options → Runs on**. If that plays correctly,
include both choices, the game and your device in [Report a problem](#report-a-problem). This helps distinguish a
player timing problem from a game that is too demanding. On H700 handhelds, GBA normally uses gpSP; older Continue
moments can still use mGBA. Changing the player can require starting the game again; keep your in-game save first.

**The menus make no sound.**
Settings → Screen & sound → Menu sounds must be on, and the console's volume must be above 0 and not muted. The clicks never play while
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
when the Wi-Fi page opens while connected, and when an update or the PortMaster catalogue fails, pocket-ui
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
being made. A saved network whose password is missing from the system's network settings, or that
ROCKNIX's own menu emptied there, is repaired at the next start from the saved network. If it lost the password on both
sides, Connect asks for it once: nothing has to be forgotten first.

**Box art found nothing for some games, or the wrong picture.**
Cartridge games are recognised by their checksum, other games by file name ([Box art & downloads](extras.md#box-art)). For one
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
or switch off ([Safe mode](safe-mode.md)). ROCKNIX's own menu only opens by itself if pocket-ui
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
message. Power off, Restart and switching off after sleeping are the normal way and never bring it up, and on dArkOS
neither does switching off from dArkOS's own menu after Back to EmulationStation.

## Will my games start?

Settings → Games → **Game check** looks at the whole card and says what stands in the way: a missing or wrong BIOS, a
disc without its track file, empty files, zips without a game, duplicates, files in the wrong folder. See
[Game check](game-check.md).

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

A zip or `.7z` file that pocket-core can't read otherwise (a rare compression method) is still tried with RetroArch, as before; standard `.7z` archives (LZMA, LZMA2, PPMd) open directly in pocket-core.

## Report a problem

When something goes wrong, Settings → Help & tools → **Report a problem** (also in About, and in the question of
SELECT + START held on the shelf) puts what we need to find it into one
file. Opening the row from Settings shows About with **Report a problem** selected: press A there once. When a game doesn't start, the message on the shelf says how to get there. A few seconds later the Pocket shows the file's name, for example `report-2026-10-26-164007.zip`
(the date and time on the Pocket's clock, as for screenshots).

Sensor readings and logs may contain Unicode (for example °C and µs); these are kept in the report.

**Where it is.** In the `reports` folder of the games share, the same place as `screenshots`: `roms/reports` on the SD
card. Copy it to your computer over [file sharing](getting-started.md#over-wi-fi-file-sharing).
On a ROCKNIX system card with the games on STORAGE and no second card, look in the `reports` folder of the ROCKNIX drive.
Turn the Pocket off before putting the card in your computer. If the Pocket put a copy on the games card in the second
slot, the confirmation names that card.

**What to do with it.** Open a bug report on pocket-ui's GitHub page (Issues → New issue → Bug report) or write to
robby@robbyczesany.com. Say what happened, and attach the zip. Nothing is sent by the Pocket itself.

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

**How do I get to ROCKNIX's standard menu?**
On the shelf, hold SELECT + START for one second and choose **Open EmulationStation**, or Settings → Help & tools → About → **Open EmulationStation…**. Read the warning first: do not use Updates or Factory reset there. Restart the Pocket to come back
([Safe mode](safe-mode.md#opening-rocknixs-own-menu)).

**Where does pocket-ui keep its data?**
In `/storage/.config/pocket-ui/` on the SD card:

| File or folder | Contents |
|---|---|
| `settings.json` | Your look, appearance, motion, On start, the systems on the shelf, and whether the first start is done. If it can't be read (and its backup can't either), pocket-ui says so at the start and uses the standard settings |
| `recents.json` | Recent games and play times |
| `game-settings/` | The pause menu's Options (picture, emulator, controls, shortcuts, extras) per console and per game |
| `slots/` | The save slot each game used last |
| `fallback.json` | Legacy automatic emulator choices, cleared on update; new retries apply once only |
| `covers/`, `snaps/`, `titles/` | Cover art, and the screenshots and title screens beside it ([Pictures](extras.md#pictures)) |
| `favorites.json` | Starred games (if you use Favourites) |
| `library.json` | Play status, notes, sessions and legacy labels ([Your library](library.md#where-it-is-kept)) |
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
Each step is in Settings: Look (Look), Wi-Fi (Connections), Box art & downloads and Add games (both under Games).
<!-- TODO: if a full reset of the first start is wanted, document it (today: set "onboarded" to
     false in settings.json). -->

**Does pocket-ui change anything on Android?**
No. See [What it does not touch](index.md#what-it-does-not-touch).
