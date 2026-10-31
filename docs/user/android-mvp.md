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
Options, Screenshot and Quit. With touch the menu is made of large buttons: one tap does it, and the arrows beside "Slot N"
choose one of eight manual slots; Options shows each setting with arrows to change it and scrolls by dragging. With a controller the
menu is a list (A chooses, B continues, L/R choose the slot).
GBA and SNES also have on-screen L/R shoulder buttons.
Quit writes a separate auto state; the next start continues there. Android Home suspends/saves and returns to a
paused game. Keep the app installed: its saves, states and screenshots are private app data and uninstalling deletes
them. Export/backup UI is still open work. ROM-folder permission does not expose another emulator's private saves.

Type and buttons are sized for the phone's density (buttons at least 48 dp), in portrait and landscape, and stay clear
of the camera hole, rounded corners and the gesture bar.
The preview has emulator evidence, not a claim of tested Mini V2 or Vivo hardware, all emulator versions or final
audio quality. Extra in-app cores and hardware rendering are still being developed.

On phones, Select and Start sit above the D-pad and face buttons in portrait, so their touch areas do not overlap.
The system Back gesture opens the pause menu; from core Options it returns to the pause menu first.

The pause menu follows the app's light/dark appearance and language.

The box's empty-picture hint points to the Android pause menu. Screenshots are still stored privately; importing
them into the box's picture carousel and exporting them to the phone gallery remain preview work.

With a connected controller, the player starts without touch controls. Using a controller hides them; touch the game once to reveal them again. Back or controller Menu still opens the pause menu.

The current internal test build also starts PSP with PPSSPP, Nintendo DS with melonDS DS and Nintendo 64
with Mupen64Plus-Next in our player. PSP has four face buttons and L/R. On DS, tap the game picture for
touchscreen input. N64 has Z and C buttons; the touch arrows move its analog stick. These three systems
have had quick emulator starts only. N64's first fixture is a Hello World demo; full game compatibility,
PSP support files and audio on a physical Android handheld still need checking. Large-core downloads
are not enabled yet.

PSP face controls use drawn cross/circle/square/triangle symbols, independent of the UI font. Short gamepad presses are held for at least one emulated frame so a quick tap is not lost between Android input events and rendering.

The 0.1.5 Android preview draws the current real game cases, discs and cartridges. Favourite games have
a sticker. Cabinet shows a TV beside the shelf in wide landscape and below it in portrait; the TV currently
shows cover/console art. X opens the case in Shelf/Cabinet or the shared info page in the other looks.
The info page and back of the box show your eight save slots on the system's save medium.
The pause menu has the same slim, coloured save row as the handheld version. Menus and opening a case
use the current sounds; a brief CRT power-on accompanies a new in-app game and is skipped by input.
Connections (a developer, publisher, series or genre shared with other games of yours has an arrow; tap it for those
games) and Rediscover (Curator; fills after a year of play on the phone) work; editable case stickers and This game/Extras
are not yet available in this Android preview.

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

Choose English or German, one of the five looks, Cozy or Quick animations, and dark or light pages.
Choose your ROM folder through Android's own picker and allow read access. Choosing another folder
adds it to your library; existing folder permissions stay in place. An empty folder is harmless:
choose another one or skip and add games later in Settings. Your ROM files stay in their folders.

Covers and pictures asks whether to download as you browse. Download on Wi-Fi loads covers,
screenshots and available game descriptions for the selected game over Wi-Fi without a data limit.
Not now keeps downloads off; Settings → Covers and pictures changes the choice later. Already
cached pictures still work offline. This does not download every game in the background. Some game
descriptions are supplied in English by Wikipedia even when menus are German.

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
tick when pressed if your phone's touch vibration is on, and hide while you play with a controller.


Version 0.1.6-android.8 adds the rest of the Cozy extras (see below). Version 0.1.6-android.7 brought the newer look of pocket-ui: real cases and cartridges, stickers, the TV on the Cabinet,
new colour sets and the new logo. Settings has new rows: Colours (five sets), Font (Barlow, Pixel, Rounded), Motion
(Full, Reduced, Off), Button symbols (letters, Xbox colours or PlayStation shapes in the controller hints) and Cozy
extras, where case sounds, the old-TV switch-on, the TV on the Cabinet, stickers and playful animations can each be
switched off. Settings that need hardware a phone does not have (LEDs, fan, sleep, Tailscale) are not shown.
