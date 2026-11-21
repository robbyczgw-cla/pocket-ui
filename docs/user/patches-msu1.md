# Patches, translations and MSU-1

Fan translations and ROM hacks usually ship as a small **patch file** (`.ips`, `.bps` or `.ups`) rather than a full ROM.
Applying the patch transforms your own copy of the original game into the translated or modded version.

pocket-ui never includes games or patches; you supply your own ROM and a patch built for that exact release.

There are two ways to play patched games; Way 1 is effortless.

## Way 1: put the patch next to the game

pocket-ui's own player (pocket-core) soft-patches games on the fly at launch, just like RetroArch. Drop the patch **into
the same folder with the exact same filename** as your ROM:

```
roms/snes/Game Name.sfc
roms/snes/Game Name.ips
```

Launch the game normally. Your original ROM on the SD card stays untouched; the patch applies cleanly in memory every
time you play.

- Supports `.ips`, `.bps` and `.ups`. If multiple formats share the same name, priority order is `.ips` → `.bps` →
  `.ups`.
- Stack multiple patches using `Game Name.ips`, `Game Name.ips1`, `Game Name.ips2`, etc., applied in order (no gaps in
  numbering, or later files are skipped).
- Works with `.zip` ROMs too: match the archive name (`Game Name.zip` + `Game Name.ips`).
- Patch files stay hidden from your shelf automatically.
- The game card lists **Patch: Game Name.ips** under "Your game", and [Pixel Path](pause-menu.md) displays
  **Patched: Game Name.ips** at the emulator stage.

### If the patch does not fit

`.bps` and `.ups` files verify checksums against the exact ROM they expect. If your ROM is a different region, revision
or has an extra header, pocket-ui refuses to launch a broken game and tells you clearly:

"The patch Game Name.bps is for another version of the game. Use the version it was made for, or take the patch out of
the folder to play the original."

Grab the matching ROM version noted in the patch's readme. Note that older `.ips` files have no checksum check, so a
mismatched `.ips` can cause crashes or garbled graphics.

Corrupt patches or patches that exceed size limits also stop launch with a clear error; pocket-ui never silently boots
the unpatched ROM when a patch is sitting beside it. To play the unpatched game, move the patch elsewhere.

### What can't be patched this way

Disc images (`.cue`, `.chd`, `.iso`, `.m3u`, etc.), arcade ZIPs and ScummVM games don't support on-the-fly patching. If a
patch sits beside one, pocket-ui stops and tells you. Patch those permanently on a computer instead (Way 2).

### Saves and achievements

- Save states are tied to the exact patched build. Adding, updating or removing a patch starts the game fresh, keeping
  your previous moment under Pause menu → Load until your next Quit. Patches are tracked by name, size and timestamp.
- In-game saves follow the ROM filename, just like RetroArch, meaning a soft-patched game and its unpatched original
  share **one save file**. Want separate saves? Pre-patch on a computer with a unique filename (Way 2).
- [Achievements](achievements.md) check the patched ROM hash, matching RetroArch. Some translations and hacks have their
  own RetroAchievements sets; otherwise no achievements trigger while the patch is active.

## Way 2: patch on a computer, once

Best for disc games, keeping patched saves separate from the original, or playing in standalone emulators:

1. Patch your original ROM on a PC using a free tool like Floating IPS or the browser-based ROM Patcher JS (runs locally
   in your browser). Select your ROM and patch file to save a new patched ROM.
2. Give it a distinct name, like `Game Name (English).sfc`.
3. Copy it into its console folder, or into ROCKNIX's hacks/translations subfolder like `snesh`
   (see [Systems](systems.md)). Both work identically.

Your patched ROM now behaves like its own standalone game with its own separate saves, while your original ROM stays
clean. See [Add games](getting-started.md#5-add-games) for folder paths. If your patcher warns of a checksum mismatch,
the patch requires a different ROM revision.

## On RetroArch

RetroArch soft-patches the same way (matching filename beside the ROM) for cores that load games into memory, including
Snes9x, Gambatte, mGBA and Genesis Plus GX (except Master System). Check the
[libretro softpatching page](https://docs.libretro.com/guides/softpatching/) for other cores.

- Unlike pocket-core, RetroArch falls back to launching the unpatched game with a brief toast if a patch doesn't match.
- Soft-patching is enabled by default in RetroArch (unchanged by ROCKNIX settings; untested on device).
- RetroArch games keep one save without pocket-ui's pause slots or Options ([Systems](systems.md#on-retroarch)).

## MSU-1 (extra music and video for some SNES games)

MSU-1 is a fan-made Super Nintendo expansion that adds CD-quality soundtracks and full-motion video to SNES classics. An
MSU-1 pack includes a patched ROM plus audio track files.

**Untested on device:** these steps follow Snes9x's documentation and ROCKNIX's MSU-1 folder setup. If your hardware
behaves differently, go by what you see.

### What you need

An MSU-1-patched ROM and its audio tracks:

- `Game Name.sfc`: the unzipped MSU-1 ROM (if the pack came with a patch, apply it first using Way 2).
- `Game Name.msu`: the small manifest file.
- `Game Name-1.pcm`, `Game Name-2.pcm`, …: one audio track per file.

All three must share the **exact same base filename** (only the extension and track number differ). Packs often ship
with mismatched names; rename them to match, paying attention to uppercase and lowercase (`Game.msu` and `game.msu` are
different on Linux).

### Where they go

Drop each game into its own subfolder inside `roms/snesmsu1` (ROCKNIX's "Super NES MSU-1", launched via RetroArch's
Snes9x core):

```
roms/snesmsu1/Game Name/
    Game Name.sfc
    Game Name.msu
    Game Name-1.pcm
    Game Name-2.pcm
```

The shelf shows a single clean game named after the folder, hiding the `.msu` and `.pcm` files.

### If there is no sound or no extra music

- Double-check every single letter and capital across all three filenames.
- Make sure the ROM is unzipped and sits right beside its `.msu` and `.pcm` files.
- Keep it in `roms/snesmsu1`, not `roms/snes` (games in `snes` run in pocket-core, which hides one Snes9x MSU-1 option
  and hasn't been tested with MSU-1).
- Press **START** on the game and check [Runs on](options.md#runs-on): it should read RetroArch · Snes9x.

See also: [Arcade games](arcade.md), [Troubleshooting](troubleshooting.md).
