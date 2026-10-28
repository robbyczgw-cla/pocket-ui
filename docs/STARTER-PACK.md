# Starter pack

A card with no games shows an empty shelf, which is a poor first minute. The starter pack offers a handful of **free
homebrew games** to download at once, so a new player can play straight away (Recalbox does the same). It is the page
Settings → Games → **Starter pack**, and on Home while no system has a game: **No games yet** with **Get the starter pack**
(A). The user's view is in [Getting started](user/getting-started.md#nothing-on-the-sd-card-yet-the-starter-pack).

> **Status** (ledger: [STATUS.md](STATUS.md)): built and tested offline (unit tests, snapshots, the virtual device, where all nine games were downloaded and placed). Not yet run on the Mini V2 itself.

## Rules

1. **Legal first.** A game goes in only when its author's own notice allows redistribution: an open-source licence, or a
   Creative Commons licence that allows it. **NonCommercial and NoDerivatives licences are out** (a pocket-ui image
   may one day be sold), and so is anything unclear: no licence stated, music or art bought or taken from somewhere
   with other terms, a port of a game whose own licence is not free. The words of the author's notice and the page
   they are on are in `assets/starter-pack.json` (`license_text`, `license_url`) and below.
2. **No ROM in the repository or the image.** The file is fetched from the author's own release page (one game from a
   pinned commit of Homebrew Hub, see the table) when the player presses A, never before.
3. **Nothing happens by itself.** No download starts at boot, and none without A.
4. **Never replaces** a file that is on the card (compared without regard to case), and **never writes** when the
   download fails, is the wrong size or hash, or the card has no room.

## The games

Checked at the sources on 2026-10-24. Every file address is fixed (a release asset or a pinned commit), so the SHA-256 below stays
true; if an author moves a file the download fails the check and nothing is added (see "Keeping it working").

| Game | System | Author | Licence | Download (size, SHA-256) |
|---|---|---|---|---|
| **uCity** (1.3) | Game Boy Color | Antonio Niño Díaz (AntonioND) | GPL-3.0-or-later; art and music CC BY-SA 4.0 | [ucity.gbc](https://github.com/AntonioND/ucity/releases/download/v1.3/ucity.gbc), 131072 bytes, `9422ee2ca7b7ea1d46b58b2a429fff3f354dfd3e732dee1e7ae6220f148ce6e0` |
| **Tobu Tobu Girl** (1.0) | Game Boy | Tangram Games (Simon Larsen) | MIT (code); art, text, sound and music CC BY 4.0 | [tobu.gb](https://raw.githubusercontent.com/gbdev/database/eee749dcd38052be3cb00a47ca03690603eb163e/entries/tobutobugirl/tobu.gb), 262144 bytes, `5d3871cae77db2287807e8914fd21f76203e73aa3fec20955489d45cb4571d8f` |
| **Libbet and the Magic Floor** (0.08) | Game Boy | Damian Yerrick (after Martin Korth's Magic Floor) | zlib | [libbet.gb](https://github.com/pinobatch/libbet/releases/download/v0.08/libbet.gb), 32768 bytes, `3607412031c8287cf878299ce96e581e85b852dde703806343b95576fa3ff1a9` |
| **Thwaite** (0.04) | NES | Damian Yerrick | GPL-3.0-or-later | [thwaite.nes](https://github.com/pinobatch/thwaite-nes/releases/download/v0.04/thwaite.nes), 24592 bytes, `a2df24d9c9f72e56c2fdc4c703becc47a5700ad0158da8208247635ebeb3779c` |
| **Concentration Room** (0.02a) | NES | Damian Yerrick | GPL-3.0-or-later, with permission to copy the game as published without its source | [croom.nes](https://github.com/pinobatch/croom-nes/releases/download/v0.02a/croom.nes), 24592 bytes, `2ce17df1ad66a8a0533c0a8739f5b5ebe275c264924bbe350c42c5ac0394f20e` |
| **Elevator Madness DX** (1.03) | Super Nintendo | The UnDisbeliever | MIT | [Elevator.Madness.DX.v1.03.zip](https://github.com/undisbeliever/elevator-madness-dx/releases/download/v1.03/Elevator.Madness.DX.v1.03.zip), 20374 bytes, `aaba4a93cc4ed62436228f7c140b67d51cc22dc210c452d54cced191f590d56b` (a zip; the ROM inside: `fef374b41df826b30ae5bfd6d73aa4611efbf49e360b86d121ddd307b9bd7b6e`) |
| **Pipes** (1.02) | Super Nintendo | The UnDisbeliever | MIT | [Pipes.v1.02.zip](https://github.com/undisbeliever/pipes/releases/download/v1.02/Pipes.v1.02.zip), 14063 bytes, `af8f73313de08a298ae937a457c894598605880b3980ce08320a58f9666ef49e` (a zip; the ROM inside: `2675243671ceec5ac9688e2d78c6c0948d341840d5bee8f576f97d5ced043dca`) |
| **KleleAtoms MD** (1.2.1) | Mega Drive | Nightwolf-47 | MIT | [kleleatoms-md-121.bin](https://github.com/Nightwolf-47/KleleAtoms-MD/releases/download/v1.2.1/kleleatoms-md-121.bin), 262144 bytes, `d0660df5af9d39696272b0c6774f25f296ee38e86eacf8b52645eab3ea8b003a` |
| **Attack on Voxelburg** (jam) | Game Boy Advance | John Tsiombikas (nuclear) | GPL-3.0-or-later | [voxelburg_jam.zip](https://github.com/MutantStargoat/voxelburg/releases/download/jam/voxelburg_jam.zip), 1189517 bytes, `e31811acda504a410b10d4824c0f2fc2e48b09c3383348994a9571d89e397b35` (a zip; the ROM inside: `9522f3e7a89640616c5397cf259785a85839692df9099a6caf73cc8a422a80ec`) |

### Licence notices

Each is quoted from the page named. `[...]` marks words left out.
The games are not changed by pocket-ui: the file on the card is the author's release, byte for byte (the SHA-256 above is checked
before it is placed; for a zip, the ROM inside is checked too). Source code: the "Source" link of each.

**uCity** (Antonio Niño Díaz (AntonioND), 2018). Licence: GPL-3.0-or-later; art and music CC BY-SA 4.0. Source: <https://github.com/AntonioND/ucity>. Page: <https://github.com/AntonioND/ucity/blob/master/readme.rst>.

> This game is licensed under the GPLv3+ license. [...] The media files (graphics and music) are licensed under a Creative Commons license (CC BY-SA 4.0).

**Tobu Tobu Girl** (Tangram Games (Simon Larsen), 2017). Licence: MIT (code); art, text, sound and music CC BY 4.0. Source: <https://github.com/SimonLarsen/tobutobugirl>. Page: <https://github.com/SimonLarsen/tobutobugirl#license>. File hosted by: Homebrew Hub (gbdev/database), pinned commit; the author's own build is only on itch.io, which has no fixed file address.

> The source code for Tobu Tobu Girl is licensed under the MIT License. [...] All assets (images, text, sound and music) are licensed under the Creative Commons Attribution 4.0 International License.

**Libbet and the Magic Floor** (Damian Yerrick (after Martin Korth's Magic Floor), 2018). Licence: zlib. Source: <https://github.com/pinobatch/libbet>. Page: <https://github.com/pinobatch/libbet/blob/master/LICENSE>.

> This program is free software. Permission is granted to use it subject to the terms of the zlib License. [...] Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely.

**Thwaite** (Damian Yerrick, 2011). Licence: GPL-3.0-or-later. Source: <https://github.com/pinobatch/thwaite-nes>. Page: <https://github.com/pinobatch/thwaite-nes/blob/master/LICENSE.txt>.

> This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

**Concentration Room** (Damian Yerrick, 2010). Licence: GPL-3.0-or-later, with permission to copy the game as published without its source. Source: <https://github.com/pinobatch/croom-nes>. Page: <https://github.com/pinobatch/croom-nes/blob/master/README.md#legal>.

> The accompanying program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License, version 3 or later. As a special exception, you may copy and distribute exact copies of the program, as published by Damian Yerrick, in iNES or UNIF executable form without source code.

**Elevator Madness DX** (The UnDisbeliever, 2016). Licence: MIT. Source: <https://github.com/undisbeliever/elevator-madness-dx>. Page: <https://github.com/undisbeliever/elevator-madness-dx>.

> The game is licensed under the MIT license. The source code can be found at https://github.com/undisbeliever/elevator-madness-dx

**Pipes** (The UnDisbeliever, 2016). Licence: MIT. Source: <https://github.com/undisbeliever/pipes>. Page: <https://github.com/undisbeliever/pipes>.

> The game is licensed under the MIT license. The source code can be found at https://github.com/undisbeliever/pipes

**KleleAtoms MD** (Nightwolf-47, 2022). Licence: MIT. Source: <https://github.com/Nightwolf-47/KleleAtoms-MD>. Page: <https://github.com/Nightwolf-47/KleleAtoms-MD/blob/master/LICENSE>.

> This game is licensed under the MIT License.

**Attack on Voxelburg** (John Tsiombikas (nuclear), 2022). Licence: GPL-3.0-or-later. Source: <https://github.com/MutantStargoat/voxelburg>. Page: <https://github.com/MutantStargoat/voxelburg/blob/master/README.md#license>.

> This program is free software. Feel free to use, modify, and/or redistribute it under the terms of the GNU General Public License v3, or at your option any later version published by the Free Software Foundation.

### Considered and left out

| Game | Why |
|---|---|
| Nova the Squirrel (NES) | Code GPL-3.0+, but graphics and levels CC BY-NC-SA 4.0: NonCommercial |
| uCity Advance (GBA) | Art CC BY-NC-SA 4.0 |
| Butano Fighter (GBA), MeteoRain (GBA) | Music under NonCommercial (and NoDerivatives) licences |
| Apotris (GBA) | No licence found for its bundled tracker music; its repository's licence file (AGPL) and the Homebrew Hub entry (GPL-3.0) do not agree |
| Skyland (GBA) | Part of its sound and type was bought under a licence made out to its author |
| The Purple Night (GBA) | Code MPL-2.0, but no licence found for the music |
| Celeste Classic ports (GBA) | No licence stated in their Homebrew Hub entries |
| Tobu Tobu Girl Deluxe (GB/GBC) | Not checked; the original is in the pack |
| Cave Story MD, the retrobrews collections | Need the original game's data; the collections' ROMs are approved for their own project only |
| Sure Instinct (SNES) | Source published, but no licence stated |
| Micro Mages | Commercial |

## How a download runs

`src/starter.rs`, in this order (one game):

1. The folder comes from `es_systems.cfg` (`/storage/roms/<system>`); a system the shelf does not have is not offered.
2. A file of that name (any case) is there already: nothing happens ("already on your card").
3. No network address: "The Pocket isn't online" (the address is looked at on the thread, `wifi::ip_now`).
4. Room: the download and the ROM both need space (`statvfs`, plus 2 MB so the card is never filled up): "Not enough room on
   the card: 3 MB free, 5 MB needed".
5. The Store's curl (`store::download_with_cause`: 15 s to connect, given up below 10 kB/s for 30 s, never larger than the list's
   size) into `<data>/starter/<id>.download`. No route to the host is "offline" too, whatever the address said.
6. The size and the **SHA-256** must equal the list's. A zip must hold the ROM with the list's own hash. Else
   "The download is damaged or was changed. Nothing was added."
7. Written into the system's folder with `fsutil::write_atomic` (temporary file, `fsync`, rename), after looking once more for
   a file of that name. The download is removed whatever happened.

Then the library is read again on a thread (`library_rescan`), and the games are on the shelf when the player leaves the page.
"Get all" does the games one after another and stops at the first "offline" or "no room".

Tests: `starter.rs` (the list: free licences only, no NonCommercial, fixed addresses, hashes well-formed, varied systems;
a good download is placed and leaves no work file; a wrong hash and a short file leave nothing; a file that is there is
kept; a full card downloads nothing; offline; zip; free space), `app_starter.rs`, `game_card.rs` (author and licence on the
card), and the German snapshots (`tests/i18n-states.txt`).

## Credits on the game card

X on a starter game shows **By** (author) and **Licence** (`license_short`) in the facts column, even though the game is not
in the offline game list (`app_card.rs`, found by system and file name: `starter::find`). Other games are not affected.

## Keeping it working

- Authors move files: re-run the check below (download, compare the SHA-256) before a release. A changed file never reaches the
  card, but "Nothing was added" is a bad first minute.
- Licences change: the `checked` date in the list is the day they were last read at their sources.

## Adding a game

1. Find the licence in the **author's own words** (README, a LICENSE file, the game's page) and copy the sentence into
   `license_text`. If music or art come from somewhere else, find their licences too. NonCommercial, NoDerivatives, "free to
   play but not to share", "all rights reserved", or no statement at all: leave it out.
2. Use a release asset or a commit-pinned file address, never a branch (`master`, `main`, `latest`).
3. Download it once into an empty folder, `sha256sum` it, put size and hash in. For a zip, hash the ROM inside as well.
4. `cargo test --release --lib starter` checks the entry's shape; add the German line for its description in
   `assets/i18n/de.tsv` (the list's text is painted through the same table as the rest).
5. Add the row to this page and move the day in `checked`.
