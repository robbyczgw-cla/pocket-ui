# Games from your phone

The quickest way for a few games: send them straight from your phone. Phone and Pocket must be on the same Wi-Fi.

1. Open Settings → Games → **Add games** (or the "Add your games" step of the first start) and press **Y** ("From your
   phone"). The page **Send from your phone** shows a large QR code and the same address to type: the Pocket's
   Wi-Fi address, a port and a code, such as `…:41234/k7m2qx9p`.
2. Point your phone's camera at the code and open the page it offers (iPhone Safari and Android Chrome both work; nothing
   to install). Pick game files there; on Android you can also pick a whole folder.
3. Each file goes into its system's folder by its file type. Where several systems take the same type (`.zip`, `.iso`,
   `.chd`, `.cue`/`.bin`, `.cso`), the phone asks which system it is, and offers only systems this Pocket has. A disc
   image is never filed under a system on a guess: PSP, PS2, GameCube, Wii and Saturn games are all `.iso` files, and one
   in the wrong folder does not start. Each file has its own answer; after one, a button offers the same system for the rest. A `.cue` or `.gdi` takes the tracks it names along, so they don't ask. A disc game
   picked as a folder keeps its folder (`psx/Game (Europe)/Game.cue` and its `.bin` files).
4. The Pocket shows what happens: **Waiting for your phone**, **Phone connected**, each file with its progress, then
   **Added: 3 games**. Press **B** when you're done: the page closes and the new games are on the shelf.

## What it does and doesn't do

- It takes game files: cartridges (Game Boy to Nintendo 64, also zipped), DS, arcade sets, disc images (`.iso`, `.chd`,
  `.cue` with its `.bin` files, `.gdi`, `.m3u` …) and 3DS. Not DOOM, Daphne, PICO-8, ports, DOS or computer games:
  their files can tell a launcher what to run, so they go onto the card with file sharing or the SD card. Saves and
  BIOS files aren't taken either; BIOS files go into `roms/bios` with the SD card in a computer or the
  [web companion](web-companion.md).
- Each system has a size limit: cartridges 64 MB, DS 512 MB, arcade sets 1 GB, discs up to 9 GB, 3DS 4 GB. Large disc
  images go straight onto the card.
- A disc sheet (`.cue`, `.gdi`, `.m3u`) may name only files in its own folder; one that points anywhere else is refused.
- A file that is already on the Pocket is never replaced: the phone says "Already on the Pocket" and keeps it. To
  send a new version, delete the old one first (file sharing or the SD card).
- Before a file is sent, the Pocket checks that it fits on the SD card (and keeps some room free for saves); if not,
  the phone says so and nothing is written, not even a folder.
- A file arrives under a temporary name in a hidden folder (`roms/.pocket-ui-uploads`) and only gets its place once it
  is complete. If the page is closed, the Wi-Fi drops or the Pocket goes dark while a file is on its way, the phone says
  so and the unfinished file is removed.
- The page closes when the screen goes off (by itself after a while, the lid or the power button), after 10 minutes
  without anything from the phone, and after an hour in any case. A opens it again, with a new code. While a file
  arrives, the Pocket doesn't go to sleep by itself.
- When you leave Add games, the new games are read in and the page offers the [Game check](game-check.md): it explains in plain
  words a file that is empty, the wrong type or misses its BIOS.
- Without Wi-Fi the page says **Connect to Wi-Fi first**; **A** opens the Wi-Fi settings.
- Not on the Steam Deck: there games go into its own folders.

## What plain HTTP on a Wi-Fi means

The page is reachable only while it is open on the Pocket, only on your Wi-Fi, and only with the code in the address,
which is new each time the page opens. **The connection is not encrypted:** someone else on the same Wi-Fi could see
the code and the games you send, and could change them on the way; with the code they could also add games while the
page is open. Both the Pocket and the phone say this under the code. Nobody can download anything from the Pocket this
way: no list of your games, no saves, no settings; the page only tells whether a game of the same name is already
there. Use it on your own Wi-Fi at home; on a Wi-Fi you don't trust (a hotel, a café) use file sharing at home or the
SD card instead. Too many tries with a wrong code close the page.
