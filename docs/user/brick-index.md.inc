<!-- The TrimUI Brick's first page of the manual (manual::PAGES under the `brick` feature). Same "Pages" list as index.md
     without the pages that are ROCKNIX's only (manual::BRICK_SKIPS). -->

# pocket-ui on the TrimUI Brick

pocket-ui is a game frontend for your TrimUI Brick: a shelf of your games, their real boxes, a pause menu with save slots, and it
picks up where you quit. You control it with the buttons.

This guide is on the Brick as well: Settings → Help & tools → **Manual**. It is in English; the menus themselves can be German or
Spanish ([Settings → System → Language](language.md)).

## On the Brick

- **Its own SD card.** pocket-ui runs from a card of its own: unzip the card download onto an empty microSD card, put it in, switch
  on. Nothing is flashed and the Brick's own system stays as it is: put your usual card back and the Brick is what it was.
- **Where things are on the card**, as a computer sees it: games in `Roms/<system>` (for example `Roms/gba`), a game's own save
  file next to it, BIOS files in `Roms/bios`, covers in `Roms/<system>/.media`, save states in `Saves/savestates`.
- **The power key.** A short press puts the Brick to sleep (or only turns the screen off: Settings → System → Sleep & power →
  Short press); the game waits in its pause menu and the power key wakes it. Hold it a moment for the
  power menu (Power off saves the game first). Held 6 seconds it switches the Brick off, after the running game has saved.
- **Updates over Wi-Fi.** Settings → System → **Updates** downloads a new pocket-ui and puts only its own program folder on the
  card: games, saves and settings stay. A new version that doesn't start goes back to the one before by itself.
- **Wi-Fi is optional.** You need it for cover art, updates, Tailscale, Sync and copying games from your phone.
- **The Brick's keys:** MENU opens the pause menu in a game and the quick settings on the shelf; F1 and F2 are quick save and
  quick load unless you choose otherwise (Settings → Sound & feel → **Lights & buttons**, with the lights and the mute slider).

## Pages

1. [Games without Wi-Fi](copy-games.md); [Game check](game-check.md): will your games start?; [games from your phone](phone-upload.md) over the same Wi-Fi; [Kids mode](kids-mode.md): a big simple shelf for a child.
2. [How to](how-to.md): update over Wi-Fi, a game that won't start, pair a controller, back up saves, sync with a second device.
3. [Playing](playing.md): Home, finding a game, the looks, console pages, Recent games; [Easy](easy.md): the look with one big game at a time; [Big text, Zoom and Hold to toggle](big-text.md): larger text in one switch, a magnifier for the pause menu, buttons that stay down. [Continue](continue.md): an open case means your moment is waiting, and A goes straight back.
4. [The pause menu](pause-menu.md): what each emulator offers, quitting, Pixel Path; [Saving in the pause menu](pause-menu-saves.md): save slots, Where I quit and Before load, Earlier, when a save fails.
5. [Options](options.md): picture, screen frames, controls, shortcuts, extras and run-ahead in a game, and [Cheats](options-cheats.md); [their shorter list](options-big.md) for the big consoles of other handhelds.
6. [Controls](controls.md): every button, screen by screen; [in Settings, setup and on the touch screen](controls-settings.md), with the power and volume keys; [the controls card](controls-card.md), which button is which on each console.
7. [Settings](settings.md): the Settings categories and the entries of Look, Playing and Games; [Sound & feel](settings-sound.md); [Connections, System and Help & tools](settings-system.md): Wi-Fi, Bluetooth, the clock, sleep, backups, the Tour, this guide and For tinkerers (Sync and PC streaming); [quick settings, sleep and power](quick-settings.md); [storage](storage.md): what takes up space on the SD card, clean-ups, hidden games; [Bluetooth](bluetooth.md): pairing controllers, headphones and speakers.
8. [Language](language.md): English, German or Spanish menus, and what stays English.
9. [Night light](night-light.md): warmer, less blue colours in the evening, on a schedule or all the time.
10. [Colours and style](theme.md): the colour sets, fonts and the Retro style for every screen, switched in place.
11. [Performance](performance.md): how fast the processor may run in games and menus.
12. [Features and the Tour](tour.md): every feature on a card, the first start's profiles, new in this version.
13. [Your library](library.md): game cards, play status, ratings and notes; [Curator](curator.md): Lens, smart shelves, Echoes, Versions and play history.
14. [Achievements](achievements.md): RetroAchievements, signing in, unlocks, offline play.
15. [Arcade games](arcade.md): which core plays them, matching romsets, parents, BIOS and CHD files, when a game does not start.
16. [Patches, translations and MSU-1](patches-msu1.md): patching a game on a computer or with RetroArch, and one MSU-1 recipe (not checked on the device).
17. [Extras](extras.md): box art, PortMaster games, tools.
18. [Sync](sync.md): saves and states on all your devices, with Syncthing.
19. [Tailscale](remote.md): reach the Brick from anywhere on your private network.
20. [Web companion](web-companion.md): your library, uploads and play time in a browser, over Tailscale.
21. [Updates](updates.md): updating pocket-ui safely.
22. [Hotel and train Wi-Fi](hotel-wifi.md): networks that want a sign-in first, and the other ways online.
23. [Troubleshooting and FAQ](troubleshooting.md): known limits and common questions.

For the install guide and the developer documentation, see the [documentation index](../README.md).
