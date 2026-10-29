# Extras

Settings → Games → **Extras** (also a step of the first start). The extras are optional and need Wi-Fi;
without a connection the page says "Connect to Wi-Fi first to use these." The page has three rows: **Cover art**,
**Pictures** (a choice of three, which works without Wi-Fi) and **PortMaster games**. Under the rows a line says where the
pictures come from and how much space they take.

This page is about the Extras in Settings. The pause menu has an Extras group of its own, with
fast-forward, rewind and more; see [Options](options.md#extras).

## Starter pack

Settings → Games → **Starter pack** is not an extra of this page, but it works the same way: free games that their authors
share for anyone to play, downloaded to your SD card with A when you ask. See
[Getting started](getting-started.md#nothing-on-the-sd-card-yet-the-starter-pack).

## Cover art

"Box art for your games, from the free libretro collection." No account is needed.

### By itself

Covers come without asking whenever the Pocket is online and some games have none yet, unless **Pictures** is
[Off](#pictures):

- in the first start, once you have left the Extras step (so the choice comes first; nothing is downloaded before),
- when pocket-ui starts and Wi-Fi is up (it waits up to three minutes for the connection),
- when the game list is read again and has new games, for example after Settings → Games → Add games.

It runs in the background. The only sign is a small note at the top right of the shelf, "Covers 3 of 12", and
the covers appearing as they arrive. It never runs while a game is played: starting a game stops it after the
cover it is on, and it goes on when you are back on the shelf. A game it found nothing for is not looked up
again (see below); a game it could not look up because the connection dropped is tried again at the next start.

### By hand

1. Select **Cover art** and press A ("Download"). If the automatic download is running, the page shows its
   progress instead of starting a second one.
2. pocket-ui looks up every game that has no cover yet (and, with Pictures on Box art and screenshots, no screenshot
   or title screen) and shows its progress, for example "Looking up 3 of 120". At the end it reports "Found 42 new covers"
   or "No new covers found" ("pictures" instead of "covers" while Pictures is Box art and screenshots, since both are counted).

Good to know:

- Games are recognised **by their checksum** first, so an oddly named file still gets the right cover. For
  cartridge consoles (Game Boy to Nintendo DS, NES, SNES, N64, Mega Drive, Master System, Game Gear, PC Engine and
  the other handhelds) pocket-ui reads each game file once in the background, at the lowest priority and never
  while a game runs (of a zip file only its table of contents, so it is quick), and looks the number up in that
  console's list of known releases. That gives the release's usual name: "0902 - MM&B (U).gba" is found as "Mega Man &
  Bass (USA)". The list is downloaded once for each console you have games for, when the Pocket is online (a few
  hundred KB), and kept on the SD card. A file is read again only when it changes.
- Games the list doesn't know, discs of the other consoles (Saturn, Dreamcast, PSP, GameCube, ...), 7z files and files
  over 64 MB are matched **by the title in the file name**. Region and version tags in brackets are ignored, a
  leading release number ("0902 - ") is dropped, and when several regions exist the original release is
  preferred. Names in the usual "No-Intro" style, such as "Tetris (World) (Rev 1)", match best. If a name is
  too different, no cover is chosen rather than a wrong one.
- A **PlayStation 1 or 2** disc is recognised by the **serial on the disc** instead: pocket-ui reads the disc's
  little boot file ("SLES_556.73"), looks it up in the console's list of releases, and the cover, the pictures and
  the facts follow that release's name ([the game card](library.md#playstation-1-and-2-discs)). The discs of a
  multi-disc game show "(Disc 2)".
- A game for which nothing was found is remembered and skipped on later runs, so running it again
  only looks up new games. A game recognised by its checksum later (once the console's list is on the SD card) is
  looked up again by itself.
- A wrong cover, or none for a game that should have one: open the game's card, press Y and choose **Wrong
  pictures? Look up again** ([the game card](library.md#wrong-pictures-look-up-again)).
- Covers are kept on the SD card, in `/storage/.config/pocket-ui/covers/<system>/<game file name>.png`.
  You can put your own PNG image there under the same name.
- Covers appear in the Shelf, Split, Cards and Cabinet looks, not in Minimal.

### Pictures

All pictures come from the free libretro collection (its thumbnail server), with no account. Only the name of a game
is sent to look it up; the games themselves, your saves and anything about you stay on the Pocket. **Pictures**
chooses what is downloaded by itself and what the shelf uses. The row shows the three choices at once; A (or
Left/Right) moves to the next:

- **Box art and screenshots** (the default): the box art and, after it, each game's screenshot and title
  screen. Online only, never while a game is played, a game that starts stops it after the file it is on, a game
  the collection has nothing for is remembered. Box art comes first, the pictures after it.
- **Box art**: only box art is downloaded, and the screenshots and title screens already on the SD card are not shown.
  Pictures that are there stay on the SD card; switching back shows them again. Switching to Box art and screenshots
  starts the download at once when the Pocket is online.
- **Off**: nothing is downloaded by itself, not box art either, and the same pictures as with Box art are shown.
  What is on the SD card stays. The **Cover art** row still looks up box art when you press A on it, and a game's
  "Wrong pictures? Look up again" still works; those are things you ask for. Choosing another value starts the
  download again.

On the first start the choice is part of the Extras step, and the downloads wait until you leave it. The same row is in
Settings → Games → Extras.

Where they show:

- **Split**: the picture on the right shows the box art; when you stay on a game for a moment (about two and a half
  seconds) it fades to the game running, then to the title screen, then back to the box art, a few seconds each. The
  next game starts again from its box art. With [Motion](settings.md#motion) Off nothing changes by itself: the box
  art stays.
- **The game card** ([What the card shows](library.md#what-the-card-shows)): the screenshot and the title screen under the facts,
  as "In the game" and "Title screen".
- **The web companion**: the game's page shows them under "Pictures" (read only).

The "In the game" picture does not have to be the collection's: in your [screenshots](playing.md#looking-at-your-screenshots)
**Y** makes one of your own the game's picture. It replaces the downloaded one (only while Game pictures shows
screenshots); [Wrong pictures? Look up again](library.md#wrong-pictures-look-up-again) on the card takes it away again
and fetches the collection's.

A game the collection has no screenshot for shows its box art alone, exactly as before; a game with neither shows
the console, as it did. Nothing is read from the SD card while you scroll through the list, only once you stay on a
game.

Good to know:

- They are matched exactly like the box art (by the checksum's name, then by the title in the file name, see above)
  and kept next to it:
  `/storage/.config/pocket-ui/snaps/<system>/<game file name>.png` for the screenshot and `.../titles/<system>/…`
  for the title screen. A PNG of your own under the same name is used like a downloaded one; an absurdly large
  file is ignored.
- Box art takes about 450 KB a game. Screenshots and title screens add much less: about 20 KB a game for 8- and
  16-bit consoles, 180 KB for Game Boy Advance, 400 KB for PlayStation. 330 games of a typical mix come to about
  40 MB, a library of 8- and 16-bit games to under 10 MB, 330 PlayStation games to about 140 MB. The lists of the
  collection are fetched compressed, a few hundred KB for every console.

<!-- TODO: the covers folder is not inside the shared roms folder, so adding your own covers needs
     another way to reach the SD card (SSH, or the SD card in a computer that reads ext4). -->

## PortMaster games

PortMaster offers PC and indie games for handhelds. pocket-ui lists the ones that run on this device's
processor type and are not marked experimental, in two lists:

- **Ready to play**: games that run as they are.
- **Bring your own game files**: free engines and ports of games you own (from Steam, GOG, itch.io and so on).
  They need files from your own copy of the game. pocket-ui shows what goes where; it never downloads game
  files itself.

1. Select **PortMaster games** and press A ("Browse"). The catalogue loads (it is refreshed at most
   once a day).
2. Left/Right switches between the two lists; each keeps its own place. Move with Up/Down, or a page at a
   time with L1/R1. The right side shows a picture, the genres and a description. Each row shows the download
   size, or **Installed**.
3. Press A to **Install**. When it is done: "… is installed. Find it under Ports."
4. Press A on an installed game to **Remove** it.
5. When PortMaster has a newer version of an installed game, its row says **Update available** (and so does its
   card, quietly, under Speed). Press A to update it, the same way it was installed; Y removes it instead. Nothing
   updates by itself.

The shelf shows the game's cover from PortMaster (else its screenshot) as the box, and the colour of the spine comes from
it. A game without any picture keeps the plain look.

Installed games appear on Home under the **Ports** system and start like any other game. PortMaster
itself is prepared automatically the first time you install something.

HOME opens the [pause menu](pause-menu.md#portmaster-ports) over a running port (Continue, Options, Quick settings,
Quit). Its performance and fan are in Options there, or on its card: X on the port, then START opens **Performance &
fan** (Battery, Auto or Performance; Very quiet, Quiet, Auto or Cool). A port that keeps a processor core busy all the time is
pointed out there once, with Battery in one press. See [PortMaster ports](performance.md#portmaster-ports).

### Games that need your own files

In the **Bring your own game files** list, the right side also shows PortMaster's instructions under
"Your own game files". Press X ("How to copy") for the page **Copy your game files**. It also opens by
itself after you install one of these games.

- **On this Pocket**: the folder the files go into, for example `/storage/roms/ports/sprout/assets`. pocket-ui
  reads it from the instructions; when they name no folder, it shows the game's own folder.
- **From a computer**: file sharing hides the `ports` folder, because the Pocket runs programs from it. Each
  of these games gets a folder of its own in the share instead, in `port-files`, named like the game's folder.
  Put the files there; **when you start the game, pocket-ui moves them into place**. The page shows the
  address, user and password while file sharing is on; when it is off, press X on the page to turn it on
  (the same switch as on Settings → Games → Add games). For -SPROUT-:
  - Windows: `\\<address>\games-roms\port-files\sprout`
  - macOS and Linux: `smb://<address>/games-roms/port-files/sprout`
- **From a browser**: the [web companion](web-companion.md)'s Upload page puts files into the same `port-files`
  folder, over Tailscale.
- **PortMaster says**: the instructions as PortMaster wrote them. Up/Down scrolls when they are long.

What pocket-ui moves from `port-files`:

- When the instructions name one folder (most games: `assets` or `gamedata`), everything you put into
  `port-files/<game>` goes into that folder.
- When they name several (Counter-Strike: `valve` and `cstrike`), put those folders into `port-files/<game>`;
  only folders with those names are moved.
- When they name the game's folder itself and the files to copy (for example `arco.exe`), only those files
  are moved.
- A file that is already in the game's folder is never replaced (only the empty placeholder some games come
  with), and links are not moved. Whatever is not moved stays in `port-files`.
- When the instructions say neither, there is no `port-files` folder for the game. Copy the files with
  [Remote access (SSH)](settings-system.md#remote-access-ssh), or with the SD card in a computer that reads Linux (ext4) drives.

### When the files are missing

When you start one of these games and its files are definitely not there, pocket-ui shows **Copy your game
files** ("… can't start yet") instead of starting into a black screen. A or B goes back to the shelf.
Before that it moves anything waiting in `port-files` into place, so copying and starting is enough.

pocket-ui only stops a game when it is sure: the folder the instructions name is missing or holds nothing
but the placeholder the game came with, or none of the files the instructions name is anywhere in the game's
folder; and nothing in the game's folder has changed since it was installed, and the game has never run for a
minute. In every other case the game starts as before. If it still doesn't work, check PortMaster's
instructions with X in the list.

Many of these games prepare your files the first time they start. That can take several minutes, and some
empty the folder you copied into afterwards; that is normal.

## Store

The Store is a page for optional extras, so the system itself can stay small. **It is not open yet:** there is no
public store, so Settings → Games has no Store row and nothing else leads to the page. The row appears by itself
on a device that has a store set up (a `store_url` in settings.json, see [STORE.md](../STORE.md)). What the page
will hold:

- other emulator cores and newer versions of cores
- screen frames (bezels) for games on pocket-ui's own host
- fonts (the only kind of theme so far)
- small tools

The page has three levels: categories, the packages in a category, and one package with its description, size
and version. It installs, updates, runs (tools) and removes packages. Installed tools are also listed under
Settings → Games → [Tools](#tools), next to ROCKNIX's own. It has the same protections as
[Updates](updates.md): the list of packages is signed, every download is checked before anything is unpacked,
and removing a package deletes exactly the files it added. The Store does **not** offer BIOS files.

The page is built and tested against a local test catalogue only. Tools you installed from a store earlier stay
under Settings → Games → [Tools](#tools), whether the Store row is there or not.

For the format of packages and how a catalogue is signed, see [STORE.md](../STORE.md).

## Tools

Settings → Games → **Tools** starts ROCKNIX's own tools, which used to need ROCKNIX's old menu (EmulationStation), and the
tools you installed from the [Store](extras.md#store). The right end of each row says what it is:

- **Emulator**: an emulator's own menus, for settings pocket-ui has no page for: **RetroArch** (cores, shaders, input),
  **Dolphin**, **PPSSPP**, **Azahar**, **ARMSX2**, DuckStation, melonDS, Flycast, ScummVM and more.
- **Controls**: **Test gamepad** (every button and stick as you press it), **Calibrate gamepad** (the fix for a
  drifting stick), **Test touch screen**.
- **System**: **File manager**, **Terminal** (needs a USB keyboard), Mount NFS, the scans for ScummVM, id Tech and
  Heroic games, installing Steam or Heroic, Remove ._ files, cloud backup and restore.
- **Store**: tools installed from a store. They run without a window; what they say shows on the page.
- **Unknown**: a script in ROCKNIX's tools folder that pocket-ui does not know.

Below the list: what the selected tool is for and how to leave it. **A** starts it: pocket-ui shows "Starting …"
and the tool takes the screen, the way a game does. When it ends, you are back on this page. A tool that ends at once
with an error (for example because its emulator is missing) is reported here in red.

- **Leaving a tool:** its own Quit or Exit; or hold **L1** and press **SELECT + START** (ROCKNIX's way, for the tools
  that support it); or hold **HOME** for a second and let go: pocket-ui stops the tool. HOME has no menu while a tool
  runs.
- **Tools that change or send something ask once more**: cloud backup and restore, installing or removing Steam and
  Heroic, Mount NFS, Remove ._ files, Calibrate gamepad, and every Unknown script. The warning is red; A again starts
  it, B keeps it closed.
- **Never listed:** anything that updates the system or its bootloader, wipes a drive or resets the Pocket (ROCKNIX's
  installer, for example). On this device such updates can make the Pocket unable to start.
- **X** looks for the tools again.

The emulators' menus change the emulators' own settings, as on ROCKNIX: games on those emulators use them, games on
pocket-ui's own host don't. Calibrate gamepad pauses the menus' controls while it runs; they come back when it
ends. **Scan ScummVM games** restarts pocket-ui at the end (ROCKNIX's script does that).
