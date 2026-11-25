<!-- Product name: "pocket-ui" is a working name. When the name is decided, replace it in every
     file under docs/user/. It is used only as a plain name, so a search-and-replace is enough.
     Keep labels that the UI itself shows (for example the "pocket-ui" row on the Updates page)
     in step with the code. -->

# pocket-ui

pocket-ui OS is a cozy handheld operating system and game player. On most handhelds it is built on ROCKNIX; on the
TrimUI Brick it runs from a card of its own. It replaces the usual menu with a shelf of your games in their real boxes. Everything works with buttons, and touch screens work on the shelf and in lists
too.

You can read this guide right on your handheld under Settings → Help & tools → **Manual**. The manual is in English,
while the menus support English, German, Spanish, Brazilian Portuguese, French, Italian and Korean
([Settings → System → Language](language.md)).

pocket-ui is in active alpha and tested daily on the Retroid Pocket Mini V2 alongside several other handhelds. These
pages cover what works today and clearly flag anything that is still experimental or untested on real hardware.

## What you need

- A supported handheld, such as a TrimUI Brick, Retroid Pocket Mini V2, Anbernic RG35XX or R36S. The
  [install page](https://pocketui.xyz/install.html) lists them all.
- A microSD card (8 GB or larger; 16 GB+ on the RG28XX and RG35XX 2024) for pocket-ui OS and your games.
  <!-- TODO: recommended SD card size. The test card grew its game partition to about 60 GB. -->
- Five minutes at a computer to prepare the ready-made card: flash the image with balenaEtcher or Raspberry Pi Imager,
  or on the TrimUI Brick unzip the card download onto an empty card. Grab the card for your handheld on the
  [install page](https://pocketui.xyz/install.html), or install pocket-ui onto an existing ROCKNIX card
  ([Getting started](getting-started.md)).
- Your own ROMs and BIOS files. pocket-ui ships with no games.
- Wi-Fi is optional. You need it for box art, descriptions, PortMaster, RetroAchievements, online updates and wireless
  file transfers. Without Wi-Fi, you can update from a zip on the card
  ([Updates](updates.md#no-wi-fi-update-from-the-card)).

## What it does

- Puts your games on one shelf: Continue first, then recent games, then each console.
- Launches every console with the right emulator and brings you right back when you quit.
- Picks up right where you left off on pocket-ui's own player, RetroArch, PS2, GameCube, Wii and 3DS.
- Searches as you type and jumps letter by letter with L2 and R2.
- Opens one pause menu with HOME: Continue, Save and Load (eight visual slots on its own player plus PS2, GameCube and
  Wii), Options, quick settings and Quit (with a Settings toggle for saving on quit).
- On its own player: live-preview picture filters, screen frames, core options, button remapping, fast-forward, rewind,
  run-ahead, cheats and shortcuts.
- Switches between recent games with SELECT, almost as fast as launching a single game.
- Organises big libraries with Curator: filters, smart shelves, "more like this" and multi-system versions
  ([Your library](curator.md#curator)).
- Sets up Wi-Fi (including saved/hidden networks and Wi-Fi country), box art and PortMaster right on the handheld with a
  gamepad keyboard.
- Launches ROCKNIX tools directly without leaving pocket-ui: emulator settings, RetroArch, gamepad test/calibration, a
  file manager and a terminal ([Tools](extras.md#tools)).
- Backs up settings, saves and library notes to the SD card, with easy restore and reset
  ([Backup & reset](settings-system.md#backup--reset)).
- Gives you quick settings (brightness, volume, Wi-Fi, Bluetooth, performance, fan, power) on HOME, gentle menu sounds,
  battery stats and this built-in manual.
- Keeps saves and save states synced across your handhelds with Pocket Sync or Syncthing ([Sync](sync.md)).
- Pairs Bluetooth controllers and headphones in one press, remembering per-controller button layouts
  ([Bluetooth](bluetooth.md), [Controller setup](controls-settings.md#controller-setup),
  [Test buttons](settings-sound.md#test-buttons)).
- Switches every menu to classic pixel art in one toggle: [Retro style](theme.md#style) in Cocoa or Pea.
- Walks you through first boot in five quick steps: language, Wi-Fi, games, layout and coziness
  ([First start](getting-started.md#3-first-start)). Re-run it any time under Settings → Help & tools →
  First-start setup.
- Connects to your Tailscale network with a QR code so you can reach your Pocket from anywhere ([Tailscale](remote.md)).
- Opens your full library in any browser over home Wi-Fi or Tailscale to browse, edit notes, upload games/BIOS/PortMaster
  files and check battery or screenshots ([Web companion](web-companion.md)). Off until you switch it on.
- Tracks RetroAchievements on its own player with unlock pop-ups, a pause-menu checklist and progress on game boxes
  ([Achievements](achievements.md)).
- Streams PC games over Wi-Fi via Moonlight, paired right on screen with a PIN ([PC streaming](pc-streaming.md)).
- Updates safely from signed releases, automatically rolling back if a new build fails to boot.

## What it does not touch

- **Your stock system stays separate.** pocket-ui OS lives on its own microSD card. On a TrimUI Brick or one of the
  supported Anbernic models, put your original card back in to return to stock. On Retroid and AYN, choose Android
  in the boot menu.
- **pocket-ui never flashes your bootloader.** App updates only install pocket-ui. The system update checker rejects
  bootloader changes, and our cards come without a bootloader flashing kit.
- The [dual-boot guide](../INSTALL-DUAL-BOOT.md) covers models that need a separate boot setup and the boot options
  that can expose internal storage to Linux.

## Pages

1. [Getting started](getting-started.md): installing, first start, Wi-Fi, adding games; [Other handhelds](other-handhelds.md): other Retroid devices, SM8550, Anbernic H700 and the R36S; [The GAMES drive](games-drive.md): games from a computer; [Second card](copy-games.md): games without Wi-Fi; [Game check](game-check.md): will your games start?; [games from your phone](phone-upload.md) over the same Wi-Fi; [games from your RomM server](romm.md), downloaded at their first start; [Kids mode](kids-mode.md): a big simple shelf for a child.
2. [How to](how-to.md): update over Wi-Fi, a game that won't start, pair a controller, back up saves, sync with a second device.
3. [Coming from ROCKNIX, dArkOS or Android](coming-from.md): what happens to your saves, states, favourites and settings.
4. [Playing](playing.md): Home, finding a game, the looks, Recent games; [Console pages](console-pages.md): a console's facts and its Credits & links; [Easy](easy.md): the look with one big game at a time; [Big text, Zoom and Hold to toggle](big-text.md): larger text in one switch, a magnifier for the pause menu, buttons that stay down. [Continue](continue.md): an open case means your moment is waiting, and A goes straight back.
5. [The pause menu](pause-menu.md): what each emulator offers, quitting, Pixel Path; [Saving in the pause menu](pause-menu-saves.md): save slots, Where I quit and Before load, Earlier, when a save fails.
6. [Options](options.md): picture, screen frames, controls, shortcuts, extras and run-ahead in a game, [Screen frame and Screen orientation](options-screen.md), and [Cheats](options-cheats.md); [their shorter list](options-big.md) for PlayStation 2, GameCube, Wii and 3DS.
7. [Controls](controls.md): every button, screen by screen; [in Settings, setup and on the touch screen](controls-settings.md), with the power and volume keys.
8. [Settings](settings.md): the Settings categories and the entries of Look; [Playing and Games](settings-playing.md): the entries of those two categories; [Screen & sound, Controls](settings-sound.md): brightness, volume, the screen and sounds; the buttons and controllers; [Connections, System and Help & tools](settings-system.md): Wi-Fi, Bluetooth, Tailscale, SSH, Sync, PC streaming, the clock, sleep, backups, the Tour and this guide; [quick settings, sleep and power](quick-settings.md); [storage](storage.md): what takes up space on the SD card, clean-ups, hidden games; [Bluetooth](bluetooth.md): pairing controllers, headphones and speakers.
9. [Language](language.md): English, German, Spanish, Brazilian Portuguese, French, Italian or Korean menus, and what stays English.
10. [Night light](night-light.md): warmer, less blue colours in the evening, on a schedule or all the time.
11. [Colours and style](theme.md): the colour sets, fonts and the Retro style for every screen, switched in place.
12. [Performance and fan](performance.md): what games start with, the fan in the menus, Smooth fan, the readout.
13. [Features and the Tour](tour.md): every feature on a card, the first start's profiles, new in this version.
14. [Your library](library.md): game cards, play status, ratings and notes; [your files, gamelists and discs](library-files.md); [Curator](curator.md): Lens, smart shelves, Echoes, Versions and play history.
15. [Achievements](achievements.md): RetroAchievements, signing in, unlocks, offline play.
16. [Systems](systems.md): which systems run on which emulator.
17. [Arcade games](arcade.md): which core plays them, matching romsets, parents, BIOS and CHD files, when a game does not start.
18. [Patches, translations and MSU-1](patches-msu1.md): patching a game on a computer or with RetroArch, and one MSU-1 recipe (not checked on the device).
19. [Box art & downloads](extras.md): box art, PortMaster games, tools.
20. [Sync](sync.md): saves and states on all your devices, with Pocket Sync or Syncthing.
21. [Tailscale](remote.md): reach the Pocket from anywhere on your private network.
22. [Web companion](web-companion.md): your library, uploads and play time in a browser, on your Wi-Fi or over Tailscale.
23. [PC streaming](pc-streaming.md): Moonlight, pairing and leaving a stream.
24. [Updates](updates.md): updating pocket-ui and the system safely.
25. [Safe mode](safe-mode.md): what happens when pocket-ui won't start, and ROCKNIX's own menu.
26. [Hotel and train Wi-Fi](hotel-wifi.md): networks that want a sign-in first, and the other ways online.
27. [Troubleshooting and FAQ](troubleshooting.md): known limits and common questions.

For the install guide and the developer documentation, see the [documentation index](../README.md).
