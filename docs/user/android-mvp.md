# Android preview

pocket-ui for Android brings the same cozy shelf and built-in game player to Android handhelds and phones. Pick your ROM
folders via Android's folder picker (subfolders inside recognised console folders work automatically, and M3U/CUE disc
sets group cleanly into one game). Built-in player cores include mGBA (GB/GBC/GBA, with optional Gambatte for GB/GBC in
Settings, with separate saves/states), Nestopia (NES), bsnes (SNES), Gearsystem (Master System/Game Gear), ClownMDEmu (Mega
Drive), Beetle WonderSwan, Stella (Atari 2600), Beetle PCE Fast (PC Engine), Beetle NGP (Neo Geo Pocket/Color), Handy
(Atari Lynx), PCSX ReARMed ([PlayStation](#playstation)), YabaSanshiro (Saturn), melonDS DS (Nintendo DS), PPSSPP (PSP),
Mupen64Plus-Next (N64) and Flycast (Dreamcast). Heavier systems like PS2, GameCube, Wii, 3DS and Xbox launch straight
into installed emulator apps. No games, BIOS files or keys are included.

On phones, four bottom tabs (**Games**, **Favourites**, **Search** and **Settings**) keep navigation thumb-friendly (add
**Curator** as a fifth tab via Settings → **Curator tab**). Tap once to lift a case, tap again (or tap Play) to launch,
and long-press or tap Info for the game card. Portrait stacks shelves vertically (tap the Games tab to jump back to the
top); landscape lays the shelf beside the highlighted game. Connect a gamepad and D-pad, A/B/X/Y, L/R (tabs), START
(Settings), SELECT (Quick settings) and R2 ([your other apps](#your-other-apps)) take over automatically, showing button
hints until you touch the screen again.

On phones, touch controls appear automatically and hide the moment you press a controller button (tap the screen once to
bring them back). Back, Pause or controller Menu opens the pause menu (Continue, Save, Load, Earlier, Options, Zoom,
Screenshot, Save & quit; launching from Handheld view opens the classic side-by-side handheld pause menu instead).
**Save & quit** writes a dedicated auto-state safely before closing (Android Home suspends and saves too), keeping
separate states even when two games share a filename in different folders, and keeping multi-disc CUE/BIN and M3U games
under one Continue entry. Because Android deletes private app data when you uninstall, make a
[save backup](#save-backup) first (or enable Saved copies below).

If an older save state can't resume, or your in-game save is newer, Recovery tells you cleanly and keeps that state under
**Load** (preserving it across app updates too).

UI controls stay at least 48 dp and steer clear of camera cutouts, rounded corners and gesture bars in both portrait and
landscape.

On Android handhelds, every physical input works including analogue L2/R2 triggers and L3/R3 stick clicks (resting a
trigger halfway won't false-fire, and Odin's L2/R2 mode setting works in any mode). R2 on a game card opens its Controls.

**A and B:** Menus (both Mobile and Handheld views) follow your handheld's Android Controller style (on Odin, Xbox style
confirms with bottom, Odin style with right; Settings → Buttons on Handheld view adjusts **Button symbols** only).
In-game controls stay faithful to the original console layout (right button = console A) across Odin, Xbox- and
Switch-style pads. Flipping Odin's Controller style mid-game pauses safely and keeps Player 1 assigned.

**Multiplayer:** Plugging in or pairing a Bluetooth/USB controller adds Player 2 to 4 automatically while your built-in pad
stays Player 1 (single-player games accept input from any pad). Disconnecting a pad pauses the game and reclaims its
slot on reconnect. Up to four players; menus respond to every connected pad.

**Default launcher:** Want pocket-ui as your handheld's default launcher? Press Home and pick pocket-ui (or go to
Android Settings → Apps → Default apps → Home app). pocket-ui never sets itself automatically, and B/Back never exit it.

**Time and battery:** Every page header displays the clock (following Android's 12/24-hour format) and battery ("87%",
"87%, charging") without needing extra permissions, staying hidden whenever Android's status bar is visible or mid-game,
and gracefully dropping extra words on huge font sizes so titles never clip.

### Your other apps

Tap **Apps** at the top of Games (or press R2 on a pad) to open a clean drawer with Play Store, Settings and your
Android games (missing icons show **App**). Tap or press A to launch; B/Back returns to your shelf. Long-press any app
(or press Y) to pin it onto a dedicated **Android** shelf right beside your retro consoles, complete with favourites,
stickers and play time (uninstalled apps leave automatically). Android's game tags show "Looks like a game", though
only you choose what goes on your shelf.

On phones, Select and Start sit cleanly above the portrait D-pad and face buttons so taps never overlap. System Back
opens the pause menu (and steps back from Options first without quitting). The pause menu adapts to Light/Dark mode and
language, wrapping long game titles and status lines cleanly. Screenshots save privately for now (carousel import and
gallery export are still preview items).

Nintendo DS (melonDS DS) and **experimental** PSP (PPSSPP), Nintendo 64 (Mupen64Plus-Next), Dreamcast (Flycast) and
Saturn (YabaSanshiro) all run inside pocket-ui's own player. Experimental systems carry an "Experimental" label while
speed, audio and Continue reliability are still being refined (don't keep your only progress copy solely in states
there; DS is not experimental). On DS, both screens stack with bottom-screen touch; N64 adds Z/C buttons and temporary
audio buffering on fresh starts to smooth out intro loads; PSP draws native cross/circle/square/triangle symbols and
holds rapid taps for at least one frame so quick presses never drop. PSP and the other hardware-rendered cores
now keep the game picture on the GPU during play. The pause menu, save previews and screenshots still show the game
picture. Performance varies by game and handheld.

Single-file games (including PSP ISOs and cartridges) load straight from your chosen folder without duplicating storage
(keep your card/folder mounted while playing). Multi-file games or stream-only storage providers make a cached copy
(showing progress + Cancel over 64 MiB, caching up to five large titles and auto-pruning older copies while keeping
saves untouched).

Real 3D game cases, printed discs, cartridges, Cabinet TV previews, interactive stickers (**Completed**, **100 %**,
**Favourite**, **Playing now**, **Game of the year**), shelf decorations, time-of-day lighting, Cozy extras, Connections,
Rediscover, [Continue](continue.md#on-an-android-phone) and [RetroAchievements](#retroachievements) all work right on
Android (Split hides cases and stickers; *This game* pre-launch options aren't in Mobile view). In Shelf and
Cabinet, rows stack vertically in portrait (swipe sideways along a shelf, up/down between shelves) and fill the screen
in landscape.

**First start:** 7 touch steps (Welcome, Language, Game folders, Look, Covers & pictures, Controls, Ready), or just 4
streamlined steps when using a controller (Language, Games, Look, Ready, with covers auto-fetching on unmetered Wi-Fi).
Auto-scans common ROM folders (`ROMs`, `Emulation/roms`, `RetroArch/roms` across internal storage and SD cards), spots
installed external emulator apps (linking to official pages for missing ones, never downloading apps itself), and flags
missing BIOS files with their folders. Supports English, German, Spanish, Brazilian Portuguese, French, Italian, Korean, Simplified Chinese, Vietnamese and Japanese;
choose Shelf, Easy, Split or Cabinet, Cozy or Quick animations, and Dark or Light mode. Re-run anytime via Settings →
First start.

**Covers and pictures:** choosing "Download on Wi-Fi" loads box art, screenshots and Wikipedia descriptions on demand as
you browse over unmetered Wi-Fi (never bulk-scraping your whole card in the background; cached art works offline).
Change anytime under Settings → Covers and pictures.

**Your own cover** (Handheld view): open a game's card and pick **Choose cover…** to choose a picture with Android's
photo picker (no storage permission needed). PNG or JPEG up to 8 MB; pocket-ui shrinks it to at most 1024 pixels on the
longest side, and it shows at once on the shelf, the case and the spine. **Reset to automatic** brings back the usual
cover; pocket-ui keeps a copy of a replaced or reset cover, as the web companion does. Cancelling or a picture it can't
read leaves the cover as it was and says why.

**Picture & Screen frames:** Player Options → Picture matches pocket-ui OS: **Screen scaling** (Aspect, Integer,
Fullscreen), **Look** (Clean, Smooth, Soft, Bloom, LCD grid with LCD colours on handhelds, Scanlines, Old TV), **Screen
frame** (same hardware-drawn bezels as Linux, excluding Ambient; **Console** picks the matching handheld shell or monitor
surround, **CRT TV** draws the tube TV) and **Resolution** on supported Snapdragon handhelds. With touch controls on,
frames sit safely inside cutouts and clear of buttons; with a controller connected, the picture uses the entire display
exactly like pocket-ui OS (e.g. GBA 6× integer at 1440×960 on 1080p). **Save settings for** switches between This system
and This game.

Options inside the player group cleanly into Picture, Sound, Controls and Speed, tucking core-specific settings under
**Advanced (this emulator)** (SNES exposes six plain-English toggles like "Less sprite flicker"). Touch controls mimic a
real pad: rollable round D-pad, console-accurate button layouts (PlayStation shapes on PSP, C buttons on N64, 6 buttons on
Mega Drive), shoulders, translucent styling and optional haptics (Off by default; respects Android vibration settings).

**Saturn & Dreamcast** (experimental): Saturn requires `saturn_bios.bin`, `sega_101.bin` or `mpr-17933.bin` in a chosen
folder (supports CUE/BIN, ISO, CHD, M3U). Dreamcast boots with a built-in HLE BIOS or improves compatibility with
`dc_boot.bin` / `dc_bios.bin` and `dc_flash.bin` inside a chosen `bios` folder (supports GDI, CUE/BIN, CDI, CHD, M3U).
Run **Game check** after adding BIOS files. pocket-ui copies originals into private app storage without touching your
files.

## Game check

Settings → **Game check** scans your selected folders offline without touching or deleting anything, reporting **All
good** with your ready game count or a concise issue list (auto-offered when adding a folder with new games; press X to
re-check):

| Row | Example | What to do |
|---|---|---|
| **Folders that can't be read** | "Android no longer lets pocket-ui read Games."; "Games is not there any more." | Pick **Choose it again** (your files are safe) or forget a removed folder |
| **Discs missing a file** | "Disc: Game.cue is there, its Game.bin is missing." / "is empty" | Copy the missing track beside it (case-insensitive matching works automatically) |
| **Empty files**, **Zips that won't start** | Corrupt or empty archive | Recopy the game file |
| **Same game twice** | `Game (1).gba` beside `Game.gba` | Both stay visible; remove the duplicate with a file manager if desired |
| **Wrong folder or file type** | `.gba` inside `snes`, `.sfcc`, `Game.sfc.part`, or ambiguous `.iso`/`.chd`/`.zip` in an unrecognised folder | Move or rename as suggested (cartridge extensions auto-detect even inside custom folders like `Sega Genesis/Sonic.md`) |
| **Emulator apps to install** | "2 games need the ARMSX2 app, which isn't installed." | Install the app from its official store or GitHub |
| **Can't start in this app yet** | Files >512 MB outside PSP/PS1/Saturn/Dreamcast, large copies exceeding free storage, PC Engine CD or Vita | Stays safely on the shelf |
| **Tips** | Optional PlayStation/PS2/Dreamcast BIOS notes | Purely informational; never takes away **All good** |

With a controller connected (including built-in handheld buttons), pocket-ui locks cleanly to landscape (either way up);
on phones without a pad, it rotates freely.

## PlayStation

PlayStation runs inside pocket-ui's built-in player on **PCSX ReARMed**:

- **Formats:** `.cue`/`.bin`, `.chd`, `.pbp`, `.iso` and multi-disc `.m3u` inside `psx`, `ps1` or `PlayStation`
  (case-insensitive track names match automatically).
- **Storage:** single `.pbp`, `.iso` and standalone `.chd` files launch directly; CUE/BIN, M3U playlists and
  parent-linked CHDs copy into app cache (progress bar + Cancel above 64 MiB; reused on later starts; keep all disc
  files inside or below the `.cue`/`.m3u` folder).
- **BIOS:** optional. Built-in HLE works out of the box, though dropping a real `scph5501.bin` into a chosen `bios`
  folder improves compatibility for tricky titles (auto-imported on next PS1 launch; noted once if missing).
- **Multi-disc:** open Pause → **Options → Disc** when a game prompts for the next disc.

## Games in other emulator apps

PS2, GameCube, Wii, 3DS and Xbox games sit right on your shelf and launch straight into their dedicated Android apps.
Exit the game there and you're right back on your shelf (Continue covers pocket-ui's own player only):

| System | Folder and files | App | Where to get it | Once in the app |
|---|---|---|---|---|
| PlayStation 2 | `ps2`: `.iso`, `.chd`, `.cso` | ARMSX2 | Google Play ("ARMSX2 (PS2 Emulator)") or github.com/ARMSX2/ARMSX2, Releases | Add your console's BIOS |
| GameCube and Wii | `gamecube`, `wii`: `.rvz`, `.iso`, `.gcm` | Dolphin | F-Droid, Google Play or dolphin-emu.org | Set up its pad, touch buttons and picture once (below) |
| 3DS | `3ds`: `.3ds`, `.cci`, `.cxi`, `.3dsx` | Azahar | azahar-emu.org or Google Play | Complete first-start data folder setup |
| Xbox | `xbox`: `.iso` (also `.xiso.iso`) | hakuX | github.com/rfandango/hakuX, Releases | Pick your Xbox games folder |

**Setting up Dolphin.** Because Dolphin doesn't take settings from pocket-ui, Handheld view shows a quick one-time setup
card before your first GameCube or Wii game (matching Dolphin's English menus): Settings › GameCube Input › GameCube
Controller 1: Standard Controller → Device: your built-in pad (same under Wii Input for Wii); hide touch controls with
Back → Overlay Controls › Toggle Controls › Toggle All; set Screen Orientation During Emulation: Landscape and Graphics
Settings › Aspect Ratio: Auto. Hit **Start the game** when ready. The card stays handy on the game card as **Setting up
Dolphin**.

**Leaving a game:** close via each app's own menu to return straight to pocket-ui: ARMSX2 Back → **Close**, Dolphin
Back → **Exit Emulation**, hakuX Back → **Exit**, Azahar Back → side menu → **Close Game → OK** (Azahar launches in a
fresh task so closing always returns cleanly to pocket-ui; also listed under Settings → **Help**).

If an app isn't installed, Play tells you which app you need and where to get it. External apps manage their own saves;
pocket-ui still tracks your play time.

## Android extras

> **Optional. pocket-ui works fully without this.**

Android extras are for players who want pocket-ui to feel even more like the whole handheld. Open them in Settings → System → **Android extras**. Nothing changes until you set one up, and every step can be undone. The top row, **Level**, shows how far you have come.

| Level | What you do | What it gives you | How to undo it |
|---|---|---|---|
| **1. Standard** (nothing to set up) | Nothing | Everything pocket-ui does without special rights: the shelf, the player, the [Home app](settings-system.md) option, pocket-ui's own volume bar, Settings → System → **Run in background** (Android stops holding pocket-ui back, for example during a download with the screen off) and the clock and battery in the header | Nothing to undo |
| **2. ADB grant** (once, from a computer) | `adb shell pm grant xyz.pocketui.app android.permission.WRITE_SECURE_SETTINGS`, then **A** on the **ADB grant** row (**Check again**) | The four switches: **No pop-up notifications**, **No animations**, **Stay awake while charging** and, on Android 10 and older, **Hide system bars**. The grant stays after restarts | Switch the switches **Off** (Android's own values come back), then `adb shell pm revoke xyz.pocketui.app android.permission.WRITE_SECURE_SETTINGS`, or uninstall pocket-ui |
| **3. Shizuku** (no computer, Android 11 or newer) | Install Shizuku, turn on Wireless debugging, pair and start Shizuku, then allow pocket-ui | Everything from level 2, with the grant given and taken back by one **A** on the **ADB grant** row, **Save state** and **Load state** in the [pause menu over other emulators](#pause-menu-in-other-emulators) (Android 13 and newer), and the whole system log in a [problem report](#report-a-problem) | **A** on **ADB grant** (**Take back**), then uninstall or stop Shizuku. After a restart Shizuku has to be started again, your switches keep working |
| **4. Root** (only if your handheld is already rooted) | **A** on **Root (Magisk)** (**Check root**) and allow pocket-ui in Magisk | The same as Shizuku, through Magisk. Nothing more on the page today | **A** on **ADB grant** (**Take back**) |

The **Pause menu in other emulators** is its own switch and does not need any level. You turn it on at Android's accessibility page, see [below](#pause-menu-in-other-emulators).

### Level 1: Standard

This is how pocket-ui starts and how most players use it. The shelf, the player, **Continue**, the volume bar and the games in other emulator apps all work without an extra. Two rows in Settings → System make it feel more like your own handheld, both with one question from Android and no computer:

- **Home app:** **On** makes pocket-ui your handheld's Home app, so it starts on the shelf after switching on.
- **Run in background:** **On** takes pocket-ui off Android's battery optimisation. Android can still close it when it needs the memory.

### Level 2: ADB grant

You need a computer with Android's free Platform-Tools (`adb`) and a USB data cable. It takes about two minutes, once.

1. On the handheld, turn on **Developer options**: Android's **Settings → About handheld** (or *About phone*), tap **Build number** seven times, then open **System → Developer options**. On Samsung it is *About phone → Software information → Build number*, on Xiaomi **OS version** or **MIUI version**.
2. Switch on **USB debugging** and connect the cable. Tap **Allow** when Android asks about your computer.
3. On the computer, run `adb devices` (it should list your handheld as `device`), then:

   ```bash
   adb shell pm grant xyz.pocketui.app android.permission.WRITE_SECURE_SETTINGS
   ```

   If the command returns with no text, it worked.
4. In pocket-ui, open Settings → System → **Android extras**, highlight **ADB grant** and press **A** (**Check again**). It says **Granted**, and **Level** says **ADB grant**.
5. Highlight a switch and press **A** (**Change**) to turn it **On**.

What the switches do, all over Android and not only inside pocket-ui:

| Switch | Effect | Note |
|---|---|---|
| **No pop-up notifications** | No banner drops down over your game | Some manufacturers' Android builds ignore Android's own setting |
| **No animations** | Apps and menus open at once | |
| **Stay awake while charging** | The screen stays on while the handheld is plugged in | |
| **Hide system bars** | Android's status and navigation bars stay hidden in every app | Android 10 and older only. Android 11 removed this setting, so the row is not shown there. pocket-ui's own screens and player hide the bars anyway |

pocket-ui writes down Android's previous values first, so **Off** puts back exactly what was there.

### Level 3: Shizuku

[Shizuku](https://shizuku.rikka.app/) is a free app that lends pocket-ui Android's shell, with no computer. You need Android 11 or newer and Wi-Fi.

1. In pocket-ui, highlight **Shizuku** and press **A** (**Get Shizuku**), or scan the code on the page. Install it and allow notifications for it.
2. In Android's Developer options, switch on **Wireless debugging**.
3. In the Shizuku app, tap **Pairing**, then in Developer options tap the text **Wireless debugging** and choose **Pair device with pairing code**. Enter the 6-digit code in Shizuku's notification.
4. In Shizuku, tap **Start** under *Start via Wireless debugging*. The top says **Shizuku is running**.
5. Back in pocket-ui, press **A** on **Shizuku** (**Allow**) and choose **Allow all the time** in Shizuku's question. It says **Ready**.
6. If **ADB grant** says **Not granted**, press **A** (**Grant**). With the grant already active, **A** on that row takes it back.

**After a restart:** Android stops Shizuku whenever the handheld restarts (it can start by itself on a rooted device). Your grant and switches keep working. Start Shizuku again when you want to take the grant back or send the whole system log: Wireless debugging on, **A** on **Shizuku** (**Open Shizuku**), **Start**. You do not pair again.

Shizuku also lets pocket-ui add the whole Android system log to a [problem report](#report-a-problem) (Settings → Help & tools → **Report a problem**), which helps a lot when another emulator app misbehaves.

### Level 4: Root

For tinkerers, and not needed: Shizuku covers nearly everything. If your handheld is **already** rooted with Magisk, select **Root (Magisk)** and press **A** (**Check root**), then allow pocket-ui in Magisk. Root then does what Shizuku does (the grant on **A**, the system log, Save state and Load state in the pause menu). Today that is all it adds on the page. This guide does not explain how to root a device.

## Pause menu in other emulators

This row works at every level. It needs no computer and no Shizuku.

1. Open Settings → System → **Android extras**, highlight **Pause menu in other emulators** and press **A** (**Accessibility page**).
2. Find **pocket-ui pause menu** (often under *Downloaded apps* or *Installed services*), switch it **On** and confirm.
3. Back in pocket-ui the row says **On**.

On Android 13 and newer the switch can be greyed out for an app installed outside the Play Store. Open Android's **Settings → Apps → pocket-ui**, tap **⋮** and **Allow restricted settings**, then turn the service on.

Start a game from the shelf in ARMSX2, Dolphin, Azahar, PPSSPP, DuckStation, hakuX or RetroArch, then press **HOME** or **SELECT + START**: pocket-ui's pause menu opens over it with **Continue**, **Back to the shelf** (the game keeps running behind it) and **Emulator's own menu** (where you save and quit). Games not started by pocket-ui are left alone.

**Save and load from the menu** (with Shizuku or root, Android 13 and newer): the menu also has **Save state** and **Load
state**. pocket-ui presses the emulator's save key (**F2**) or load key (**F4**) for you. RetroArch knows these keys from
the start; for every other emulator, teach them once: open the emulator's controls or hotkey page, pick its save state (or
load state) entry so it waits for a key, then open pocket-ui's pause menu and choose **Teach the save key (F2)** (or **Teach
the load key (F4)**). pocket-ui presses the key a moment later. On Android 14 and newer the key goes only to the emulator.
On Android 13 pocket-ui checks right before and after that the emulator is in front; in rare cases, when another window
comes in front in that very moment, another app can get the key. A short line at the bottom says what happened:

| Line | Meaning |
|---|---|
| **Saved** | pocket-ui saw a new or changed save state file in the emulator's usual folder (it can't tell which game or slot) |
| **Save key sent** / **Load key sent** | The emulator got the key; for a save, pocket-ui found no new file (a state folder of your own, or the key not taught yet) |
| **Not saved** / **Not loaded** | Something else came in front, so nothing was pressed. Try again |
| **Not sent safely** | pocket-ui can't confirm where the key went: another window came in front just then, or Android or Shizuku didn't answer in time. Check in the emulator whether it saved |

End a game in the emulator's own menu.

Good to know: some handhelds keep **HOME** for Android (**SELECT + START** still works), a back gesture is no key at all, the game sees a short **SELECT** press before **START**, and whether it pauses behind the menu depends on the emulator.

## Undo everything

1. Settings → System → **Android extras**: **A** (**Change**) on every switch that is **On**.
2. **ADB grant**: **A** (**Take back**) with Shizuku or root running, or `adb shell pm revoke xyz.pocketui.app android.permission.WRITE_SECURE_SETTINGS` from a computer. Uninstalling pocket-ui removes it as well, so turn the switches off first.
3. **Pause menu in other emulators**: **A**, then switch **pocket-ui pause menu** off on Android's accessibility page.
4. Optional: uninstall Shizuku, switch off USB debugging, Wireless debugging and Developer options.

## Plays with

Every system with a built-in core runs inside pocket-ui by default. Settings → **Plays with** lets you hand any system
over to an installed external emulator (RetroArch, PPSSPP, DuckStation). Only installed apps appear, and uninstalling one
falls back to pocket-ui automatically. Experimental built-in cores (N64, PSP, Saturn, Dreamcast) are labelled
"Experimental" beside **pocket-ui**. External apps use their own pause menus and Continue (RetroArch needs its matching
core installed inside RetroArch); switch back to **pocket-ui** anytime to restore pocket-ui's pause menu and Continue.

## Save backup

Settings → **Save backup** gives you three ways to protect your progress:

- **Export saves:** packs all in-game saves, save states, slots and thumbnails into a single
  `pocket-ui-saves-<date>.zip` wherever you choose (internal storage, SD card or cloud folder; aborts cleanly if any
  file disappears mid-export).
- **Import saves:** restores from any pocket-ui backup ZIP. Games without local saves restore straight away; if local
  saves already exist, you choose **Keep this phone's saves** (fill missing only) or **Use the backup's saves**
  (overwrite). Nothing is overwritten without asking.
- **Saved copies:** shows live status of automatic folder backups (On, Off, Folder missing) and triggers an immediate
  sync on A (or reopens folder permissions if Off/missing).

Reinstalling or moving phones? Pick the same ROM folder and import, and games reconnect by console and filename (games not on
the shelf yet hold their saves until their folder is added). Settings, favourites, stickers and the Continue shelf list
aren't part of save backups (Continue refills as you play).

### Saved copies in your game folder

No manual exports needed: whenever a game closes (**Save & quit** or exit), pocket-ui quietly mirrors its saves and
states in the background into a `pocket-ui saves` folder inside your chosen ROM folder (one subfolder per game plus a
`README.txt`), completely safe from Android app uninstalls.

Reinstall pocket-ui, point it at the same ROM folder, and missing saves restore automatically. Existing saves on the
device are never overwritten blindly: identical files skip, newer saves win over older copies, and conflicts are flagged
without losing either version, and failed reads never touch your phone's save. Requires write permission to your ROM
folder (grant when picking the folder; if denied or missing, **Saved copies** reads **Off** or **Folder missing**).
Don't manually rename files inside `pocket-ui saves`.

## Help

Settings → **Help** offers quick built-in answers on picking folders, extra systems, pause menu, saves, **Plays with**,
exiting external emulators and reporting bugs (A opens a topic, B returns).

## RetroAchievements

Works right inside pocket-ui's own player just like on Linux handhelds ([Achievements](achievements.md); external apps
handle their own).

- **Sign in:** Settings → **RetroAchievements → Sign in** with your free retroachievements.org username and password
  (sent once over HTTPS and never saved; only the private token stays on device, and **Log out** asks twice and clears it). Zero
  network traffic unless you sign in.
- **In-game:** zero startup delay, works on direct ROMs, zips, ISOs, CUEs and M3Us (shows "Achievements: 12 of 40" and
  4-second unlock cards; *softcore* mode keeps save states, Earlier and cheats active; loading states resets triggers).
- **Achievements list:** bottom row of the pause menu, showing locked achievements with progress first, then unlocked ones.
- **Offline:** previously played games unlock offline and queue uploads for your next online launch.
- **Auto 100% sticker:** mastering every achievement automatically places the golden "100 %" sticker on the box once
  (counts toward your shelf trophy; removable anytime).
- **Limits:** `.chd` discs can't be hashed on Android (use CUE/BIN, ISO or M3U for achievements), and unlock chimes
  aren't on Android.

## Report a problem

Settings → **Report a problem** builds one clean diagnostic text (app version/build, Android device/display/controllers
by USB ID, look/language, per-system game counts and cores, last audio stats and log tail, capped at 60,000 chars) with
**Share the report**, **Copy the report** (for GitHub Issues or email to robby@robbyczesany.com) and **Back**. Nothing
is uploaded automatically, and private details (IPs, emails, passwords, tokens, folder paths, phone/controller names and
game titles, replaced by consistent anonymous IDs like `[game gba 1a2b3c4d]`) are scrubbed out automatically.

## Easy and Just Play

Pick **Easy** under Settings → Look (or first start) for one giant game at a time with Previous/Next and Play/Continue.
**Just Play** (on Games and Quick settings) picks and launches a built-in-player game in one tap, or press X on Quick
settings' Just Play row with a controller to preview why it picked that game (Right rolls another).

The Games shelf's top **Continue** row lists only verified resumable moments from pocket-ui's player (recently played
external-app titles stay on their console shelf). Failed **Save & quit** stays open with the error; older short PSP
states may stay under Load instead of auto-resuming.

## Android 0.2

[Zoom and Hold to toggle](controls.md#on-android) help with small RPG text and holding down run buttons. **Text size**
respects the larger of the app setting and Android's font scale (up to 200%). Turning on Settings → **Remind me where I
was** (Off by default) shows [Where was I?](continue.md#on-an-android-phone) after long breaks and waits for a press.
Battery saver pauses optional artwork downloads and startup animations without throttling gameplay.
The screen stays on while an update, the starter pack or a RomM game downloads, and during play; otherwise Android's own
screen timeout counts.

**Retro style** (Settings → Style → Retro) brings pixel fonts, chunky frames and Cocoa/Pea palettes. Cards from other
handhelds (`Game Boy Advance (GBA)`, `Sega Genesis`, existing local box art) work straight out of the box without
writing to your card. For mGBA, Gambatte, bsnes and Nestopia, dropping a matching `.cht` file beside a ROM adds
**Cheats** to Pause → Options (saved per game/core; memory-write codes flagged; no bundled database).

Save safety stays strict: failed loads keep Before load intact, double failures pause safely with **Return to library**,
and Earlier states remain safe. Game and console titles always stay in their original language across all UI elements,
and the player requests Android's low-latency audio path with gentle sample-rate tuning.

## More systems

Since 0.5.x, **every built-in core ships inside the APK**, from 8-bit through PS1 and Saturn plus PSP (with PPSSPP's
system files), DS, N64 and Dreamcast. Nothing extra needs downloading. Older separately downloaded cores clean up
automatically on next start while preserving saves, states and options. No games or console BIOS files are included.

## Licences

**Settings → Licences** lists pocket-ui's GPL v2+ licence alongside every bundled emulator's licence, exact upstream
commit and patches (matching source bundles live beside each release APK, or open a "Source request" issue anytime).

## Updates over Wi-Fi

Open **Settings → Updates** to check your version, tap **Check again**, or toggle **Check for updates automatically**.
Uses its own signed Android APK channel over **Wi-Fi only** (never mobile data; press B or Cancel to stop). First time
only, Android asks permission to install updates from pocket-ui, then asks to confirm each install; reopen pocket-ui
once done; games, folder permissions, settings and saves stay untouched. *(Never uninstall to update; if you ever do
uninstall, tick Android's **Keep app data** or back up saves first.)*

In Handheld view, idle daily Wi-Fi checks show a one-time shelf card when a new release drops (**A** installs, **B**
dismisses; turning off auto-check in Mobile view disables both).
