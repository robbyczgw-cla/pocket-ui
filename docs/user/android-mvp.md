# Android preview

This APK is an early Android version of pocket-ui. Pick ROM folders through Android's folder picker; recognised
system folders can contain subfolders. The app remembers access and the selected game. It reads M3U/CUE disc sets
as one entry. GB/GBC/GBA run in our own player with mGBA, NES with Nestopia, SNES with bsnes, Master System/Game Gear with Gearsystem, Mega Drive cartridges with ClownMDEmu, WonderSwan with Beetle WonderSwan, Atari 2600 with Stella, PC Engine with Beetle PCE Fast, Neo Geo Pocket/Color with Beetle NGP and Atari Lynx with Handy. Settings can select Gambatte for GB/GBC; its saves and states are separate from mGBA. Larger systems and disc sets still need an installed emulator.
No games, BIOS or keys are supplied. Install and configure external emulators separately.

On a phone the tabs are at the bottom: Games, Favourites, Search, Curator and Settings. Tap a game to select it
(its case lifts), tap it again or tap Play to start it. Hold your finger on a game to open its card; Info does the same.
In portrait the shelves stack and scroll up and down with a flick; tap the Games tab again to go back to the top. In
landscape the shelf fills the page beside the selected game; drag along it. The card has Play, Favourite, Turn over
(Shelf and Cabinet) and Back. Settings rows change at one tap. Curator's filters have tick boxes and a Show games button.
Search: tap the field for the keyboard; Done or Android's Back closes it and shows the matching games.
With a gamepad, D-pad moves, A chooses, B goes back, X opens the box, Y stars, L/R change tabs, START opens Settings
and SELECT opens quick settings. The key hints appear above the tabs as soon as you press a controller key and go away
when you touch the screen again.

The player has touch controls on a phone. Back, Pause or the controller's Menu opens Continue, Save, Load, Earlier,
Options, Controls, Zoom, Screenshot and Save & quit. With touch the menu is made of large buttons: one tap does it, and the arrows beside "Slot N"
choose one of eight manual slots; Options shows each setting with arrows to change it and scrolls by dragging. With a controller the
same page shows a selection ring (A chooses, B continues, L/R choose the slot).
GBA and SNES also have on-screen L/R shoulder buttons.
Save & quit writes a separate auto state; the next start continues there. It closes the game only after that state is written, however long the phone takes, and a write is never half done. Two games with one file name in different folders keep their own states on the phone. Android Home suspends/saves and returns to a
paused game. Keep the app installed: its saves, states and screenshots are private app data and uninstalling deletes
them. Export/backup UI is still open work. ROM-folder permission does not expose another emulator's private saves.

If a moment cannot be resumed, or your in-game save is newer, the recovery message tells you where the moment was
kept under **Load**. After changing app versions, this keeps the moment that was just attempted and its own picture.

Type and buttons are sized for the phone's density (buttons at least 48 dp), in portrait and landscape, and stay clear
of the camera hole, rounded corners and the gesture bar.
The preview has emulator evidence, not a claim of tested Mini V2 or Vivo hardware, all emulator versions or final
audio quality. Extra in-app cores and hardware rendering are still being developed.

On phones, Select and Start sit above the D-pad and face buttons in portrait, so their touch areas do not overlap.
The system Back gesture opens the pause menu; from core Options it returns to the pause menu first.

The pause menu follows the app's light/dark appearance and language. Its heading shows the game's whole name, in two or three lines when it is long, and never translated; a long status line under it ("Saved to slot 2", an error) wraps instead of being cut.

The box's empty-picture hint points to the Android pause menu. Screenshots are still stored privately; importing
them into the box's picture carousel and exporting them to the phone gallery remain preview work.

With a connected controller, the player starts without touch controls. Using a controller hides them; touch the game once to reveal them again. Back or controller Menu still opens the pause menu.

The current internal test build also starts PSP with PPSSPP, Nintendo DS with melonDS DS and Nintendo 64
with Mupen64Plus-Next in our player. PSP has four face buttons and L/R. On DS, tap the game picture for
touchscreen input. N64 has Z and C buttons; the touch arrows move its analog stick. These three systems
have had quick emulator starts only. N64's first fixture is a Hello World demo; full game compatibility,
A big game, such as a PSP ISO, is copied into the app on its first start. A window shows how far it is and lets you cancel; later starts don't wait, and your phone needs room for the copy. The app keeps the last five big games ready and clears older copies when it needs the room; your saves stay. PSP support files and audio on a physical Android handheld still need checking. Large-core downloads
are not enabled yet.

PSP face controls use drawn cross/circle/square/triangle symbols, independent of the UI font. Short gamepad presses are held for at least one emulated frame so a quick tap is not lost between Android input events and rendering.

The 0.1.5 Android preview draws the current real game cases, discs and cartridges. Cabinet shows a TV beside the shelf in wide landscape and below it in portrait; the TV currently
shows cover/console art. X opens the case in Shelf/Cabinet or the shared info page in the other looks.
The info page and back of the box show your eight save slots on the system's save medium.
The pause menu has the same slim, coloured save row as the handheld version. Menus and opening a case
use the current sounds; a brief CRT power-on accompanies a new in-app game and is skipped by input.
Connections (a developer, publisher, series or genre shared with other games of yours has an arrow; tap it for those
games) and Rediscover (Curator; fills after a year of play on the phone) work; This game and RetroAchievements
are not yet available in this Android preview.

**Your games stand on shelves** in the Shelf, Cabinet and Cards looks: the games you played last on top, then your
Favourites, then a shelf for each system. Upright, swipe along a shelf to see more of it and up or down to move through them.
Sideways, one shelf fills the screen: swipe up or down, or use the two arrows at the top, to get to the next. On a controller Up and Down change the shelf.
With only one system and nothing played yet, your games stand in simple rows.

**Stickers:** on a game's card tap **Stickers** (or press SELECT): Completed, 100 %, Favourite, Playing now and Game of the
year with a year you choose go on its case. Completed and 100 % also earn the trophy and the framed "100 %" on your shelf.
Minimal and Split show no cases, so no stickers either; the sticker page says so.

Every Cozy extra of the handheld version has its switch: the eight you see on Settings → Cozy extras, and Playful
cases, Old-TV switch-on, Case sounds and Slideshow before screen off as rows of Settings itself. The memory card in the pause menu shows the whole card
or cartridge with its eight slots; printed discs print the cover on the disc in an open box; light by time of day tints the
Shelf and Cabinet looks with the hour; decorations put a plant (and a figurine for each system you play a lot) on a shelf
at the end of your games, growing with the time you play on this phone; the idle slideshow (off until you switch it on)
shows your covers, pictures and saves after a quiet minute on Games or Favourites, keeps the screen on while it can run
(at most about eleven minutes after your last touch), and ends with any touch.


Version 0.1.5-android.2 starts with seven short setup steps: welcome, language, game folders, look,
covers and pictures, controls, and ready. Continue advances, Back returns, and Skip leaves a step
for later. If you close the app or return from the folder picker, setup resumes where you left it.
Settings → First start opens the guide again. You can finish without games or a network connection.

Choose English, German or Spanish, one of the looks, Cozy or Quick animations, and dark or light pages.
Choose your ROM folder through Android's own picker and allow read access. Choosing another folder
adds it to your library; existing folder permissions stay in place. An empty folder is harmless:
choose another one or skip and add games later in Settings. Your ROM files stay in their folders.

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
new colour sets and the new logo. Settings has new rows: Colours (five sets), Font (Barlow, Pixel, Rounded), Motion
(Full, Reduced, Off), Button symbols (letters, Xbox colours or PlayStation shapes in the controller hints) and Cozy
extras, where case sounds, the old-TV switch-on, the TV on the Cabinet, stickers and playful animations can each be
switched off. Settings that need hardware a phone does not have (LEDs, fan, sleep, Tailscale) are not shown.

## Game check

Settings → **Game check** asks the same question of the folders you chose. It reads your folders' lists through Android, nothing
is downloaded, deleted or changed, and saves are not touched. It answers **All good** with how many games are ready, or a short
list. Choosing a folder from Settings with new games offers the check. Tap a row or use A; Android's Back or B goes back,
**Check again** (X) looks at everything again.

| Row | Example | What to do |
|---|---|---|
| **Folders that can't be read** | "Android no longer lets pocket-ui read Games."; "Games is not there any more." | **Choose it again**; your games are safe. A folder that is gone can be forgotten here, its files stay |
| **Discs missing a file** | "Disc: Game.cue is there, its Game.bin is missing." or "is empty" | Put the file next to it with that name; the line says when only capitals differ |
| **Empty files**, **Zips that won't start** | As above | Copy the game into the folder again |
| **Same game twice** | `Game (1).gba` next to `Game.gba` | Both stay on the shelf. A phone can't hide a copy yet; delete it with a file manager if you want |
| **Wrong folder or file type** | A `.gba` in `snes`, `.sfcc`, `Game.sfc.part`; a folder whose name says no system, for a type that doesn't say either (`.iso`, `.chd`, `.zip`) | The line names the folder or the name it should have. A cartridge's type says its system, so `Sega Genesis/Sonic.md` is fine. Pick the folder above `psx`, not `psx` itself |
| **Emulator apps to install** | "2 games need the DuckStation app, which isn't installed." | Install it from its official site or store. A game an installed emulator takes is no fault |
| **Can't start in this app yet** | A game over 512 MB on systems other than PSP; a big game when the phone hasn't the room for its first start; PC Engine CD; PlayStation Vita | The game stays on the shelf |
| **Tips** | "PlayStation needs a BIOS file from your own console."; "The files in your bios folder aren't read." | Your choice. Tips don't take the green away |

A BIOS is never called missing on Android, because nobody here can know: this app's own player has no system that asks you for
one, and each emulator app keeps its own in a folder other apps can't read. Which core RetroArch has is out of sight in the same
way, so that is a tip too. A game's content is not read, so a damaged game with the right size and name is only found when it starts.

## Easy and Just Play

Settings → Look (also in first start) offers Easy: one large game with previous and
next buttons. Tap Play or Continue to start it; tap its name for Info. On a controller,
arrows change games, A starts and X opens Info. Games and Quick settings offer Just
Play: it chooses a game and starts it straight away. Automatic choices use the app's
bundled player; choose games for other emulator apps yourself.

The pause menu's Save & quit saves your in-game data and a moment before returning
to the library. If saving fails, the menu stays open with the error. The picture
choices remain Sharp, Smooth, CRT and Scanlines. Shorter older PSP moments may be
kept rather than resumed because the bundled emulator cannot safely check their length.

The Games shelf now calls its first row Continue and includes only games whose open
moment was verified by the Android worker. Recently played external games without
an internal moment remain on their system shelf. Easy keeps its single-game order.

## Android 0.2

[Controls](controls-card.md#on-android) explains Android gamepad and touch-pad buttons from
a game's card or the pause sheet. [Zoom and Hold to toggle](controls.md#on-android) help
with small dialogue and keeping one game button pressed. Android's font scale and the app's
Text size use the larger choice, up to 200% system text. After a long break, [Where was I?](continue.md#on-an-android-phone)
can hold a resumed picture briefly for reading; Settings → Remind me where I was turns it off.
Back returns one page, and only Save & quit leaves gameplay. A controller disconnect pauses
play and brings back the touch pad. Battery saver pauses optional media downloads, idle
slideshow and the startup animation; it leaves the game speed and saved preferences alone.
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

Settings → **Updates** explains Android updates: install a newer, matching signed APK over the installed app.
Games and settings stay. Do not uninstall to update: uninstalling removes private saves. The preview does not use
the SD-card updater and does not announce Linux releases as Android updates.

Game and system names keep their original spelling in every language, including phone rows, title bands, box headers and the pause header. Buttons and instructions follow the selected language.

Box headers leave room between the game name and its system at large Android font sizes.
