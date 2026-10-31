# Arcade games

Arcade games work, but they are fussy. An arcade game is a ZIP file, and the ZIP has to be the right kind for the
emulator. Most "my arcade game does not start" cases come down to that. This page explains how the pieces fit.

pocket-ui does not provide arcade games or BIOS files, and it does not download them. Use only games you are allowed to use.

## What plays them

Arcade games run on **RetroArch**, ROCKNIX's standard emulator frontend, with a libretro core. pocket-ui's own host
(pocket-core) does not play them yet: FinalBurn Neo is prepared there, but it has not been checked on the device, so
arcade games are not sent to it. On RetroArch you get one save, and Quit saves the moment; there are no eight slots and
no pocket-ui Options (see [Systems](systems.md#on-retroarch)).

Which core plays a game depends on the folder it is in. These are ROCKNIX's defaults:

| Folder | Plays with by default |
|---|---|
| `roms/arcade`, `roms/mame` | MAME 2003-Plus |
| `roms/fbneo`, `roms/neogeo`, `roms/cps1`, `roms/cps2`, `roms/cps3` | FinalBurn Neo |

Each of these systems can also use the other cores below. Press **START** on a game's card or a console page: the first
row of [Runs on](options.md#runs-on) shows what plays it now, and Left/Right choose another core.

This default matters. A game that is in `roms/arcade` plays with MAME 2003-Plus, even when its ZIP was made for
FinalBurn Neo. Put such games in `roms/fbneo`, or choose FinalBurn Neo under Runs on.

NAOMI and Atomiswave boards are different: they run on ROCKNIX's standalone Flycast. See
[Systems](systems.md#on-standalone-emulators).

## The ZIP has to match the core

An arcade ZIP is not just "the game". It holds the game's chips in the exact files and names that one version of an
emulator expects. Each core expects the set of one version, and the version that matters is the one the set was made
for. A set for another version can be missing a file or hold a changed one. Then the game does not start, or starts
wrong.

| Core | The set it expects |
|---|---|
| FinalBurn Neo | The current FinalBurn Neo set |
| MAME 2003-Plus | The MAME 2003-Plus set (based on MAME 0.78 with additions) |
| MAME 2010 | The set of MAME 0.139 |
| MAME 2015 | The set of MAME 0.160 |
| MAME | The set of the current MAME |
| FB Alpha 2012 | The set of FBA 0.2.97.24 |

This table follows the [libretro arcade guide](https://docs.libretro.com/guides/arcade-getting-started/). "Current"
means a recent set: FinalBurn Neo and MAME change their sets often, so an old collection can fall behind the core.
ROCKNIX 20261001 ships FinalBurn Neo, MAME 2003-Plus and the other cores above. The core's own version is in
RetroArch's menu under Information → Core Information ([Tools](extras.md#tools) opens RetroArch's menu).

To see which version your set was made for, look at what came with it: a `.dat` file, a readme or the name of the
collection. If you cannot tell, try the core that fits the collection's name (FinalBurn Neo for an "FBNeo" set, and so
on).

Do not rename the ZIP and do not unpack it. The emulator finds a game by the **name of the ZIP**, such as `sf2.zip`, and
reads the files inside. Keep every set zipped. `.7z` is listed for these systems, but the libretro guide says it
"might work or not", so use `.zip`.

## Parents, clones and BIOS files

Many games exist in several versions. In the guide's words:

- A **parent** is usually the newest or the world version. It may need a BIOS set.
- A **clone** is an older or changed version of a parent. It often needs its parent's files.
- A **BIOS set** holds a board's firmware, for example `neogeo.zip` for the Neo Geo games.

How much is inside one ZIP depends on how the set was built:

| Set type | What a game's ZIP holds |
|---|---|
| Full non-merged | Everything the game needs, parent and BIOS included. It stands alone. |
| Non-merged | The game and its parent's files. A BIOS set may still be needed. |
| Split | Only what is different from the parent. The parent is a separate ZIP. |
| Merged | A parent and all its clones in one ZIP. Not recommended. |

The libretro guide suggests non-merged for a few games and split for a whole collection. If you copy only a few games,
non-merged or full non-merged saves you hunting for pieces.

With a split set, copy these with the game:

1. The parent's ZIP, when the game is a clone.
2. The BIOS ZIP, when the game needs one (`neogeo.zip` for Neo Geo, `pgm.zip`, `decocass.zip` and others).

Where they go:

- Next to the game, in the same folder (`roms/neogeo/neogeo.zip`). This works for FinalBurn Neo and for MAME
  2003-Plus. A folder does not share its files with another folder: games in `roms/arcade` need their own copy.
- Or, for FinalBurn Neo, in `roms/bios/fbneo/`, once for all folders. `roms/bios` is RetroArch's system folder here.

MAME 2003-Plus says that, unless the set is full non-merged, its BIOS files must be in the game folder. So put the BIOS
ZIPs next to the games for that core.

A BIOS ZIP such as `neogeo.zip` is not a game. Because it is a ZIP in a game folder, it can show up on the shelf; it
does not play.

## Hard disk and disc games (CHD)

A few arcade games come with a `.chd` file, which is the game's hard disk, CD or laser disc. It is not listed as a game
of its own. Put it in a **folder named after the game's ZIP**, in the same folder as the ZIP:

```
roms/mame/
    kinst.zip
    kinst/
        kinst.chd
```

The folder name is the ZIP's name without `.zip`. The libretro guide gives the same layout. A game with a CHD does not
start without it.

## When a game does not start

A card at the top of the shelf says "... didn't start" when the game gave up within a few seconds. Go through these in
order, and change one thing at a time.

1. **Check the set version first.** Press **START** on the game and look at Runs on: which core is it? Does your ZIP
   come from a set made for that core (the table above)? This is the most common cause. Most often the ZIP and the
   core are from different families, such as a FinalBurn Neo ZIP in `roms/arcade`.
2. **Move it or switch the core.** If the set fits another core, choose it under Runs on and **Save changes → For this
   game**. Try the core that fits the set, not every core in turn.
3. **Look for missing files.** Is the parent ZIP of a clone in the same folder? Is the BIOS ZIP in the folder or in
   `roms/bios/fbneo`?
4. **Check the ZIP itself.** The name must be as the set made it. It must still be zipped. Copy it again if you suspect a
   damaged copy.
5. **Hard disk games:** is the CHD in a folder named after the ZIP?

A game that runs but looks or sounds wrong often comes from a set that is close to the core's version, but not the same.
Use the set the core expects.

The two main cores differ in age. MAME 2003-Plus is older and light on the processor, and FinalBurn Neo is newer. Its
information file calls FinalBurn Neo the first choice for most people who play arcade games through libretro.

## Vertical games

Many arcade games were made for a screen that stands upright (vertical shooters, Pac-Man, Galaga). On RetroArch they
show upright and small, with black on both sides. pocket-ui's [Screen orientation](options.md#screen-orientation) is
for games on its own host; on RetroArch, the emulator's own settings turn the picture. Open RetroArch's menu during the
game with **SELECT + X** (ROCKNIX's RetroArch shortcut), then:

- **FinalBurn Neo** (`roms/fbneo`, `roms/neogeo`, `roms/cps1` to `cps3`): Quick Menu → Core Options → **Vertical
  mode** → Enabled, or Alternate for the other direction. The picture turns to fill the screen; hold the Pocket turned
  so that it stands upright for you.
- **MAME 2003-Plus** (`roms/arcade`, `roms/mame`): Quick Menu → Core Options → **TATE Mode** → Enabled.
- **Any core**: RetroArch's own menu, under Video → Output → **Video Rotation**, turns the picture in quarter turns.

These turn the picture, not the controls. So that up on the D-pad is up in the turned game, change the directions
under Quick Menu → Controls → Port 1 Controls (with the D-pad on top, the D-pad's up becomes Right, right becomes Down, down
becomes Left and left becomes Up). ROCKNIX does this for MAME 2003-Plus by itself when TATE mode is set in its own
per-game settings (EmulationStation's game settings, kept in `system.cfg`). Save the core options for the game
(Quick Menu → Core Options → Manage Core Options → Save Game Options) so they stay for this game alone.

## What has been checked

Everything on this page comes from ROCKNIX's own system list for this build (20261001), the cores' information files and
the libretro guide; the option names under Vertical games come from the cores themselves and from ROCKNIX's start
script. **No arcade game has been run through pocket-ui on the device yet.** If an instruction here does
not match what you see, trust what you see and tell us.

See also: [Systems](systems.md), [Troubleshooting](troubleshooting.md), [Patches and MSU-1](patches-msu1.md).
