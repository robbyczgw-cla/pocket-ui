# Notice: what pocket-ui contains and uses

pocket-ui's own code, documents and drawings are licensed under the **GNU General Public License,
version 2 or (at your option) any later version** (see [LICENSE](LICENSE)).

This file lists everything that is *not* ours, or that pocket-ui starts or downloads: under which licence, and
where it comes from. Every entry was checked against the repository or the upstream source in October 2026. Where
something could not be established it says **UNKNOWN**; nothing is guessed.

Like ROCKNIX, pocket-ui is made of and works with many open-source components, and **each keeps its own licence**.
Some of the emulators that ROCKNIX ships, and that pocket-ui starts or loads, are licensed for **non-commercial use
only**. pocket-ui itself contains none of them.

Contents: [ROCKNIX](#rocknix) | [Emulators and cores](#emulators-and-libretro-cores) |
[System pictures](#system-pictures) | [Fonts](#fonts) | [Sounds, shaders, test files](#sounds-shaders-and-test-files) |
[Data sources](#data-sources) | [Rust crates](#rust-crates) | [No games, no BIOS](#no-games-no-bios)

## ROCKNIX

pocket-ui runs on top of [ROCKNIX](https://rocknix.org) ([source](https://github.com/ROCKNIX/distribution)),
the Linux distribution for handheld game consoles, and is **based on ROCKNIX**. It is an independent project and
is **not endorsed by, or affiliated with, the ROCKNIX team**. ROCKNIX is itself a fork of
[JELOS](https://github.com/JustEnoughLinuxOS/distribution); its README says that all licences apply and that the
JELOS team is credited.

Licences, as ROCKNIX states them in its `README.md` and `LICENSE.md` (read 2026-10-22):

| Part | Licence |
|---|---|
| ROCKNIX's original software and scripts | GNU GPL version 2 (ROCKNIX's text says "GNU GPL Version 2"; the file adapted below carries `GPL-2.0-or-later`) |
| ROCKNIX branding and images (name, logo, splash, artwork) | CC BY-NC-SA 4.0 |
| All other software in the distribution | each component's own licence; "includes components licensed for non-commercial use only" |

What this repository takes from ROCKNIX:

- No ROCKNIX image, branding or artwork. `docker/` and `scripts/vdev.sh` unpack an image the developer supplies
  into a local virtual device; the unpacked parts are git-ignored and are not ours to share.
- `deploy/portmaster-setup.sh` is a shortened adaptation of ROCKNIX's `start_portmaster.sh`
  (`SPDX-License-Identifier: GPL-2.0-or-later`, Copyright (C) 2022-present JELOS). It keeps the same steps
  without opening the PortMaster GUI. The original is GPL-2.0-or-later, the same licence as pocket-ui, so it is
  used here under the same terms; the file carries the original SPDX line and copyright notice.
- Test fixtures `src/fixtures/dolphin-wii-{classic,hremote,nunchuck,vremote}.ini` and
  `src/fixtures/dolphin-gcpad-{south,east}.ini` are unchanged copies of ROCKNIX 20261001's Dolphin controller profiles
  (`/usr/config/dolphin-emu/WiiControllerProfiles/*.ini`, `GamecubeControllerProfiles/GCPadNew.ini.{south,east}.stacked`),
  ROCKNIX's software under the GPL like pocket-ui. The unit tests read them to prove that pocket-ui's button chords
  never press a Wiimote button; they are not shipped.
- At run time pocket-ui starts ROCKNIX's own scripts (`runemu.sh`, `start_es.sh`) and emulators. They stay
  ROCKNIX's and are not part of this repository.

## Emulators and libretro cores

None of the programs in this section is part of this repository, and pocket-ui does not ship them. They come
with ROCKNIX; `pocket-core` loads libretro cores at run time from `/usr/lib/libretro`, and pocket-ui starts the
other emulators as separate programs through ROCKNIX's `runemu.sh`.

**Libretro cores that pocket-core loads** (`src/hostsys.rs`: `HANDHELD_CORES` and `SYSTEMS`). The verdicts below come
from the source of each core at the commit ROCKNIX 20261001 builds (`PKG_VERSION` in
[`projects/ROCKNIX/packages/emulators/libretro/<core>-lr/package.mk`](https://github.com/ROCKNIX/distribution/tree/20261001/projects/ROCKNIX/packages/emulators/libretro)):
the upstream licence file and the licence lines in the source files, not only the tag in ROCKNIX's package file.
Checked 2026-10-22; the same table is machine-readable in `scripts/core-licences.txt`
(`scripts/check-core-licences.sh` lists the cores that are GPL-2.0-only). "Default" means pocket-ui routes the system
to pocket-core out of the box (`verified` in `hostsys.rs`; the rest stay on RetroArch unless a test switch is set).
The source archives were read without their submodules.

Verdicts: **GPL-2.0-or-later** (the files say "version 2 ... or (at your option) any later version"),
**GPL-3.0-or-later**, **permissive** (MIT, MPL-2.0, zlib), **non-commercial**, **GPL-2.0-only** or **mixed, with
GPL-2.0-only code** (a project that is mostly "or later" but has files that say "version 2" only).

| Core | Systems | ROCKNIX 20261001 builds | Verdict | Evidence (file: licence line) | Default |
|---|---|---|---|---|---|
| [gambatte](https://github.com/libretro/gambatte-libretro/tree/d9d6cd06382d1ced30de34d56d3609452323dab1) | Game Boy, Game Boy Color | `d9d6cd0` | **GPL-2.0-only** | `libgambatte/include/inputgetter.h`: "GNU General Public License version 2 as published by the Free Software Foundation"; `COPYING` is GPLv2, no file says "or later". Libretro glue part MIT (blipper) | yes |
| [mgba](https://github.com/libretro/mgba/tree/e31759b24e7a4e3899285ff720d7b573ac328ae7) | Game Boy Advance | `e31759b` | permissive (MPL-2.0) | `src/core/core.c`: "subject to the terms of the Mozilla Public License, v. 2.0"; bundled libpng, zlib, inih, lzma, sqlite3 are permissive | yes |
| [nestopia](https://github.com/libretro/nestopia/tree/b08b473f961e057216bc9e8c9d61054b1ab116cf) | NES | `b08b473` | GPL-2.0-or-later | `source/core/NstFds.hpp`: "version 2 of the License, or (at your option) any later version"; 618 files like it. The libretro glue has no header, `COPYING` is GPLv2 | yes |
| [snes9x](https://github.com/libretro/snes9x/tree/890b5d445538fe790aa3add3d5702c80f551e0ae) | SNES | `890b5d4` | **non-commercial** | `LICENSE`: "Snes9x is freeware for PERSONAL USE only" | yes |
| [genesis_plus_gx](https://github.com/libretro/Genesis-Plus-GX/tree/b7e79b3641eb2e322c5ba29903352e115887e83c) | Mega Drive / Genesis, Sega CD | `b7e79b3` | **non-commercial** | `LICENSE.txt`: "Redistributions may not be sold, nor may they be used in a commercial product or activity" | Mega Drive yes, Sega CD no |
| [gearsystem](https://github.com/drhelius/Gearsystem/tree/2c725424715dd41a5d6f8e97a4d318316cf9dbad) | Master System, Game Gear, SG-1000 | `2c72542` | GPL-3.0-or-later | `platforms/libretro/libretro.cpp`: "either version 3 of the License, or any later version"; blargg's `Stereo_Buffer` GPL-2+, `Blip_Buffer`/`Sms_Apu` LGPL-2.1+ | yes |
| [picodrive](https://github.com/libretro/picodrive/tree/733c711a477a642fd2006d5a7a581b2790ec36b4) | Sega 32X | `733c711` | **non-commercial** | `COPYING`: "Redistributions may not be sold, nor may they be used in a commercial product or activity" (MAME-style); a few GPL-2+ parts (DrZ80) | no |
| [beetle_pce_fast](https://github.com/libretro/beetle-pce-fast-libretro/tree/bebe2b13a840fbfd45cb501f5cd0efe01bdb1735) | PC Engine / TurboGrafx-16 (+CD) | `bebe2b1` | GPL-2.0-or-later | `mednafen/mednafen-endian.c`: "either version 2 of the License, or (at your option) any later version"; `COPYING` is GPLv2, `libretro.c` has no header | PC Engine yes, CD no |
| [beetle_supergrafx](https://github.com/libretro/beetle-supergrafx-libretro/tree/3c6fcd3deded54ebecd69408f108407ac03d11b5) | SuperGrafx | `3c6fcd3` | GPL-2.0-or-later | `libretro.cpp`, `mednafen/pce_fast/pce.cpp`: "either version 2 of the License, or (at your option) any later version"; `COPYING` is GPLv2; sampled, not a full audit | yes |
| [beetle_vb](https://github.com/libretro/beetle-vb-libretro/tree/83ed42608601fb7b01d41e4f8fb2007a37b8c84e) | Virtual Boy | `83ed426` | GPL-2.0-or-later | `libretro.cpp`, `mednafen/hw_cpu/v810/v810_cpu.cpp`: "either version 2 of the License, or (at your option) any later version"; `COPYING` is GPLv2; sampled, not a full audit | yes |
| [beetle_ngp](https://github.com/libretro/beetle-ngp-libretro/tree/a50d5ac288a81f2104ddf43195a4efdd15c72227) | Neo Geo Pocket | `a50d5ac` | GPL-2.0-or-later | `libretro.c`: "either version 2 of the License, or (at your option) any later version" | no |
| [beetle_wswan](https://github.com/libretro/beetle-wswan-libretro/tree/4b01295838ea89e3f1355bbe4cb5cf98aa6108cd) | WonderSwan | `4b01295` | GPL-2.0-or-later | `mednafen/state.c`: "either version 2 of the License, or (at your option) any later version"; `v30mz.c` is permissive (credit the author) | yes |
| [handy](https://github.com/libretro/libretro-handy/tree/bc55d462f0b2d6b073ea93dc552ebd73cec60fd1) | Atari Lynx | `bc55d46` | permissive (zlib) | `lynx/ram.h`: "Permission is granted to anyone to use this software for any purpose"; blip parts LGPL-2.1+ / GPL-2+; no top-level licence file | no |
| [stella](https://github.com/stella-emu/stella/tree/c1ffb833c8b180433b0cad76bb6b55f8dfbc46ee) | Atari 2600 | `c1ffb83` | GPL-2.0-or-later | `Copyright.txt`: "either version 2 of the License, or any later version"; source files only point to `License.txt` (the GPLv2 text) | yes |
| [prosystem](https://github.com/libretro/prosystem-libretro/tree/8a88014287c7a01cd568067e5a557d0a2b2a051f) | Atari 7800 | `8a88014` | GPL-2.0-or-later | `core/Sally.h`: "either version 2 of the License, or (at your option) any later version"; `License.txt` is GPLv2 | no |
| [pokemini](https://github.com/libretro/pokemini/tree/132111b76343559860532a1ccc094f93f1ed5650) | Pokemon Mini | `132111b` | GPL-3.0-or-later | `LICENSE`: "either version 3 of the License, or (at your option) any later version" | no |
| [pcsx_rearmed](https://github.com/libretro/pcsx_rearmed/tree/da2cb8ecd17fd0932ab6d94774c0522beebce6e3) | PlayStation | `da2cb8e` | GPL-2.0-or-later (+ LGPL-3.0-or-later part) | `frontend/main.h`: "either version 2 of the License, or (at your option) any later version"; bundles GNU Lightning (LGPL-3.0-or-later, used by the lightrec dynarec), so a binary with it is GPLv3 only | yes |
| [fbneo](https://github.com/libretro/FBNeo/tree/f3b774987e009d07f1322ebc4910532ed5b8c808) | Neo Geo, CPS1-3, arcade | `f3b7749` | **non-commercial** | `src/license.txt`: "You may not sell, lease, rent or otherwise seek to gain monetary profit"; also subject to the MAME licence | no |
| [mupen64plus_next](https://github.com/libretro/mupen64plus-libretro-nx/tree/f275caf4b2bfa1e6d1c51636746ea793f3d80320) | Nintendo 64 | `f275caf` | **mixed, with GPL-2.0-only code** | `GLideN64/LICENSE`: "GLideN64 is licensed under the GNU General Public License version 2" (its own files carry no header). Glue and `mupen64plus-core` files are "version 2 ... or any later version"; also GPL-3.0-only xBRZ, LGPL-3.0 gles2n64 and CC0/MIT parts | yes |
| [flycast](https://github.com/flyinghead/flycast/tree/c3763d8fc4208dd6f8f0bc456383543b8406a8a0) | Dreamcast | `c3763d8` | GPL-2.0-or-later (+ GPL-3.0 parts) | `core/cheats.cpp`, `shell/libretro/libretro.cpp`: "either version 2 of the License, or (at your option) any later version"; bundles GPL-3.0-only xBRZ and GPL-3+ `iso9660.h`, so a binary with it is GPLv3 only | yes |
| [ppsspp](https://github.com/hrydgard/ppsspp/tree/fa50bb1976065c4f8b1b47af227d367fe9771555) | PSP | `fa50bb1` | **mixed, with GPL-2.0-only code** | `Common/CPUDetect.cpp`: "the Free Software Foundation, version 2.0." (no "or later"; Dolphin-derived, also `Arm64Emitter.cpp`, `PrxDecrypter.h`); most other files say "version 2.0 or later versions"; GPL-3.0-only xBRZ | yes |
| [melondsds](https://github.com/JesseTG/melonds-ds/tree/bc4e4b67d2d470d7c682810a1e892cafd6f9082b) | Nintendo DS | `bc4e4b6` | GPL-3.0-or-later | `src/libretro/exceptions.cpp`: "either version 3 of the License, or (at your option) any later version" | yes |
| [yabasanshiro](https://github.com/libretro/yabause/tree/39535a6abcad5abf9f71c8b2a7975f005ee12ed6) | Sega Saturn | `39535a6` | GPL-2.0-or-later | `yabause/src/yabause.c`: "either version 2 of the License, or (at your option) any later version"; GPL-2-only files are macOS/Qt-only | yes |
| [dosbox_pure](https://github.com/schellingb/dosbox-pure/tree/7f6e8fb7385fa446d1444d671063268520bf9b54) | DOS | `7f6e8fb` | GPL-2.0-or-later | `dosbox_pure_libretro.cpp`: "either version 2 of the License, or (at your option) any later version"; README: "version 2 or later" | yes |
| [scummvm](https://github.com/libretro/scummvm/tree/292e409938384ac0b3819a336c61fbb71dcbb9c3) | ScummVM | `292e409` | GPL-3.0-or-later | `video/avi_decoder.h`: "either version 3 of the License, or (at your option) any later version" | yes |
| [fake08](https://github.com/jtothebell/fake-08/tree/814991a2571ad3970e386cef48f3b148aa1c27b9) | PICO-8 | `814991a` | permissive (MIT) | `LICENSE.MD`: "License: MIT" | yes |
| [tic80](https://github.com/nesbox/TIC-80/tree/7020500a6e88f6ee91301933bb77f082a10e10f5) | TIC-80 | `7020500` | permissive (MIT) | `LICENSE`: "MIT License" | yes |

What the table shows:

- ROCKNIX's `PKG_LICENSE` does not match the source for three cores it tags GPL-2.0-or-later: gambatte is GPL-2.0-only in every file,
  and mupen64plus_next and ppsspp contain GPL-2.0-only files (GLideN64, Dolphin-derived `CPUDetect.cpp` and
  `Arm64Emitter.cpp`). libretro's "GPLv2" for the others is only the licence text's version; their files say "or later".
- Several cores bundle GPL-3.0-only or LGPL-3.0 code (xBRZ in flycast, ppsspp and mupen64plus_next, GNU Lightning in
  pcsx_rearmed): a binary with those can only be distributed under GPLv3, not GPLv2. mupen64plus_next and ppsspp mix
  such code with GPL-2.0-only code, so no single GPL version covers them as built; that is an upstream matter.
- The check is `scripts/check-core-licences.sh` (table: `scripts/core-licences.txt`); see "How the licences fit
  together" below.

**Standalone emulators and frontends that pocket-ui starts or talks to** (`src/standalone.rs`,
`src/emu_settings.rs`, `deploy/pine.py`), licence as ROCKNIX tags them:

| Program | Used for | Licence |
|---|---|---|
| [ARMSX2](https://github.com/ARMSX2/ARMSX2) | PlayStation 2 | GPL-3.0-or-later |
| [Dolphin](https://github.com/dolphin-emu/dolphin) | GameCube, Wii | GPL-2.0-or-later |
| [Azahar](https://github.com/azahar-emu/azahar) | Nintendo 3DS | GPL-2.0-or-later |
| [RetroArch](https://github.com/libretro/RetroArch) | every system pocket-ui does not host itself | GPL-3.0-or-later |
| [PortMaster](https://github.com/PortsMaster/PortMaster-GUI) | PC game ports (`src/ports.rs`) | MIT (each port has its own licence) |

Every other emulator ROCKNIX ships (and pocket-ui can start through `runemu.sh`) keeps its own licence; see
ROCKNIX.

**How the licences fit together.** pocket-ui is GPL-2.0-or-later, so it can be combined with GPL-2.0 code and,
by the "or later", with GPL-3.0 code. The cores are not part of this repository or of the pocket-ui programs:
`pocket-core` loads the ones ROCKNIX installs, at run time, the same arrangement as RetroArch and other libretro
frontends. Three of them contain GPL-2.0-only code (gambatte, parts of mupen64plus_next and ppsspp). Four Rust crates
that pocket-ui and pocket-core link (`winit`, `glutin`, `glutin_egl_sys`, `dpi`) are Apache-2.0, which the FSF
counts as compatible with GPLv3 but not with GPLv2-only code. We state this so you can judge it yourself. If you hold
rights in one of these components and see a problem, please open an issue; we would rather change a default than
argue.

**Non-commercial cores.** snes9x, genesis_plus_gx, picodrive and fbneo may not be used commercially. pocket-ui
itself is not restricted by them, since it does not contain them, but an image or a device that bundles them
cannot be sold, and anyone who redistributes such an image must keep to those terms. A release image meant for
anything beyond private and non-commercial use has to be built without them.

## System pictures

The pixel-art console pictures in `assets/systems/` (compiled into pocket-ui by `src/system_art.rs`) are
described in full, file by file, in [assets/systems/SOURCES.md](assets/systems/SOURCES.md). In short:

- **Photos:** 36 photographs by Evan Amos on Wikimedia Commons, each marked **public domain** on its Commons file
  page (licence read from the Commons API and the file pages on 2026-10-22, for the last 15 on 2026-10-27).
- **Conversion:** each photo was converted to pixel art with [PixelLab](https://www.pixellab.ai)
  (`image-to-pixelart`), then cut out and cleaned with our own scripts in `assets/systems/tools/`. Every logo,
  brand name and model lettering was painted over, so the pictures carry no trademarks.
- **Generated from text:** `supergrafx.png` and `pokemini.png`: no public-domain photo of these two exists, so they were
  made with PixelLab (`create-image-pixflux`) from text prompts alone, no photo and no input image, on 2026-10-27
  (prompts and seeds in `assets/systems/SOURCES.md` and `pixellab-assets.json`). They are in the style of the others
  and not exact pictures of the hardware; they carry no logo.
- **Drawn for this project:** `pico-8.png` and `ports.png` (a generic cartridge and a folder of cartridges), made
  with `tools/draw_generic.py`. They copy no real product.
- **PixelLab's terms** ([Terms of Service](https://www.pixellab.ai/termsofservice), last updated 2025-11-23, read
  2026-10-22): the user owns the copyright in what is created with PixelLab and may use, modify and distribute
  it for any purpose, commercial use included (sections 1.3 and 3.3). The exception is training other models on
  the output without PixelLab's written permission (sections 1.2 and 3.3). The user is responsible for the
  rights in the input images (here: public-domain photos). By using the service the user also agrees to the
  Open RAIL-M licence (section 4.1).
- **Licence of the result:** the pictures are the project's own derivative work of public-domain photos and ship
  with pocket-ui under GPL-2.0-or-later. PixelLab's restriction on training other models is a term between
  PixelLab and its user; it is not part of the GPL and is not passed on to recipients.

The raw material of the first experiments (converted photos with their logos still on, and text-generated console
props) is not part of the public repository, and nothing in the program uses it.

## Interface pictures

The small pixel-art pictures of the interface in `assets/ui-art/` (compiled into pocket-ui by `src/ui_art.rs`) are
described file by file in [assets/ui-art/SOURCES.md](assets/ui-art/SOURCES.md). In short:

- **Generated from text prompts:** the empty pages' vignettes and the Year in review's stickers were made with
  PixelLab (`create-image-pixflux`) from text prompts alone, no input image; the prompts and seeds are in
  `assets/ui-art/pixellab-assets.json`. PixelLab's terms as above: the user owns what is created and may
  distribute it for any purpose.
- **Drawn for this project:** the handhelds (`tools/draw_devices.py`, from their published proportions, no photo
  traced, no logos) and the Settings icons (`tools/draw_icons.py`).
- **Licence of the result:** GPL-2.0-or-later, like pocket-ui. No logo, brand name or lettering is in any of them.

## Fonts

| Font | Where | Licence | Notes |
|---|---|---|---|
| Barlow Medium, Barlow Condensed SemiBold (version 1.408) | `assets/fonts/`; compiled into the binaries (`src/text.rs`) | SIL Open Font License 1.1 | Copyright 2017 The Barlow Project Authors ([source](https://github.com/jpt/barlow)), designer Jeremy Tribby. Licence text: `assets/fonts/OFL.txt` |
| Titillium Web Regular, Bold | `store/test-catalog/src/fonts/`; packed into the test store's `type-titillium` package only, not compiled in | SIL Open Font License 1.1 | Copyright (c) 2009-2011 Accademia di Belle Arti di Urbino and students of MA course of Visual design. Licence text: `store/test-catalog/src/fonts/OFL.txt` (also inside the package as `Titillium-OFL.txt`) |
| Noto Sans CJK SC Regular | Not shipped: pocket-ui uses the system's copy if present (ROCKNIX installs it at `/usr/share/fonts/truetype/noto-cjk/`), for Japanese, Chinese and Korean letters (`src/text_fallback.rs`) | SIL Open Font License 1.1 (ROCKNIX's package) | Nothing of it is in this repository or our binaries |
| 8x8 bitmap glyphs (crate `font8x8` 0.3.1) | linked as a Rust crate | MIT | The crate says the glyph data comes from public-domain IBM VGA fonts |

The OFL requires the copyright notice and licence text to travel with every copy of the fonts. For an image,
that means shipping `assets/fonts/OFL.txt` with it.

## Sounds, shaders and test files

- **UI sounds:** eight short sample files (`assets/ui-sounds/`) generated with ElevenLabs sound effects under a paid
  plan, which allows commercial use and redistribution; edited by us. Details in `docs/LICENCES.md`.
- **Shaders** (`src/shaders/`): written for the project; they carry no third-party notice.
- **Test ROMs:** `scripts/testroms.py` writes small original test ROMs (colour bars and a tone) for systems with
  no free game. They are generated, not stored, and are ours.
- **Test store catalog** (`store/test-catalog/`): our own test pattern core (`testpattern.c`), a screen-frame
  pack made by our code, a small shell tool, the Titillium fonts above, and a catalog signed with the store key (the private key lives outside the
  repository).
  The catalog's own metadata still says MIT for the packages we wrote; they are ours and fall under
  GPL-2.0-or-later like the rest.
- **Screenshots** in `docs/` show pocket-ui itself. Some of them show frames of a Game Boy Color game, uCity
  (Antonio Niño Díaz, [AntonioND/ucity](https://github.com/AntonioND/ucity)), licensed GPL-3.0-or-later with
  art and music under CC BY-SA 4.0 (its readme, recorded in `assets/starter-pack.json`). Whether the screenshots meet the
  art licence (attribution, share-alike) has not been assessed.

## RetroAchievements

- **rcheevos** (`vendor/rcheevos/`): the RetroAchievements client library, version 12.5.0 from
  [RetroAchievements/rcheevos](https://github.com/RetroAchievements/rcheevos), **MIT** (`vendor/rcheevos/LICENSE`,
  compatible with GPLv2). Compiled into `pocket-ui` and `pocket-core` by `build.rs`; `vendor/rcheevos/UPSTREAM` says
  which files were taken. `vendor/rcheevos/libretro/libretro.h` is rcheevos' own test copy of the libretro structures,
  same licence.
- **The RetroAchievements service** (`retroachievements.org`): used only when the user signs in with their own
  account (Settings → RetroAchievements). Achievement sets, badges and the player's data come from the site at run
  time and are cached on the device (`/storage/.config/pocket-ui/achievements/`). **Not shipped**; they belong to
  RetroAchievements and its contributors.

## Data sources

- **libretro thumbnails** (box art): downloaded by the user's device at run time from
  `thumbnails.libretro.com` (`src/covers.rs`) and cached there. **Not shipped** with pocket-ui or any image. The
  artwork belongs to its rights holders; the collection states no licence.
- **No-Intro game lists** (recognising a cartridge game by its CRC-32, `src/rom_id.rs`): downloaded by the user's
  device at run time from [libretro-database](https://github.com/libretro/libretro-database) (`metadat/no-intro`,
  CC BY-SA 4.0 as a repository; the lists come from [No-Intro](https://datomatic.no-intro.org)) and kept on the
  card as CRC → name tables. **Not shipped** with pocket-ui or any image.
- **PortMaster catalog** (`ports.json`, `src/ports.rs`): downloaded at run time from
  [PortMaster-New](https://github.com/PortsMaster/PortMaster-New) (MIT) and cached on the device. Ports are
  installed through PortMaster; each port has its own licence. pocket-ui lists ports that need no extra
  game files and ports that need the player's own files (it explains where to copy them and never downloads them).
- **Cheat files** (`.cht`, `src/cheats.rs`): read from `/usr/share/libretro-database/cht`, which ROCKNIX
  ships. Not in this repository.
- **Wikidata** (game facts: year, developer, publisher, genre, series, players, review scores):
  `assets/gamedb.bin`, built by `scripts/build-gamedb.py` from the [Wikidata](https://www.wikidata.org) query
  service. Wikidata's data is released under [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/); the
  game card still names it ("Facts: Wikidata, CC0", scores "via Wikidata").
- **Wikipedia** (the game card's description): **not shipped**. The device fetches the article's lead text when a
  card is opened and caches it under the data folder (`src/wiki_text.rs`). The card shows the article and licence
  ("Text: Wikipedia, CC BY-SA 4.0"). Opening a card lets Wikipedia see which article was asked for.

## Rust crates

127 crates end up in the aarch64 build (115 linked into the programs, 12 only run at build time), 240 in the full
dependency graph (all platforms, including a macOS development build). For every crate with a choice, the
GPLv2-compatible option is the one used: MIT, BSD, ISC, Zlib, Unlicense, 0BSD and Unicode-3.0 qualify; for
`MIT OR Apache-2.0` the MIT side is used. There is no copyleft or non-commercial crate among them.

**Not compatible with GPLv2 (Apache-2.0 only), linked into `pocket-ui` and `pocket-core`:** `winit` 0.30.13,
`glutin` 0.32.3, `glutin_egl_sys` 0.7.1 and `dpi` 0.1.2 (Apache-2.0 AND MIT). Apache-2.0 is fine with
GPL-3.0-or-later but not with GPL-2.0-only (see "How the licences fit together" above). `gl_generator` and
`khronos_api` (also Apache-2.0) only run in a build script of `glutin_egl_sys`; they are not linked, and the EGL
bindings they generate are compiled in.

- The list with name, version and licence: [docs/THIRD-PARTY-CRATES.md](docs/THIRD-PARTY-CRATES.md), generated by
  `scripts/third-party.sh` from `Cargo.lock` (run it again after any dependency change).
- The licence and notice files that the crates' licences require to accompany a binary:
  `scripts/third-party.sh --texts FILE` collects them from the sources of the crates. `gl_generator` and
  `khronos_api` ship no licence file in their packages.

## No games, no BIOS

pocket-ui contains no game, no ROM, no BIOS or firmware file, no key and no emulator binary of a third party. The
source names BIOS files (`src/hostsys.rs`) only so it can tell the user which one is missing; the user supplies
them. `scripts/check-release-tree.py` enforces this before a release.
