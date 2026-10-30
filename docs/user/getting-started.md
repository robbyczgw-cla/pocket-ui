# Getting started

This page takes you from an empty microSD card to your first game. You need a computer once, to write the card.

> **Already play on ROCKNIX or Android?** [Coming from ROCKNIX or Android](coming-from.md) says what happens to your
> saves, states, favourites and settings.

> **Status:** the SD card image builds and passes its offline checks and was started once on a fresh card on a
> Mini V2 (2026-10-27; that start found a Wi-Fi bug in the first-start setup, now fixed), but a full first start as a
> new player has not been repeated with the fixed build, and it is not published. The [install guide](../INSTALL-DUAL-BOOT.md) has the details and the manual way that
> was used on the test device.

> **Other Retroid devices.** pocket-ui is made and tested on the **Retroid Pocket Mini V2**. The **Retroid Pocket 5**,
> **Retroid Pocket Flip 2** (Snapdragon 865) and the first **Retroid Pocket Mini** run the same ROCKNIX system and are
> supported as **experimental**: pocket-ui knows them, but hasn't been tested on one yet. The SD card image below is
> for the Mini V2 only. On the others, see [Other Retroid devices](#other-retroid-devices).

> **Anbernic handhelds (H700).** The **RG28XX**, **RG35XX** (2024, Plus, H, Pro, SP), **RG40XX** (H, V), **RG34XX**,
> **RG34XX-SP** and **RG-SP** run ROCKNIX's H700 image and are supported as **experimental**. The SD card image below is
> for the Mini V2 only. See [Anbernic H700 handhelds](#anbernic-h700-handhelds-experimental).

## 1. Write the SD card

1. Get the SD card image, `pocket-ui-<version>-rp-mini-v2.img.xz`.
   <!-- TODO: link the download once the image is published. Until then it is built with scripts/make-image.sh
   (docs/SD-IMAGE.md). -->
2. Write it to a microSD card of 8 GB or more with **balenaEtcher** or **Raspberry Pi Imager** (in Imager: "Use
   custom"). Both read the `.xz` file directly. Everything on the SD card is erased.

The SD card holds two parts: a small one with the system files (readable on any computer, with the notes in
`pocket-ui-info/README.txt`) and STORAGE for pocket-ui, your games and your saves. STORAGE grows to fill the SD card at
the first start.

The SD card contains no games and no BIOS files. Add only games and BIOS files you own.

### Already have the official ROCKNIX on a card? Two files

If you flashed the official ROCKNIX image yourself, you can add pocket-ui from your computer, without Wi-Fi and without
any command line:

1. New to ROCKNIX on the Mini V2? Set it up once with [ROCKNIX's own guide](https://rocknix.org/devices/retroid/retroid-pocket-mini/) (it covers flashing the card and ROCKNIX's one-time bootloader step), then come back. Flash ROCKNIX 20261001 or newer to the card, but **don't switch the device on yet**.
2. Take the card out and put it in again. Your computer shows one drive called **ROCKNIX**.
3. Copy two files into that drive, in the top folder: `mount-storage.sh` and `pocket-ui-seed.tar.gz` (both are in the
   pocket-ui card zip). Don't rename them or unpack them. Eject the card.
4. Put the card in and switch on. The first start looks like normal ROCKNIX: it makes room for your games and restarts
   by itself.
5. At the second start the screen says "pocket-ui: installing", and then pocket-ui opens. The two files disappear from
   the drive on their own.

Your games, saves and settings are never touched. If pocket-ui doesn't come up, ROCKNIX starts as usual and the
drive holds a file `pocket-ui-install.log` that says why (for example "damaged": copy the tar again). The two files
stay until the install worked, so you can just try again.

## 2. Start pocket-ui

pocket-ui starts from the SD card next to Android; Android and its data stay as they are, and nothing is flashed. A
setting in Retroid's own start menu picks which one starts:

1. Put the SD card into the switched-off device.
2. Hold **Volume Down** and press **Power**. Retroid's menu opens; its bottom line reads "Boot mode: Android".
3. Pick **Switch boot mode** with the volume keys and press Power until the line reads **Boot mode: Loader**, then
   pick **Start**. (Volume Up + Power does not start pocket-ui on current Retroid firmware.)
4. The first start takes a few minutes and restarts once by itself. Then pocket-ui's setup opens (step 3 below).

The setting stays: from now on, switching on starts pocket-ui while the SD card is in.

**What you see when it starts:** Retroid's logo, a moment of black, then a dark screen with a single "pocket-ui" bar
whose coloured edge slowly changes colour, as long as the Pocket is getting ready (about 20 seconds). Then the screen
dips to black and the shelf fades in. The first start after setting up the card says "Getting ready for the first
start…" under it, and a start that takes longer than a minute says "Still starting…".

**To go back to Android:** hold Volume Down and press Power, pick **Switch boot mode** until the line reads **Boot
mode: Android**, then **Start**. The SD card can stay in.

**If the screen stays black** with Boot mode: Loader, switch back to Android as above and see "If something goes
wrong" in the [install guide](../INSTALL-DUAL-BOOT.md). The guide also has the way for a ROCKNIX card you
already use, with no computer commands: copy the one file **Install pocket-ui.sh** into the `roms/ports` folder of the
card (switch on Samba in ROCKNIX's Network Settings, then open `\\<device name or IP>` on Windows or `smb://<device name or IP>` on a Mac and use the games share; [ROCKNIX explains it](https://rocknix.org/play/add-games/)), open **Ports** in EmulationStation (restart its list once: Start → Game Settings → Update Gamelists) and start
**Install pocket-ui**. The screen shows what it checks and installs; if something does not fit, it says what, changes
nothing and **Close** takes you back to EmulationStation. To remove pocket-ui again, start **Uninstall pocket-ui** in
Ports (the installer leaves it there).

If pocket-ui is not installed, ROCKNIX's own menu (EmulationStation) appears instead. If pocket-ui keeps stopping, it
goes back to the last version that worked, then to a [safe mode](safe-mode.md) page; EmulationStation only opens by
itself if pocket-ui cannot start at all.

## 3. First start

The first time pocket-ui runs, it walks you through a short setup. **START skips any step.** You can
open every step again later from **Settings** (press START on the Home screen).
Each step says "Step n of 7" at the top right.

1. **Welcome.** Under the cartridge and your handheld, press A to begin. Left/Right choose the language, **English** or **Deutsch** (German): the screen
   changes at once, and the choice is kept ([Language](language.md)). It starts on the system language
   ROCKNIX was set to. SELECT opens the manual, this guide, on the device (in English); B comes back.
2. **Choose a look.** A small picture of the shelf in each look, the one shown in the middle and its
   neighbours at the sides. It is your own shelf when the SD card already has games: your systems and one game,
   with its box art (a plain drawn box with the console where it has none yet), shown as the game to continue
   if you played it, else as "Not played yet". On an empty card it shows three systems and games called
   "Sample game". Left/Right (or Up/Down) switch looks, Y switches between Dark, Light and Automatic
   ([Appearance](settings.md#appearance)), L1 between Cozy and Quick animations ("Cozy: watch your game slide into the
   console. Quick: straight into the game.", [Animations](settings.md#animations)), R1 between Normal, Medium and Large
   text ([Text size](settings.md#text-size)), then A ("Keep this look"). See [Playing](playing.md#looks) for the five looks.
3. **How do you like to play?** Three ways to start the extras: **Simple**, **Enthusiast** or **Choose for yourself**.
   See below.
4. **Wi-Fi.** See below.
5. **Time zone.** pocket-ui suggests one. See below.
6. **Extras.** Pictures, cover art and PortMaster games. **Pictures** is a choice you see here, **Box art**,
   **Box art and screenshots** (the default) or **Off**, with a line about where they come from. Cover art
   and PortMaster need Wi-Fi and are optional. Nothing is downloaded before you leave this step: with a connection,
   the pictures for the games already on the SD card then download by themselves in the background, as chosen, unless
   you chose Off; this page shows the progress. See [Extras](extras.md#cover-art).
7. **Add your games.** A tile per system with its folder, and a switch for file sharing. See below.
8. **You're all set.** It counts the games already on the SD card ("Your 16 games are on the shelf."). Press A ("Go to
   your games") to reach Home.

After a restart or a flat battery in the middle, the setup opens again at the step it had reached.

In most steps, B goes back one step. The hint bar at the bottom lists only what works right now: without Wi-Fi the Extras
step leaves out A ("Download", "Browse"), and Add your games leaves out X (file sharing), because both need a connection.

### How do you like to play?

pocket-ui has more than it shows at first: Curator's Lens and Echoes, Pixel Path, screen frames, going straight back
into your game, and more. This step sets them in one go. Up/Down choose, A takes it; under the three rows the page
says what the selected one switches on and off, in everyday words ("On: suggestions for similar games, gentle
animations. Off: pick games by mood and length, ..."). The names of the features are on their cards in
[Settings → System → Tour](tour.md).

- **Simple**: "As pocket-ui ships, the same as skipping this step." This is the start
  setting of a new Pocket, nothing more: Lens & smart shelves, Stickers, Echoes, Versions and Motion are on; Up next,
  Pixel Path, Echoes beyond this device, frames on every console and going straight back into your game are off.
- **Enthusiast**: "Suggestions, play statistics, picture details and frames, all on." All of Curator, Pixel Path in the pause menu and over every game, Auto screen frames on every
  console, and On start set to your last game.
- **Choose for yourself**: one card per feature, each with a small picture of what it does and three rows, **On**, **Off**
  and **Later**. Up/Down choose, A takes the choice and shows the next card, B goes back a card, START skips the rest.
  Later leaves the feature as it is and keeps the card for [Settings → System → Tour](tour.md).

START skips the step and leaves everything as it is, which is the same as Simple. A profile only sets switches that Settings has too, nothing of
its own: each one stays in its Settings page afterwards, and [Settings → System → Tour](tour.md) shows the cards again
whenever you like. The table there lists every card and what Simple and Enthusiast do with it. Menu sounds, haptics
and the stick lights are left as they are by both.

### Time zone

The step comes right after Wi-Fi and shows a city with the time it is there now. pocket-ui guesses it without
asking any web service, so nothing about you leaves the device. It looks, in this order, at:

1. **The zone the system already has**, if it is not UTC and somebody chose it. The note says "Already set on this Pocket." The zone a fresh image ships with (New York) does not count: nobody chose it, so the next two steps decide.
2. **The country of your Wi-Fi region.** The Wi-Fi chip knows the country it is allowed to transmit in (the same
   setting that decides which channels it may use). For a country with one time zone, such as Austria or Germany,
   that zone is shown, with the note "A guess from your Wi-Fi region". For a country with several time zones
   (the United States, Canada, Australia, Brazil, Russia, Mexico and a few more), the step opens straight on a
   list of that country's cities, because there is no single answer.
3. **UTC**, if nothing is known. The note says so.

Your choices:

- **A** ("Looks right") keeps the suggestion and goes on. **START** does the same: skipping keeps the guess.
  Nothing changes if the suggestion is the zone the Pocket already has, or if it was UTC.
- **X** ("Change") opens the list of regions, with a search row (the on-screen keyboard) on top. This is the same
  list as Settings → System → [Time](settings-system.md#time). In a city list, A sets that city and moves on; B goes back.
- **B** goes back to Wi-Fi.

The Wi-Fi region is only a hint to where you live: a Pocket bought abroad can still carry another country's
setting, so look at the city before you press A. The clock sets itself from the internet whenever Wi-Fi is
on; that switch stays under Settings → System → Time. Without Wi-Fi, set the date and time by hand there
([Date and time](settings-system.md#time)).

## 4. Connect to Wi-Fi

Settings → Connections → **Wi-Fi**, or the Wi-Fi step of the first start.

1. pocket-ui scans for networks. The line under the title says which network you are connected to, how strong
   it is here and the Pocket's address. The list shows the connected network first, then the others by signal
   strength, then saved networks that are out of range. Each row says **Connected**, **Connecting…**, **Saved**,
   **Saved, out of range**, **Password** or **No password**. A network only says **Connected** once it has given the
   Pocket an address and things online work.
   If the first row says **This network wants a sign-in**, the network (a hotel, a train) wants its page done
   first: see [Hotel and train Wi-Fi](hotel-wifi.md).
2. Choose a network and press A ("Connect").
   A on the network you are connected to gets a tick and "Connected to Home. Press START for the next step." under the
   title. A network one search missed stays listed until three searches in a row miss it.
3. For a network with a password, type it on the on-screen keyboard:
   - D-pad: move between keys
   - A: type the selected key
   - B: delete the last character (with nothing typed, B goes back to the list)
   - X: Shift, for one capital letter
   - Y: switch between letters and symbols ("?123" / "abc")
   - START, or the **Connect** key: connect

   The password is shown in plain text while you type.
4. Connecting can take up to a minute. The result appears under the title, for example
   "That network is out of range. Scan again." or "The network took too long to respond. Try again." When the network
   refuses the password, the keyboard opens again, empty, with "Wrong password. Try again." above the keys; the refused
   password isn't kept, so the next one you type is the one tried. B goes back to the list.

If the Pocket is already connecting to the network you chose, it waits for that connection (up to 50 seconds) instead of
starting again. Choosing a different network leaves the previous one first; a newly typed password replaces an old
attempt. "Wi-Fi isn’t ready yet" means the radio or network service is still starting; try again in a moment.

Press X ("Scan again") to refresh the list. The line under the title is always true to what happened:

- **Wi-Fi is off. Press A to turn it on.** The radio is off (switched off in quick settings, or blocked while the
  Pocket started), and the first row, **Wi-Fi is off**, turns it on with A. The list is empty because there is nothing
  to search with, not because nobody is near. If it does not come on, the line says "Couldn't turn Wi-Fi on. Press A to
  try again."
- **Couldn't search for networks. Try again (X).** The search itself did not work (the system's network service did not
  answer). X tries again. After a start, or when Wi-Fi has just come on, pocket-ui tries once more by itself after a
  few seconds.
- **No networks found. Press X to scan again.** The search worked and nobody is in reach.

Quick settings and this page show the same state: switching Wi-Fi in one shows in the other. The network is remembered and reconnects when the
system starts. If the connection drops while pocket-ui runs, it reconnects by itself (see
[Troubleshooting](troubleshooting.md#questions)).

### Saved networks, hidden networks, the Wi-Fi country

- **A on a saved network** connects to it without asking for the password again. If the saved password no
  longer works, the keyboard comes up.
- **Y on a saved network** asks "Forget …? Press Y again." Y again forgets it (the password goes with it); B keeps
  it. To use it again, connect and type the password.
- **A network that stays on Connecting…**: the Pocket keeps trying a saved network by itself, for example after the
  router's password changed. After a minute the line under the title says "… isn't letting this Pocket in. Press A to
  type the password again." and the row says **Not answering**; A opens the keyboard.
- **Other network…**, the second-to-last row: for a network that doesn't show its name. Type the name, START, then
  the password, START. Leave the password empty for an open network.
- **Wi-Fi country**, the last row: **Automatic** (the default) uses the country the Wi-Fi chip reports, shown
  in brackets. Choose your country when networks on some 5 GHz channels don't show or connect. L2/R2 jump to the
  next or previous letter. The choice is kept across restarts.

## 5. Add games

### Nothing on the SD card yet: the starter pack

A card with no games shows **No games yet** on Home, and under it **Get the starter pack** (A). It is a handful of small games
that their authors share for anyone to play, from Game Boy to Super Nintendo, Mega Drive and GBA. You can also open
it from Settings → Games → **Starter pack**.

1. Connect to Wi-Fi first (Home says "Needs Wi-Fi" while the Pocket is offline).
2. Press A on **Get the starter pack**. The page lists the games with their system and size; **Get all** takes every
   one, A on a game takes just that one.
3. Each game downloads with a progress bar, is checked, and is put into its system's folder (for example
   `roms/gb`). B leaves the page while it runs; the games appear on the shelf when it is done.

Nothing is added without your A, a file that is already there is never replaced, and nothing is written when the
SD card is short of space or the download is damaged. If the Pocket is offline, the page says so; try again once Wi-Fi is on.
Each game's card (X) names its author and licence, and [the credits](../STARTER-PACK.md) list them all. Your own
games work the same way as before: copy them into the folders below.

Games go into the `roms` folder, **one folder per system**, for example `roms/gba` for Game Boy
Advance or `roms/snes` for Super Nintendo. The folder names are the ones ROCKNIX uses; the
[Systems](systems.md) page lists the common ones.

- Put game files **directly** into the system folder. One level of subfolders is read as well, the way a folder per game
  is usually kept: a subfolder with a single game in it shows as one game named after the folder (a disc game with its
  `.cue`/`.bin` files, or a game with an `.m3u` playlist for several discs, see [Games on several discs](systems.md#games-on-several-discs)); a subfolder with several games shows each of
  them. Folders starting with `.` or `_` and anything two levels down are not read.
- Each system accepts the file types ROCKNIX lists for it. Other files are ignored.
- Names are tidied on the shelf: tags such as "(USA)" or "[!]" are hidden, a version number at the end is dropped ("Pipes.v1.02"
  becomes "Pipes"), and "Legend of Zelda, The" becomes "The Legend of Zelda". When two games of a system would end up
  with the same name, each keeps what tells it apart: "Tobu Tobu Girl (Europe)" and "Tobu Tobu Girl (World)", else the
  file type ("(ZIP)") or the folder it is in.
- A disc game (`.cue` or `.gdi`) whose track file is missing is not on the shelf: it could not start. Add games
  names it (see below); once the file is there, the game appears. The track files of a disc (`.bin` next to its
  `.cue`) are part of that game, not games of their own.
- BIOS files, for the systems that need them, go into `roms/bios`. You have to provide them
  yourself.

### The systems grid

Settings → Games → **Add games** (and the "Add your games" step of the first start) shows a tile per system: its
console, its name and its folder, for example `roms/gba`. A system that already has games shows a tick and how
many.

- The popular systems come first: Game Boy, Game Boy Color, Game Boy Advance, NES, Super Nintendo, Mega Drive,
  PlayStation, Nintendo 64, PSP, Nintendo DS, PlayStation 2, GameCube and Dreamcast. Any other system that has
  games follows them.
- The last tile, **All systems**, opens every system ROCKNIX can start (A on it; A again closes the list). Systems
  without a console picture show a plain cartridge.
- Move with the D-pad (L1/R1 jump three rows). Under the grid, the selected system shows its folder, the file
  types it takes, and whether it needs a BIOS: "Not needed", the files it needs and whether they are in
  `roms/bios` (in red while one is missing), or, for systems pocket-ui knows nothing about, that a BIOS would go
  into `roms/bios`.
- The game counts are read when the page opens.
- Under the title, the page says what in `roms` isn't read, in place of its usual line: a folder that is no system's
  ("Folder without a system: Sega Genesis. Its games go in roms/megadrive.", with the folder ROCKNIX reads when it can
  guess it from the name), and a disc game whose file is missing ("Not on the shelf: Lost Bin. Its file Lost Bin
  (Europe).bin is missing."). A folder counts when it holds a file some system takes; `bios`, `savestates` and the
  other folders ROCKNIX keeps for itself, and folders starting with `.` or `_`, never do.

### Over Wi-Fi (file sharing)

1. Open Settings → Games → **Add games** (or the "Add your games" step of the first start). Without Wi-Fi, the right
   side says "Connect to Wi-Fi first" and shows no address yet. With Wi-Fi but sharing off, it says "File sharing is
   off" and how to turn it on, again without an address.
2. Press X ("Turn on file sharing"). The right side under the grid now shows the address, a user (`pocket`) and a
   password made for this Pocket.
3. On your computer, open the address shown on the screen and log in with that user and password:
   - macOS: Finder → Go → Connect to Server, `smb://<address>/games-roms`, "Registered User".
   - Windows: `\\<address>\games-roms` in Explorer's address bar.
   - Linux: `smb://<address>/games-roms` in the file manager.

   The share opens the `roms` folder. Only that folder is shared: pocket-ui's settings, system
   updates and logs are not reachable over the network. The `ports`, `PortMaster`, `bios` and
   `savestates` folders are hidden too, because the Pocket runs programs from them. Files for PortMaster
   games go into `port-files` instead (see [Extras](extras.md#games-that-need-your-own-files)). The `screenshots`
   folder is shared: that is where [screenshots](playing.md#screenshots) taken in a game end up.
4. Copy your games into the right system folders.
5. Press X again ("Turn off file sharing") when you're done. Sharing is also off after every restart.

The password stays the same for this Pocket. BIOS files can't be copied over the share; put them into
`roms/bios` with the SD card in a computer.

### With the SD card in a computer

The `roms` folder is on the SD card's large STORAGE part, which uses the Linux file system ext4.
Windows and macOS cannot write to ext4 without extra software, so the "Add your games" screen
suggests file sharing instead.

### Coming from EmulationStation

The whole move, saves included, is on [Coming from ROCKNIX or Android](coming-from.md). Only the gamelists are here.

If your games were on ROCKNIX's EmulationStation, KNULLI, Batocera or ES-DE, bring the scraped information along: copy
each system's `gamelist.xml` with the games (it sits in the system's folder, `roms/gba/gamelist.xml`, and ES-DE keeps
its lists in `ES-DE/gamelists/` and its pictures in `ES-DE/downloaded_media/`). pocket-ui reads them and never changes
them. A file name like "sf2ce.zip" then shows its real name, a card shows the description, developer and genre
the gamelist holds, and a game without box art shows the picture the gamelist names. Settings → Games →
**From EmulationStation** offers to add your favourites to Favourites, and your ratings as My rating, when you want
them; nothing is taken over by itself. More: [Your EmulationStation gamelists](library.md#your-emulationstation-gamelists).

### When new games appear

The game list is read when pocket-ui starts. To be on screen quickly, the shelf first shows the list as it was
last time and reads the folders again right after; games you added or deleted since then appear or disappear within
about a second, the way they do after Add games. It is also read again whenever you leave a step you opened from
Settings (for example Settings → Games → Add games, then B). If a game does not show up, check that it is directly
in the right folder and has a file type that system accepts; Add games names folders it doesn't read and disc games
missing a file.

A game whose file is no longer on the SD card (deleted or moved over file sharing, for example) does not start: the
Pocket says "*Game* can't start: its file … is no longer there" and reads the list again.

## Other Retroid devices

pocket-ui also runs on the **Retroid Pocket 5**, the **Retroid Pocket Flip 2** (the Snapdragon 865 one) and the first
**Retroid Pocket Mini**, as **experimental**: they use the same ROCKNIX system as the Mini V2, so most things should
work, but nobody has tried pocket-ui on one yet.

- **Installing.** Don't use the Mini V2's SD card image on them. Install ROCKNIX for that device as ROCKNIX's own guide
  says, start it once, then add pocket-ui with the install package for an existing ROCKNIX card (the
  [install guide](../INSTALL-DUAL-BOOT.md#install-on-a-rocknix-card-you-already-use) has the steps). It recognises
  these devices and says they are experimental.
- **What you see.** The first start says once that pocket-ui hasn't been tested on your device; Settings → System →
  About shows the device with "(experimental)". Everything else is the same as on the Mini V2. On the Pocket 5's and
  Flip 2's wider screen, Settings and the Shelf look sit in a column in the middle; on the Flip 2, closing the lid
  puts it to sleep.
- **If something doesn't work**, Settings → System → About → Report a problem saves a file to attach to a report.



### Experimental SM8550 handhelds

The **Retroid Pocket 6** (including TOP-DPAD), **Retroid Pocket Nova**, **AYN Odin 2 Mini**,
**AYN Odin 2** and **AYN Odin 2 Portal** are experimental. Only the Odin 2 Mini has been tried on a real device
(2026-10-27: the install package, the first-start screens, the shelf, a GBA game with the pause menu, screen off and
wake, driven over the command pipe; no button or touch pressed, no Bluetooth, no sleep); the other four are computer
tests only, with no hardware verification. Use ROCKNIX's own **SM8550 aarch64 image**
(20261001), set up following ROCKNIX's guide for your device, boot it once, then add pocket-ui
with the install package. The Mini V2 SD image must not be used on these devices.

Pocket 6 and the Odin 2 family use 1920×1080; Nova uses 1280×960. First start says once that the device is untested;
About marks it experimental. **Stock ROCKNIX still disables sleep on these devices (checked 2026-10-27):** the power button
turns the screen off, and idle turns it off with the game paused. Any button brings it back.
Settings → System → Sleep & power offers **Switch off after** (Off by default) to save and power
off after a longer dark spell on battery. Screen off still uses battery. ROCKNIX controls the fan;
pocket-ui's fan choices have no effect here. Touch, LEDs, vibration, battery drain, 120 Hz pacing
and Zink on the real GPU still need a tester. Mini and Portal menus start with Xbox buttons
(A bottom); the original Odin 2 uses Nintendo buttons (A right). Saved layout choices win. See [device notes](../DEVICES.md).

## Anbernic H700 handhelds (experimental)

pocket-ui runs on the Anbernic handhelds that ROCKNIX's **H700 image** supports. All of them are **experimental**:
pocket-ui knows each model (screen, buttons, what it has), but it has only been tested on a computer, not on a real
handheld yet. If something doesn't work, Settings → System → About → Report a problem saves a file to attach to a
report. The **RG CubeXX** (square 720×720 screen) is **not supported yet**: pocket-ui draws that screen too slowly on
this processor for now, and the installers say so and change nothing.

| Model | Screen | Wi-Fi | Sticks | Lid | Lights |
|---|---|---|---|---|---|
| RG28XX | 640×480 | none | no | no | |
| RG35XX 2024 | 640×480 | none | no | no | |
| RG35XX Plus | 640×480 | yes | no | no | |
| RG35XX H | 640×480 | yes | yes | no | |
| RG35XX Pro | 640×480 | yes | yes | no | |
| RG35XX SP | 640×480 | yes | no | yes | |
| RG40XX H | 640×480 | yes | yes | no | RGB |
| RG40XX V | 640×480 | yes | yes | no | RGB |
| RG34XX | 720×480 | yes | no (ROCKNIX lists none) | no | |
| RG34XX-SP | 720×480 | yes | yes | yes | |
| RG-SP | 720×480 | yes | no | yes | |
| RG CubeXX | 720×720 | yes | yes | no | RGB (**not yet**) |

All of them have 1 GB of memory, no touch screen and no fan. The RGB lights of the RG40XX are ROCKNIX's; pocket-ui
doesn't change them.

### Install

1. **Use a spare microSD card for ROCKNIX and keep your stock card as it is.** The H700 handhelds have no internal
   storage: the stock system lives on the card in slot 1, so ROCKNIX goes on a different card and you can switch back
   by swapping cards. Back up the card with your games before you put it in a ROCKNIX boot.
2. Download ROCKNIX's **H700 image** from the [official ROCKNIX H700 install page](https://rocknix.org/configure/h700-installation/)
   (version 20261001 or newer) and flash it with balenaEtcher or Raspberry Pi Imager. There is one image per memory
   type: **try the DDR4 image first. If the handheld doesn't start from it** (the screen stays black), flash the **DDR3**
   image onto the same card instead; only the card is at risk.
3. Take the card out and in again so your computer shows the **ROCKNIX** drive. Copy your model's device tree file
   from its `device_trees` folder to the top folder of that drive and name the copy **`dtb.img`** (all lower case).
   Eject the card properly: skipping this step can leave the card unable to start.

   | Model | Device tree file (in `device_trees`) |
   |---|---|
   | RG28XX | `sun50i-h700-anbernic-rg28xx.dtb` |
   | RG35XX 2024 | `sun50i-h700-anbernic-rg35xx-2024.dtb` (`...-2024-rev6-panel.dtb` if the picture is garbled) |
   | RG35XX Plus | `sun50i-h700-anbernic-rg35xx-plus.dtb` (`...-plus-rev6-panel.dtb`) |
   | RG35XX H | `sun50i-h700-anbernic-rg35xx-h.dtb` (`...-h-rev6-panel.dtb`) |
   | RG35XX Pro | `sun50i-h700-anbernic-rg35xx-pro.dtb` |
   | RG35XX SP | `sun50i-h700-anbernic-rg35xx-sp.dtb` (`...-sp-v2-panel.dtb`) |
   | RG40XX H | `sun50i-h700-anbernic-rg40xx-h.dtb` (`...-h-v2-panel.dtb`) |
   | RG40XX V | `sun50i-h700-anbernic-rg40xx-v.dtb` (`...-v-v2-panel.dtb`) |
   | RG34XX | `sun50i-h700-anbernic-rg34xx.dtb` |
   | RG34XX-SP | `sun50i-h700-anbernic-rg34xx-sp.dtb` (`...-sp-v2-panel.dtb`) |
   | RG-SP | `sun50i-h700-anbernic-rg-sp.dtb` |

   The name in brackets is for a newer panel revision: if the first file shows a garbled or striped picture, hold
   the power button for 10 seconds, flash again and try that one.
4. Start ROCKNIX from the card. The first start grows its storage and restarts by itself; then ROCKNIX asks its own
   first questions. Let it come up to EmulationStation once.
5. Add pocket-ui. Without a network (the RG28XX and RG35XX 2024 have no Wi-Fi), use one of these:
   - **The Ports installer:** copy the single file *Install pocket-ui.sh* into the `roms/ports` folder of the card
     (the card in your computer, or ROCKNIX's file sharing), start EmulationStation again, open **Ports** and start
     **Install pocket-ui**. The screen shows the checks and the install, then pocket-ui starts.
   - **Two files on the card:** copy `mount-storage.sh` and `pocket-ui-seed.tar.gz` to the top folder of the
     **ROCKNIX** drive, put the card in and start the handheld. It installs during that start and opens pocket-ui.
   With Wi-Fi and SSH, the install package for an existing ROCKNIX card works too (see the
   [install guide](../INSTALL-DUAL-BOOT.md#install-on-a-rocknix-card-you-already-use)).

   All three check the device first: only the models in the table are accepted, and an H700 handheld that isn't in
   it is refused with a sentence and nothing is changed. ROCKNIX keeps starting as before.

### What works, what is hidden

- **Games.** The consoles ROCKNIX's H700 image ships cores for play in pocket-ui's own player, with its pause menu,
  states and shortcuts. **N64, PSP, Dreamcast and Nintendo DS stay on ROCKNIX's own emulators** (they ask more than
  these processors give). PS2, GameCube, Wii and 3DS are not in ROCKNIX's H700 image, so those systems don't appear.
- **Screen.** pocket-ui draws its normal layout and reduces it to the small screen, so it looks like on the Mini V2,
  only smaller. Small hint text can be hard to read on the 640×480 handhelds; Settings → Look & feel → Text size
  makes it larger.
- **Hidden or reduced.** Fan settings say "No fan"; there is no touch setup. The RG28XX and RG35XX 2024 say
  "This device has no Wi-Fi" on the Wi-Fi page and offer nothing to switch on there: everything else works offline,
  while online features (Store, covers, updates, RetroAchievements, file sharing) wait for a network. On the
  handhelds without sticks, no stick lights are offered.
- **Time zone** is set the H700 way (ROCKNIX's system setting); the first-start step is there as on the other devices.
- **To go back:** take the card out and put your stock card in, or run `sh /storage/pocket-ui/uninstall.sh`, or
  start *Uninstall pocket-ui* in Ports: EmulationStation is back at the next start. Don't run ROCKNIX's online
  update on a card with pocket-ui until a pocket-ui release says that version was checked.
