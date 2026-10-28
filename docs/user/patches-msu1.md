# Patches, translations and MSU-1

Fan translations and ROM hacks usually come as a small **patch file**, not as a game. A patch has the ending `.ips`,
`.bps` or `.ups`. It changes your own copy of the original game into the translated or changed one.

pocket-ui does not provide patches or games. You need your own copy of the original game, and a patch made for
exactly that version.

There are two ways to play the patched game. The first one is the simplest.

## Way 1: put the patch next to the game

pocket-ui's own host (pocket-core) applies a patch when the game starts, as RetroArch does. Put the patch **next to
the game, with the same name**:

```
roms/snes/Game Name.sfc
roms/snes/Game Name.ips
```

Start the game as usual. The game file on the SD card is not changed; the patch is applied each time the game starts.

- `.ips`, `.bps` and `.ups` all work. If there are several with the game's name, `.ips` is used first, then `.bps`,
  then `.ups`.
- Several patches can be stacked as `Game Name.ips`, `Game Name.ips1`, `Game Name.ips2` and so on. They are applied in
  this order. The numbers have to follow each other: after a gap, the rest is left out.
- A zipped game works too. The patch has the zip's name: `Game Name.zip` and `Game Name.ips`.
- The patch file itself is not shown on the shelf, because it is not a game.
- The game's card shows the patch under "Your game", as **Patch: Game Name.ips**. While the game runs,
  [Pixel Path](pause-menu.md) in the pause menu says **Patched: Game Name.ips** at the emulator.

### If the patch does not fit

A `.bps` or `.ups` patch knows which game it was made for. If your file is another version (another country or
revision, or a file with an extra header), the game does not start, and the shelf says:

"The patch Game Name.bps is for another version of the game. Use the version it was made for, or take the patch out
of the folder to play the original."

Look for the version the patch names. An `.ips` patch cannot check this: a patch for the wrong version can give a
broken game.

A damaged patch, or one that would make the game too big, also stops the start with a message. The game never silently starts
as the original while a patch is next to it. To play the original, move the patch to another folder.

### What can't be patched this way

Disc games (`.cue`, `.chd`, `.iso`, `.m3u` and the like), arcade ZIPs and ScummVM games are not patched. With a patch
next to one, the game does not start and the shelf says so. Patch those on a computer (Way 2 below).

### Saves and achievements

- The save belongs to the game file's name, as in RetroArch. A patched game and its original in the same folder,
  with the same name, share **one save**. To keep them apart, patch on a computer and give the new file a name of its
  own (Way 2 below).
- [Achievements](achievements.md) look at the patched game, not at your original file, as RetroArch does. Some
  translations and hacks have their own achievements on RetroAchievements; for any other patched game there are none,
  and your original's achievements are not counted while it is patched.

## Way 2: patch on a computer, once

Use this for disc games, to keep a patched game's saves apart, or to play the patched game in another emulator.

1. On a computer, apply the patch to your original game file with a patcher. Free ones are Floating IPS and the web-based
   ROM Patcher JS, which runs in a browser and keeps the file on your computer. Choose the original file and the patch.
   The patcher writes a new file.
2. Give the new file a name of its own, such as `Game Name (English).sfc`.
3. Copy the new file to the Pocket, into the system's folder or into its hacks folder. ROCKNIX has folders for hacks and
   translations, such as `snesh` (see [Systems](systems.md)). Both are read the same way.

The patched game is then an ordinary game, and it keeps its own saves: the save belongs to the file's name, so the
translation and the original do not share a save. Keep the original under its own name. See
[Add games](getting-started.md#5-add-games) for where files go.

If the patcher says the checksum or the file does not match, the patch is for another version of the game.

## On RetroArch

RetroArch applies patches itself in the same way (it calls this softpatching), with the same names next to the game.
It works only with cores that load the game from memory; the libretro list marks Snes9x, Gambatte, mGBA and Genesis
Plus GX (not for Master System) as working. Check the
[libretro softpatching page](https://docs.libretro.com/guides/softpatching/) for your core.

- When a patch does not fit, RetroArch starts the original game instead, with only a short message. pocket-core
  stops instead.
- RetroArch's softpatching is usually on. ROCKNIX's RetroArch settings do not change that. It was not tried on the
  device.
- Games run on RetroArch have one save, no pocket-ui slots or Options ([Systems](systems.md#on-retroarch)).

## MSU-1 (extra music and video for some SNES games)

MSU-1 is an add-on for the Super Nintendo. Fan projects use it to add better music or video to a game, for
example a remade soundtrack. A pack for it has a game made for MSU-1 and extra files for the music.

**This has not been checked on the device.** The recipe below follows the Snes9x core's documentation and ROCKNIX's
own MSU-1 system. If something differs, trust what you see.

### What you need

A game that is already MSU-1-ready, and the music files:

- `Game Name.sfc`: the game. If the pack comes with a patch for the original game, use Way 2 above to make it. Do not
  zip it.
- `Game Name.msu`: a small data file.
- `Game Name-1.pcm`, `Game Name-2.pcm` and so on: the music, one file per track.

The three kinds of file share **one name**, only the ending differs. The number after the dash is the track number.
Packs often come with other names. Rename the files, because the core finds the music by the game's name. Capital letters
count; the Pocket tells `Game.msu` from `game.msu`.

### Where they go

The folder for this is `roms/snesmsu1` (ROCKNIX's "Super NES MSU-1"). It plays with RetroArch and its Snes9x core.
Put one game's files in a folder of their own:

```
roms/snesmsu1/Game Name/
    Game Name.sfc
    Game Name.msu
    Game Name-1.pcm
    Game Name-2.pcm
```

The shelf then shows one game, named after the folder. The `.msu` and `.pcm` files are not shown.

### If there is no sound or no extra music

- The three names must be the same. Check each letter.
- The game has to be unzipped, with the files next to each other.
- The folder is `snesmsu1`. A game in `roms/snes` plays on pocket-ui's own host. That host also uses Snes9x, but pocket-ui
  hides one of the core's MSU-1 options, and MSU-1 has not been checked there.
- Press **START** on the game and look at [Runs on](options.md#runs-on): it should say RetroArch · Snes9x.

See also: [Arcade games](arcade.md), [Troubleshooting](troubleshooting.md).
