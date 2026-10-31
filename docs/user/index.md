<!-- Product name: "pocket-ui" is a working name. When the name is decided, replace it in every
     file under docs/user/. It is used only as a plain name, so a search-and-replace is enough.
     Keep labels that the UI itself shows (for example the "pocket-ui" row on the Updates page)
     in step with the code. -->

# pocket-ui

pocket-ui is a game frontend and a small handheld system for the **Retroid Pocket Mini V2**.
It runs on top of ROCKNIX, a Linux system for handhelds, and replaces ROCKNIX's usual menu with a
shelf of your games. You control it with the buttons; the touch screen works on the shelf and in the
lists as well.

This guide is on the device as well: Settings → Help & tools → **Manual**. It is in English; the menus themselves can be
German ([Settings → System → Language](language.md)).

pocket-ui is at an early stage and has been tested on one device. The pages below describe what works
today and say clearly where something is still being built or has not been confirmed on a real device.

## What you need

- A Retroid Pocket Mini V2.
- A microSD card of 8 GB or more for ROCKNIX, pocket-ui and your games.
  <!-- TODO: recommended SD card size. The test card grew its game partition to about 60 GB. -->
- A computer, once, to write the SD card image with balenaEtcher or Raspberry Pi Imager. The image is not published
  yet, and has been started on a device once (a full first start as a new player is still to be repeated); see
  [Getting started](getting-started.md).
- Your own game files. pocket-ui does not come with games or BIOS files.
- Wi-Fi is optional. You need it for cover art, game descriptions, PortMaster games, RetroAchievements, updates
  and copying games over the network.

## What it does

- Shows your games on one shelf: the last game first, then recent games, then your systems.
- Starts games with the emulator that suits each system, and brings you back to the same place
  when you quit.
- Continues games where you left off (on pocket-ui's own emulator host, on RetroArch, and for
  PlayStation 2, GameCube, Wii and 3DS).
- Finds any game as you type, and jumps through long lists letter by letter with L2 and R2.
- Gives you a pause menu on the HOME button: continue, save and load (eight slots per game with previews, on its
  own host and for PlayStation 2, GameCube and Wii), options, quick settings and quit. Whether quit keeps the moment
  for next time is one switch in Settings.
- On its own host: picture settings with a live preview, screen frames, the emulator's own
  options, button remapping, fast-forward, rewind, run-ahead, cheats and button shortcuts.
- Switches between recent games with SELECT, and between two games on its own host about as fast as one starts.
- Helps you find your way around a larger collection with Curator: filters, smart shelves, your own stickers,
  "more like this" and the versions of a game you own on several systems ([Your library](curator.md#curator)).
- Sets up Wi-Fi, cover art and PortMaster games on the device itself, with an on-screen keyboard
  for the gamepad: saved networks, hidden networks and the Wi-Fi country too.
- Starts ROCKNIX's own tools without ROCKNIX's old menu: the emulators' settings, RetroArch's menu, the gamepad test
  and calibration, a file manager and a terminal ([Tools](extras.md#tools)).
- Backs up its settings and your library notes to the SD card, and restores or resets them
  ([Backup & reset](settings-system.md#backup--reset)).
- Offers quick settings (brightness, volume, Wi-Fi, Bluetooth, performance, fan, power) on HOME, quiet menu sounds,
  a battery page with history and time left, and this guide.
- Keeps your saves and save states in step with a computer, a phone or another handheld with Syncthing, and asks
  it to look for new saves when a game ends ([Sync](sync.md)).
- Pairs Bluetooth controllers in one press, and uses them in the menus as soon as they connect ([Bluetooth](bluetooth.md)).
- Puts the Pocket on your Tailscale network, with a QR code to sign in, so it can be reached from anywhere ([Tailscale](remote.md)).
- Opens your library in a browser on your phone or computer, over Tailscale only: edit, upload games, BIOS and
  PortMaster files, see play time and the screen ([Web companion](web-companion.md)). Off until you switch it on.
- Counts RetroAchievements while you play on its own host: a small card for each unlock, the list in the pause
  menu, and your tally on the game card ([Achievements](achievements.md)).
- Plays your computer's games over Wi-Fi with Moonlight, from an entry on Home that you switch on in Settings, and pairs
  the computer on the Pocket with a PIN on screen ([PC streaming](pc-streaming.md)).
- Updates itself from a signed source, with an automatic way back if a new version does not start.

## What it does not touch

- **Android stays as it is.** ROCKNIX, pocket-ui and your games live on the microSD card, and Linux cannot
  see Android's storage: the boot files used here switch the internal storage off.
- **The bootloader is never rewritten by pocket-ui.** It refuses any system update that would rewrite the
  bootloader or a partition (see [Updates](updates.md)).
- The SD card image writes nothing to the device. The manual install can write one small thing: Retroid's own
  u-boot into the `loader` partition, if that partition is empty. Android does not use that partition. The
  [install guide](../INSTALL-DUAL-BOOT.md) explains every step, how to back it up and how to undo it.

## Pages

1. [Getting started](getting-started.md): installing, first start, Wi-Fi, adding games; [Game check](game-check.md): will your games start?
2. [Coming from ROCKNIX or Android](coming-from.md): what happens to your saves, states, favourites and settings.
3. [Playing](playing.md): Home, finding a game, the looks, console pages, the game switcher. [Frozen games](frozen-games.md): your last five games stand a little open, and A goes straight back.
4. [The pause menu](pause-menu.md): save slots, Auto and Before load, quitting, Pixel Path.
5. [Options](options.md): picture, screen frames, controls, shortcuts, extras and run-ahead in a game; [their shorter list](options-big.md) for PlayStation 2, GameCube, Wii and 3DS.
6. [Controls](controls.md): every button, screen by screen; [in Settings, setup and on the touch screen](controls-settings.md), with the power and volume keys.
7. [Settings](settings.md): the Settings categories and the entries of Look, Sound & feel, Playing and Games; [Connections, System and Help & tools](settings-system.md): Wi-Fi, Bluetooth, Tailscale, Sync, the clock, sleep, backups, the Tour and this guide; [quick settings, sleep and power](quick-settings.md); [storage](storage.md): what takes up space on the SD card, clean-ups, hidden games; [Bluetooth](bluetooth.md): pairing controllers, headphones and speakers.
8. [Language](language.md): English or German menus, and what stays English.
9. [Night light](night-light.md): warmer, less blue colours in the evening, on a schedule or all the time.
10. [Colours and font](theme.md): the colour sets and fonts for every screen, switched in place.
11. [Performance and fan](performance.md): what games start with, the fan in the menus, Smooth fan, the readout.
12. [Features and the Tour](tour.md): every feature on a card, the first start's profiles, new in this version.
13. [Your library](library.md): game cards, play status, ratings and notes; [Curator](curator.md): Lens, smart shelves, stickers, Echoes, Versions and play history.
14. [Achievements](achievements.md): RetroAchievements, signing in, unlocks, offline play.
15. [Systems](systems.md): which systems run on which emulator.
16. [Arcade games](arcade.md): which core plays them, matching romsets, parents, BIOS and CHD files, when a game does not start.
17. [Patches, translations and MSU-1](patches-msu1.md): patching a game on a computer or with RetroArch, and one MSU-1 recipe (not checked on the device).
18. [Extras](extras.md): cover art, PortMaster games, tools, the Store (not open yet).
19. [Sync](sync.md): saves and states on all your devices, with Syncthing.
20. [Tailscale](remote.md): reach the Pocket from anywhere on your private network.
21. [Web companion](web-companion.md): your library, uploads and play time in a browser, over Tailscale.
22. [PC streaming](pc-streaming.md): Moonlight, pairing and leaving a stream.
23. [Updates](updates.md): updating pocket-ui and the system safely.
24. [Safe mode and EmulationStation](safe-mode.md): what happens when pocket-ui won't start, and ROCKNIX's own menu.
25. [Hotel and train Wi-Fi](hotel-wifi.md): networks that want a sign-in first, and the other ways online.
26. [Troubleshooting and FAQ](troubleshooting.md): known limits and common questions.

For the install guide and the developer documentation, see the [documentation index](../README.md).
