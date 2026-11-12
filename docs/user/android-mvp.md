# Android preview

This APK is an early Android version of pocket-ui. Pick ROM folders through Android's folder picker; recognised
system folders can contain subfolders. The app remembers access and the selected game. It reads M3U/CUE disc sets
as one entry. GB/GBC/GBA run in our own player with mGBA, NES with Nestopia, SNES with bsnes, Master System/Game Gear with Gearsystem, Mega Drive cartridges with ClownMDEmu, WonderSwan with Beetle WonderSwan, Atari 2600 with Stella, PC Engine with Beetle PCE Fast, Neo Geo Pocket/Color with Beetle NGP, Atari Lynx with Handy and PlayStation with PCSX ReARMed ([PlayStation](#playstation)). Settings can select Gambatte for GB/GBC; its saves and states are separate from mGBA. Saturn is also bundled. PSP, DS, N64 and Dreamcast are optional downloads for the built-in player. Other larger systems need an installed emulator.
No games, BIOS or keys are supplied. Install and configure external emulators separately.

On a phone the tabs are at the bottom: Games, Favourites, Search and Settings. **Curator** (filters such as genre, players and
length) comes back as a fifth tab with Settings → **Curator tab**. Tap a game to select it
(its case lifts), tap it again or tap Play to start it. Hold your finger on a game to open its card; Info does the same.
In portrait the shelves stack and scroll up and down with a flick; tap the Games tab again to go back to the top. In
landscape the shelf fills the page beside the selected game; drag along it. The card has Play, Favourite, Turn over
(Shelf and Cabinet) and Back. Settings rows change at one tap. Curator's filters have tick boxes and a Show games button.
Search: tap the field for the keyboard; Done or Android's Back closes it and shows the matching games.
With a gamepad, D-pad moves, A chooses, B goes back, X opens the box, Y stars, L/R change tabs, START opens Settings,
SELECT opens quick settings and R2 opens [your other apps](#your-other-apps). The key hints appear above the tabs as soon as you press a controller key and go away
when you touch the screen again.

The player has touch controls on a phone. Back, Pause or the controller's Menu opens Continue, Save, Load, Earlier,
Options, Controls, Zoom, Screenshot and Save & quit. With touch the menu is made of large buttons: one tap does it, and the arrows beside "Slot N"
choose one of eight manual slots; Options shows each setting with arrows to change it and scrolls by dragging. With a controller the
same page shows a selection ring (A chooses, B continues, L/R choose the slot).
GBA and SNES also have on-screen L/R shoulder buttons.
A game's first still picture stays visible until the game sends a newer picture.
Save & quit writes a separate auto state; the next start continues there. It closes the game only after that state is written, however long the phone takes, and a write is never half done. Two games with one file name in different folders keep their own states on the phone. Android Home suspends/saves the game. Back through Android's recent apps it waits paused; through the app icon (or Home,
when pocket-ui is your home screen) the library opens with the game first under Continue. Disc sheets and playlists keep one Continue entry for the whole game, including CUE/BIN and M3U sets; their tracks and discs stay together. Your saves and save states are private app data, and Android deletes them when the app is removed:
make a [save backup](#save-backup) first. ROM-folder permission does not expose another emulator's private saves.

If a moment cannot be resumed, or your in-game save is newer, the recovery message tells you where the moment was
kept under **Load**. After changing app versions, this keeps the moment that was just attempted and its own picture.

Type and buttons are sized for the phone's density (buttons at least 48 dp), in portrait and landscape, and stay clear
of the camera hole, rounded corners and the gesture bar.
The preview has emulator evidence, not a claim of tested Mini V2 or Vivo hardware, all emulator versions or final
audio quality. Extra in-app cores and hardware rendering are still being developed.

On a handheld every button of its pad works: L2/R2 (also as analog triggers) and the stick buttons L3/R3 reach games,
and R2 on a game card opens its Controls. A trigger resting part way in doesn't count as pressed. The Odin's "L2/R2 mode"
(analog, digital or both) makes no difference.

**A and B.** In menus the button Android calls A confirms and B goes back, whichever physical button the handheld's
"Controller style" makes of it. In a game the buttons keep their places like on the original console (the right-hand
button is the console's A), on the Odin in either style, and on Xbox- and Switch-style pads. Changing the Odin's
Controller style during a game pauses it and the pad stays player 1.

**A second controller.** A Bluetooth or USB pad plugged in or switched on is player 2 in a game that has a second player
port, the handheld's own pad stays player 1. In a game with only one player port every pad plays player 1. A pad that
drops out mid-game pauses the game; it gets its player back when it returns. Up to four players; menus take any pad.

**Home screen.** pocket-ui can be the home screen of an Android handheld: press Home and Android asks which app to use
(or Settings → Apps → Default apps → Home app). The app never sets this itself. B and Back never close it.

**Time and battery.** At the right end of the top line of every page (Games, Favourites, Search, Settings and the rest) the launcher shows the time and the battery, as the handhelds do. The time follows Android's own setting: 13:28 on a 24-hour clock, 1:28 PM on a 12-hour one. The battery reads "87%", and "87%, charging" on the charger. A device without a battery shows the time alone. It stays out while Android's own status bar is showing (nothing twice), and in a game. Where a very large font leaves no room, "charging" goes first, then the battery, then the time, so the page's name and its game count are never cut for it. It needs no permission.

### Your other apps

**Apps** at the top of the Games page (R2 on a controller) pulls out the drawer: the phone's other apps, such as the
Play Store, Settings and your Android games, as quiet cards by name. Tap one or press A to open it; when you come back,
the drawer is where you left it, and B or Back goes to the game you were on. To put an Android game on your shelves,
hold its card (or press Y): it says **On the shelf** and stands on its own **Android** shelf beside your consoles, in a
case with its name. Hold it again (Y) to take it off. Android marks some apps as games; their cards say **Looks like a
game**, but only you put a game on the shelf. A game on the Android shelf starts its app, and favourites, stickers and
play time work as for your other games. An app you uninstall leaves the shelf by itself.

On phones, Select and Start sit above the D-pad and face buttons in portrait, so their touch areas do not overlap.
The system Back gesture opens the pause menu; from core Options it returns to the pause menu first.

The pause menu follows the app's light/dark appearance and language. Its heading shows the game's whole name, in two or three lines when it is long, and never translated; a long status line under it ("Saved to slot 2", an error) wraps instead of being cut.

The box's empty-picture hint points to the Android pause menu. Screenshots are still stored privately; importing
them into the box's picture carousel and exporting them to the phone gallery remain preview work.

With a connected controller, the player starts without touch controls. Using a controller hides them; touch the game once to reveal them again. Back or controller Menu still opens the pause menu.

Optional downloads add Nintendo DS (melonDS DS) and, as **experimental** systems, PSP (PPSSPP), Nintendo 64
(Mupen64Plus-Next) and Dreamcast (Flycast) in our player. The system line before Play or Continue and the Controls page identify PSP,
N64 and Dreamcast (like the bundled Saturn) as experimental. Compatibility, full game speed, sound and saving/Continue are still being tested for them; do not rely on them
for your only copy of progress. DS is not marked experimental. PSP has four face buttons and L/R. On DS, both screens share the display;
the lower screen accepts the stylus. N64 has Z and C buttons; the touch arrows move its analog stick. A fresh N64 start temporarily buffers extra sound to bridge intro loading; it returns to the normal sound delay during silence or gradually during continuous music. Later loading stalls can still interrupt sound while N64 remains experimental.

Single-file games, including PSP ISOs and small cartridges, normally load directly from the folder you chose, without
using space for a second copy. Keep the card or folder available while playing. Some storage providers only offer streams,
and games with several files need their files together; these still use an app copy. For a required copy over 64 MiB,
a window shows progress and Cancel, and later starts reuse it. The app keeps up to five big copies and clears older ones
when it needs room; your saves stay. An older copy is removed when that game successfully starts directly.
PSP is still experimental: not every game runs at full speed yet.

PSP face controls use drawn cross/circle/square/triangle symbols, independent of the UI font. Short gamepad presses are held for at least one emulated frame so a quick tap is not lost between Android input events and rendering.

The 0.1.5 Android preview draws the current real game cases, discs and cartridges. Cabinet shows a TV beside the shelf in wide landscape and below it in portrait; the TV currently
shows cover/console art. X opens the case in Shelf/Cabinet or the shared info page in the other looks.
The info page and back of the box show your eight save slots on the system's save medium.
The pause menu has the same slim, coloured save row as the handheld version. Menus and opening a case
use the current sounds; a brief CRT power-on accompanies a new in-app game and is skipped by input.
With Cozy starts, the insertion sound follows the cartridge reaching the console; a misleading
audio-driver delay on a local fast output no longer brings the sound forward by half a second.
Connections (a developer, publisher, series or genre shared with other games of yours has an arrow; tap it for those
games) and Rediscover (Curator; fills after a year of play on the phone) work; This game is not yet available in this
Android preview. [RetroAchievements](#retroachievements) work in the games pocket-ui plays itself.

**Your games stand on shelves** in the Shelf and Cabinet looks: the games you played last on top, then your
Favourites, then a shelf for each system. Upright, swipe along a shelf to see more of it and up or down to move through them.
Sideways, one shelf fills the screen: swipe up or down, or use the two arrows at the top, to get to the next. On a controller Up and Down change the shelf.
With only one system and nothing played yet, your games stand in simple rows.

**Stickers:** on a game's card tap **Stickers** (or press SELECT): Completed, 100 %, Favourite, Playing now and Game of the
year with a year you choose go on its case. Completed and 100 % also earn the trophy and the framed "100 %" on your shelf.
Split shows no cases, so no stickers either; the sticker page says so.

Cozy extras have their switches in Settings. Printed discs print the cover on an open box's disc;
light by time of day and decorations apply to Shelf and Cabinet. The pause menu uses a slim row of save blocks.


First start has eight short setup steps: welcome, language, game folders, look,
covers and pictures, more systems, controls, and ready. Continue advances, Back returns, and Skip leaves a step
for later. If you close the app or return from the folder picker, setup resumes where you left it.
Settings → First start opens the eight-step guide again. You can finish without games or a network connection.

Choose English, German or Spanish, one of the four looks (Shelf, Easy, Split, Cabinet; Minimal or Cards from an
earlier build open as Split), Cozy or Quick animations, and dark or light pages.
Choose your ROM folder through Android's own picker and allow read access. Choosing another folder
adds it to your library; existing folder permissions stay in place. An empty folder is harmless:
choose another one or skip and add games later in Settings. Your ROM files stay in their folders.
You can select the folder above your system folders, or a system folder itself, such as `psp`, `ps1` or
`Genesis`. The selected folder's name supplies the system when a disc or zip cannot say it; a system
folder inside it takes precedence. If no games are found, the page explains which folder to choose; a folder
chosen later in Settings that brings no new games says so in a short message.

Covers and pictures asks whether to download as you browse. Download on Wi-Fi loads covers,
screenshots and available game descriptions for the selected game over Wi-Fi without a data limit.
Not now keeps downloads off; Settings → Covers and pictures changes the choice later. Already
cached pictures still work offline. This does not download every game in the background. Some game
descriptions are supplied in English by Wikipedia even when menus are in another language.

The Android app icon is the pixel cartridge with the paper label and lowercase p; Android can also use its monochrome themed icon.


In the player's Options, Picture has Scaling (Fit / aspect, Integer or Stretch / fullscreen), Screen
frame (None, Console or CRT TV), and Picture effect (Sharp, Smooth, CRT or Scanlines). Fit is the
phone default: landscape uses the available height with side bars; portrait places the picture at
the top, above the touch controls. Integer uses whole physical source scanlines while keeping the
core's display aspect. Stretch fills the available picture area and changes the aspect ratio.
Frames use a small border; they do not reduce the default frameless picture. Effects run in GLES
for software and hardware cores. Sharp uses nearest filtering; Smooth uses linear filtering;
CRT adds scanlines and edge shading, and Scanlines adds just the line pattern.
Save settings for selects This system or This game. A game override wins at the next launch;
selecting This system writes the system default and removes this game's override.

Version 0.1.5-android.6: a USB or Bluetooth controller works in the whole app, not only in games. Its D-pad and left
stick move through the shelves and menus; the key hints appear as soon as you use it. The pause menu is the same
large page with a controller (a ring shows where you are) and always fits on the screen, also in landscape. Options
are grouped into Picture, Sound, Controls and Speed; settings that belong to one emulator (its model, BIOS and
others that need a restart) are under "Advanced (this emulator)", closed until you open it.
The touch controls look like a pad: a round D-pad you can roll your thumb around (diagonals too), round buttons in
each system's own layout (PlayStation shapes on PSP, C buttons on N64, six buttons on Mega Drive), shoulder buttons
and small Select/Start pills. They are see-through, sit beside the picture in landscape and under it in portrait,
tick when pressed only if Settings → Haptics and your phone's touch vibration are both on, and hide while you play with a controller. Haptics defaults off.


Continue is here too: your last five games stand a little open and Continue goes straight back to the moment you left. See [Continue](continue.md#on-an-android-phone).

Version 0.1.6-android.8 adds the rest of the Cozy extras (see below). Version 0.1.6-android.7 brought the newer look of pocket-ui: real cases and cartridges, stickers, the TV on the Cabinet,
new colour sets and the new logo. Settings has new rows: Colours (five sets), Style (Barlow, Retro), Motion
(Full, Reduced, Off), Button symbols (letters, Xbox colours or PlayStation shapes in the controller hints) and Cozy
extras, where case sounds, the old-TV switch-on, the TV on the Cabinet, stickers and playful animations can each be
switched off. Settings that need hardware a phone does not have (LEDs, fan, sleep, Tailscale) are not shown.

Saturn (YabaSanshiro) and Dreamcast (Flycast) are also **experimental** in the app's own player,
with the same pause menu, Save, Load and Continue. Dreamcast touch arrows also move the analog stick. Saturn needs a BIOS from your own console:
put `saturn_bios.bin`, `sega_101.bin` or `mpr-17933.bin` in one of your chosen folders. Dreamcast
can start with its built-in replacement; the app says when no BIOS was found. For better compatibility,
add `dc_boot.bin` (or `dc_bios.bin`) and `dc_flash.bin` to a chosen `bios` folder. Run **Game check**
after adding files. No BIOS is supplied. The app reads the originals and keeps private copies.

Saturn discs can be CUE/BIN, ISO, CHD or M3U; Dreamcast discs can be GDI, CUE/BIN, CDI, CHD or M3U.
Keep a sheet with all the files it names in the same folder. The shelf uses the shared disc insertion
animation; no other emulator app is needed. Large discs need free internal storage for their copy.

## Game check

Settings → **Game check** asks the same question of the folders you chose. It reads your folders' lists through Android, nothing
is downloaded, deleted or changed, and saves are not touched. It answers **All good** with how many games are ready, or a short
list. Choosing a folder with new games offers the check, from Settings or from the Games page; **Not now**, B and Android's Back
leave it where you came from. Tap a row or use A; Android's Back or B goes back,
**Check again** (X) looks at everything again.

| Row | Example | What to do |
|---|---|---|
| **Folders that can't be read** | "Android no longer lets pocket-ui read Games."; "Games is not there any more." | **Choose it again**; your games are safe. A folder that is gone can be forgotten here, its files stay |
| **Discs missing a file** | "Disc: Game.cue is there, its Game.bin is missing." or "is empty" | Put the file next to it with that name. A file whose name differs only in capitals is found and needs no renaming |
| **Empty files**, **Zips that won't start** | As above | Copy the game into the folder again |
| **Same game twice** | `Game (1).gba` next to `Game.gba` | Both stay on the shelf. A phone can't hide a copy yet; delete it with a file manager if you want |
| **Wrong folder or file type** | A `.gba` in `snes`, `.sfcc`, `Game.sfc.part`; a folder whose name says no system, for a type that doesn't say either (`.iso`, `.chd`, `.zip`) | The line names the folder or the name it should have. A cartridge's type says its system, so `Sega Genesis/Sonic.md` is fine. Pick the folder above your system folders, or a recognised system folder itself |
| **Emulator apps to install** | "2 games need the ARMSX2 app, which isn't installed." | Install it from its official site or store. A game an installed emulator takes is no fault |
| **Can't start in this app yet** | A game over 512 MB on systems other than PSP, PlayStation, Saturn and Dreamcast; a big game requiring a copy when the phone hasn't room for it; PC Engine CD; PlayStation Vita | The game stays on the shelf |
| **Tips** | "No PlayStation BIOS found. Games start with the player's built-in stand-in, and a few need the real one."; "PlayStation 2 needs a BIOS file from your own console."; "PlayStation, Saturn and Dreamcast BIOS files are read from your bios folder." | Your choice. Tips don't take the green away |

Missing BIOS files appear as tips in Game check. Saturn needs one before it can start; PlayStation and Dreamcast can start with built-in replacements
([PlayStation](#playstation)); each emulator app keeps its own in a folder other apps can't read, so nobody here can know. Which core RetroArch has is out of sight in the same
way, so that is a tip too. A game's content is not read, so a damaged game with the right size and name is only found when it starts.

With a controller (a handheld's own buttons count) the app stays in landscape, either way up; without one it turns with
the phone.

## PlayStation

PlayStation games start in this app's own player with PCSX ReARMed, the core pocket-ui uses on handhelds.

- **What it reads:** `.cue` with its `.bin` tracks, `.chd`, `.pbp`, `.iso` and `.m3u` for a game on several discs, in a
  folder named `psx` (or `ps1`, `PlayStation`). A `.cue` with its tracks, or an `.m3u` with its discs, is one game on the
  shelf. A track whose name differs from the `.cue`'s only in capitals is found.
- **Storage:** a single `.pbp`, `.iso` or standalone `.chd` can load directly. CUE/BIN, playlists and CHD that need a parent file
  still use an app copy, keeping the
  disc's files and any parent together. A required copy over 64 MiB shows one progress bar and Cancel; later starts reuse it.
  All of a game's files must lie in the folder of its `.cue` or `.m3u`, or in folders below it.
- **BIOS:** not needed to start. Without one, games run with the player's built-in stand-in; a few games need the real one.
  Put a BIOS from your own console, such as `scph5501.bin`, in a folder named `bios` among the folders you chose. The app
  copies it into its own storage at the next PlayStation start. When none is found, the first PlayStation start says so
  once while the app runs, and the Game check lists it as a tip. No BIOS is supplied.
- **Several discs:** an `.m3u` starts with its first disc. When the game asks for the next one, open the pause menu,
  **Options**, and change **Disc** (for example "Disc 2 of 2").

## Games in other emulator apps

PlayStation 2, GameCube and Wii, 3DS and Xbox games are on the shelf like the others and start in their emulator app,
straight into the game: no second menu in between. Close the game in the app's menu and you are back on the shelf.
Continue shows only games of this app's own player, which keeps their moment.

| System | Folder and files | App | Where to get it | Once in the app |
|---|---|---|---|---|
| PlayStation 2 | `ps2`: `.iso`, `.chd`, `.cso` | ARMSX2 (NetherSX2 also works) | github.com/ARMSX2/ARMSX2, Releases | Add your console's BIOS |
| GameCube and Wii | `gamecube`, `wii`: `.rvz`, `.iso`, `.gcm` | Dolphin | F-Droid, Google Play or dolphin-emu.org | Nothing |
| 3DS | `3ds`: `.3ds`, `.cci`, `.cxi`, `.3dsx` | Azahar | azahar-emu.org or Google Play | Its first-start steps (a folder for its data) |
| Xbox | `xbox`: `.iso` (also `.xiso.iso`) | hakuX | github.com/rfandango/hakuX, Releases | Choose your Xbox games folder |

**Leaving a game.** Close it in the app's own menu and you are back in pocket-ui: ARMSX2 Back → **Close**, Dolphin Back →
**Exit Emulation**, hakuX Back → **Exit**, and Azahar Back → scroll the side menu to **Close Game** → **OK**. Azahar's game
starts in a fresh Azahar task, so it doesn't matter that you opened Azahar's own screens before; without that, Close Game
used to end on Azahar's game list and pocket-ui stayed out of sight. Settings → **Help** has the same list.

When the app isn't installed, Play says which app it is and where to get it, and nothing is started or counted as
played. Saves and save states stay in the emulator app; the time you were away counts as played.

## Plays with

Every system with pocket-ui's own player starts in it. Settings → **Plays with** lets you change that per system: the
page lists each of your systems that has an emulator app on the phone that pocket-ui knows (RetroArch, PPSSPP,
DuckStation), and A steps through **pocket-ui** and the installed apps. Only installed apps are offered, so the page is
empty until you install one. Nintendo 64, PSP, Saturn and Dreamcast carry a small "Experimental" next to
**pocket-ui**: the own player for them is still being tested. A game of a system set to an app starts there, comes back
to pocket-ui when you leave it, and counts play time and the shelf as for the apps above. Continue and the pause menu
belong to that app then; set the system back to **pocket-ui** and Continue is there again. RetroArch needs the system's
core installed in RetroArch itself. An app you remove falls back to pocket-ui by itself. Nothing is chosen for you.

## Save backup

Settings → **Save backup** has three rows.

- **Export saves** writes every game's saves and save states (the moment you quit at, your slots and their pictures) into
  one file, `pocket-ui-saves-<date>.zip`. Android asks where it goes: any folder, a card or a cloud folder. The row
  shows how many games have something to keep.
- **Import saves** asks for such a file and brings its saves back. A game without saves on this phone gets the backup's.
  When some games already have saves here, the page asks first: **Keep this phone's saves** only adds the games that have
  none, **Use the backup's saves** replaces those games' saves with the backup's. Nothing is replaced without that answer.

- **Saved copies** shows whether the copies below are working (On, Off, Folder missing). With A it copies now; when it is
  Off or the folder is missing, A opens the folder choice again.

After installing the app again, choose the same game folder, then import: every game finds its saves. On another phone or
card a game is found again by its console and file name; a game that isn't on the shelf yet keeps its saves waiting until
its folder is chosen. A file that is no pocket-ui backup is refused and nothing changes. Settings, favourites and stickers
are not part of a backup, and neither is the list of what you played last: after an import each game continues where it
was when you start it, and the Continue shelf fills again as you do.

### Saved copies in your game folder

You don't have to export first. When a game ends (**Save & quit**, or the game closing), pocket-ui copies that game's
saves and save states into a folder called `pocket-ui saves` inside the game folder you chose, one folder per game, with
a short `README.txt`. The copy runs in the background after the game is over, never while you play, and Save & quit
doesn't wait for it. Android doesn't delete that folder when the app is removed.

After installing the app again, choose the same game folder: pocket-ui finds `pocket-ui saves` and brings back the saves of
every game that has none yet, and says so. A game that already has saves on the phone is never overwritten: files that
are the same are left alone, and when the phone and the copy differ (for example you played a bit before choosing the
folder), nothing is replaced, the page names how many games differ, and both versions stay as they are. A newer save never
loses to an older one: a file is only written over another when it is different and was written later.

pocket-ui needs permission to write into your game folder for this. The folder choice asks for it; if you chose your folder
with an older version (read only), or Android refuses, Settings → Save backup → **Saved copies** says **Off** and A
leads to the folder choice. If the card is out or the folder was removed it says **Folder missing**, and after a failed copy a
short message appears; it is tried again when the next game ends. Don't rename or edit the files in `pocket-ui saves`. The
copies hold your saves and save states, not settings, favourites or the Continue shelf.

## Help

Settings → **Help** has short answers to what players ask most: choosing your games folder, loading more systems, the pause
menu, saving and carrying on, the copy of your saves, **Plays with**, leaving a game in another emulator app, and reporting a
problem. Choose a topic with A and its answer appears above the list; B goes back.

## RetroAchievements

[RetroAchievements](https://retroachievements.org) counts the achievements you unlock in classic games, with a free
account. In pocket-ui's own player they work as on the handhelds ([Achievements](achievements.md)); a game that another
emulator app plays has them from that emulator, and pocket-ui does nothing there.

**Sign in once:** Settings → **RetroAchievements** → **Sign in**. Type your user name and press START (or Done), then
your password: it shows as dots, the last character readable. The password is sent once, over HTTPS, and is not kept:
the site answers with a sign-in key and only that key is saved on the device, in pocket-ui's private files, readable by
the app alone. A wrong name or password says so in the site's words, and the next try has your name filled in.
**Log out** is the same row (press it twice: the first press asks) and deletes the key; what you unlocked stays on the site.

**Without signing in nothing happens:** no network request, no list, no extra row. The app asks the site only while you
are signed in.

**While you play:** the game starts as fast as without an account. The game is identified from its file, also when
it was opened straight from your folder without a copy, and from a zip, an ISO, a CUE sheet or an M3U playlist; a game
the site doesn't know gets no note. A note over the game says how many you have ("Achievements: 12 of 40"). An unlock
brings a small card at the top for four seconds: its picture, "Achievement unlocked" with its points, and its title,
one card after the other. Save states, Earlier and cheats stay available: this is the *softcore* mode (no hardcore, as
on the handhelds), and a loaded moment starts watching again from that point, so loading can't unlock anything.

**The list:** with an account, the pause menu has an **Achievements** row, last, which lists the game's achievements:
the locked ones first with your progress where the game counts it, then the unlocked ones, each with its description.
B (or the Back button) goes back; a controller scrolls with Up and Down, a finger by dragging.

**Without a network:** a game you played online before keeps its achievements, and you can unlock them offline. Unlocks
the site hasn't confirmed are kept and sent, with the time you earned them, the next time a game starts with a network
(the list says how many wait).

**A golden sticker:** when every achievement of a game is unlocked, the "100 %" sticker comes onto its case by itself,
once. It counts for the trophy on the shelf like a finished game does (Cozy extras → Decorations); take it off from the
card's Stickers whenever you like and it stays off.

**Limits:** a disc in the **CHD** format can't be identified (pocket-ui has no CHD reader; the pause menu says so);
a CUE/BIN, ISO or M3U copy of the same disc can. There is no unlock sound on Android yet.

## Report a problem

Settings → **Report a problem** (the last row) puts what helps find a problem into one text: pocket-ui's version and
build, your device (model, Android version, screen and density, text size, game controllers by their USB ids), the look
and language, your games per system and the core that plays each, the sound counters of the player's last audio line,
and the end of the logs of the app and of the player. It has three rows:

- **Share the report** opens Android's share sheet with the text: send it to yourself, to a chat or to a mail app.
- **Copy the report** puts the text on the clipboard, to paste into a new issue on pocket-ui's GitHub page
  (Issues → New issue → Bug report). Say what happened and, if it matters, which game.
- **Back**.

Nothing is sent by the app, and no file is left on the phone. The text is at most 60,000 characters, so it fits an
issue; when the log is longer, the newest lines stay. Where something is missing the report says so ("No game has
played sound yet"). **Left out:** network and e-mail addresses, passwords and tokens, user names in paths, the folder
names inside your phone's storage and the addresses of folders you picked, and your games' names (each becomes a system
and a short code such as `[game gba 1a2b3c4d]`, the same for the same game; name the game in your report if it matters).
The phone's own name and the controllers' names are never read.

## Easy and Just Play

Settings → Look (also in first start) offers Easy: one large game with previous and
next buttons. Tap Play or Continue to start it; tap its name for Info. On a controller,
arrows change games, A starts and X opens Info. Games and Quick settings offer Just
Play: it chooses a game and starts it straight away. Automatic choices use the app's
bundled player; choose games for other emulator apps yourself. On a controller, X on Quick
settings' Just Play row shows the chosen game's card first, with a line saying why; Right
on that card brings another.

The pause menu's Save & quit saves your in-game data and a moment before returning
to the library. If saving fails, the menu stays open with the error. The picture
choices remain Sharp, Smooth, CRT and Scanlines. Shorter older PSP moments may be
kept rather than resumed because the bundled emulator cannot safely check their length.

The Games shelf now calls its first row Continue and includes only games whose open
moment was verified by the Android worker. Recently played external games without
an internal moment remain on their system shelf. Easy keeps its single-game order.

## Android 0.2

[Controls](controls-card.md#on-android) shows where the console's buttons are on your gamepad
or touch pad, from a game's card or the pause sheet. [Zoom and Hold to toggle](controls.md#on-android) help
with small dialogue and keeping one game button pressed. Android's font scale and the app's
Text size use the larger choice, up to 200% system text. After a long break, [Where was I?](continue.md#on-an-android-phone)
can hold a resumed picture briefly for reading; Settings → Remind me where I was turns it off.
Back returns one page, and only Save & quit leaves gameplay. A controller disconnect pauses
play and brings back the touch pad. Battery saver pauses optional media downloads and the startup animation; it leaves the game speed and saved preferences alone.
A refused old PSP moment opens Recovery with a clear explanation and keeps the original
under Load. The in-game save remains safe.


Settings → **Style → Retro** gives menus pixel type and frames. **Colours** then chooses Cocoa or Pea; Appearance
still chooses dark/light. Long selected names have a three-line title/subtitle band beside or below their cover.
Folders from another card, such as `Game Boy Advance (GBA)` or `Sega Genesis`, work without renaming. Box art already
inside a chosen folder is preferred to downloaded pictures, and nothing is written to the card.

For mGBA, Gambatte, bsnes and Nestopia, a `.cht` file with the game's exact filename stem next to the ROM adds
**Cheats** to pause → Options. Codes start Off and are remembered for that game and core. Unsupported memory-write
cheats are explained. External apps manage their own cheats. This preview includes no cheat database.

A failed Save & quit keeps the player open and says why. A Load that cannot keep Before load leaves the game
unchanged. If both a Load and its recovery fail, play and saving stop: load another moment, or choose **Return to
library** to close that session safely. Earlier saves are kept. **Where was I?** now waits for a press; that press
only dismisses the reminder.

Settings → **Updates** downloads a newer, matching signed APK over Wi-Fi and asks Android to install it.
Games and settings stay. Do not uninstall to update: uninstalling removes private saves (Settings → Save backup keeps them in a file). The preview does not use
the SD-card updater and does not announce Linux releases as Android updates.

Game and system names keep their original spelling in every language, including phone rows, title bands, box headers and the pause header. Buttons and instructions follow the selected language.

Box headers leave room between the game name and its system at large Android font sizes.

The in-app player requests Android’s low-latency callback audio path and adjusts the sample rate gently to keep its short queue stable. Output latency and Bluetooth behaviour depend on the device; speaker timing still needs a listening check.

## More systems

The APK includes the small systems through PlayStation and Saturn. **More systems?** in first start offers PSP, DS,
N64 and Dreamcast with each download's size. Nothing starts selected. Choose the systems you want and
Continue; downloads wait for unmetered Wi-Fi. Skip leaves every system undownloaded.
Settings → **Cozy extras** offers the same systems later. Download now shows the size; B cancels a running download.
Choosing it again resumes the saved bytes when the server supports it. A lost Wi-Fi connection preserves them too.
After a download is checked and installed, its row says Installed and the game can start normally.
Starting a game for a missing system opens Extras with that system selected and an explanation.
PPSSPP's system files come with its download. No games or console BIOS are included.
Until the download channel is public, these systems are listed as **Coming soon**: first start and Extras show them
without a size or a choice, and starting a game for one says that the system is coming soon, with no download and no error.
Once the channel is public the same pages offer the downloads.

## Licences

**Settings → Licences** lists pocket-ui's own licence (GPL version 2 or later) and every emulator in the app, the optional
downloads too. Choose one to see its licence, where its source code is (upstream and exact commit) and the patches applied
to it. The complete source of each emulator comes next to the APK on its release page; you can also ask for it any time
(open an issue titled "Source request"). The licence texts are in the app's package as well.

## Updates over Wi-Fi

Open **Settings → Updates**. It shows the installed version, a newer pocket-ui when one is available,
**Check again**, and **Check for updates automatically**, using the same signed channel rules as Linux.
The Android channel is separate. If it has not been published yet, the page says so.

Choose **Install pocket-ui …**, then **Install now**. The download needs Wi-Fi, shows its progress,
and **B** or the on-screen **Cancel** stops it. If Wi-Fi disappears, the update stops; it never uses
mobile data. A changed download or a different APK signing key produces an explanation and leaves
the app as it was.

The first time, Android asks you to allow this app to install updates. Enable that permission and
return to pocket-ui. Confirm Android's update dialog. Later updates retain that permission, but
Android still asks you to confirm the installation. pocket-ui closes afterwards; open it again. The first
opening confirms the installed version. Games, app settings,
ROM-folder access and saves stay in place. Android's Cancel button cancels its confirmation dialog.
Automatic checks run while the launcher is idle on Wi-Fi; they never install an update by themselves.
