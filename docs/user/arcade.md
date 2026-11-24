# Arcade games

Arcade ROMs are awesome once set up, though they're notoriously picky: every arcade game is a ZIP built for a specific
emulator version, and 90% of "my arcade game won't start" problems come down to a mismatched ZIP. Here's how to make
them click.

pocket-ui never includes or downloads arcade ROMs or BIOS files; use only games you own.

## What plays them

Arcade games run through **RetroArch** (ROCKNIX's libretro player). pocket-ui's own player (pocket-core) has FinalBurn
Neo wired up in code but hasn't been hardware-verified yet, so arcade games stay on RetroArch for now. On RetroArch you
get auto-save on Quit and one state, without pocket-ui's eight manual slots or Options
([Systems](systems.md#on-retroarch)).

Your folder decides which core launches by default (ROCKNIX defaults):

| Folder | Plays with by default |
|---|---|
| `roms/arcade`, `roms/mame` | MAME 2003-Plus |
| `roms/fbneo`, `roms/neogeo`, `roms/cps1`, `roms/cps2`, `roms/cps3` | FinalBurn Neo |

Want a different core for a specific game or folder? Press **START** on a game card or console page to open
[Runs on](options.md#runs-on) and switch cores with Left/Right.

Folder choice matters: dropping a FinalBurn Neo ROM into `roms/arcade` launches it with MAME 2003-Plus by default and
usually fails. Either put FBNeo games in `roms/fbneo` or switch the core under **Runs on**.

Sega NAOMI and Atomiswave run on ROCKNIX's standalone Flycast instead; see
[Systems](systems.md#on-standalone-emulators).

## The ZIP has to match the core

An arcade ZIP isn't a single cartridge ROM; it's a bundle of individual arcade board chip dumps named exactly as a
particular emulator version expects. Feed a core a ROM from the wrong set version and it either refuses to boot or
glitches out:

| Core | The set it expects |
|---|---|
| FinalBurn Neo | Current FinalBurn Neo ROMset |
| MAME 2003-Plus | MAME 2003-Plus ROMset (based on MAME 0.78 plus additions) |
| MAME 2010 | MAME 0.139 ROMset |
| MAME 2015 | MAME 0.160 ROMset |
| MAME | Current MAME ROMset |
| FB Alpha 2012 | FBA 0.2.97.24 ROMset |

This follows the [libretro arcade guide](https://docs.libretro.com/guides/arcade-getting-started/). Both FinalBurn Neo
and current MAME evolve over time, so older sets can drift out of sync. ROCKNIX 20261001 ships all the cores above;
check exact core versions in RetroArch under Information → Core Information (open RetroArch via
[Tools](extras.md#tools)).

Not sure which set you have? Check any `.dat` file, readme or folder name that came with your set, and match it to the
core above.

**Never rename or unzip arcade ROMs.** Cores identify games strictly by their short ZIP filename (like `sf2.zip`) and
read the chip dumps inside directly. Stick to `.zip`, as libretro notes `.7z` arcade sets are hit-or-miss.

## Parents, clones and BIOS files

Arcade sets group alternate revisions together:

- **Parent:** usually the primary or World release (may still need a board BIOS).
- **Clone:** a regional variant, bootleg or revision of a parent, often relying on files inside the parent ZIP.
- **BIOS set:** arcade board firmware, like `neogeo.zip` for Neo Geo.

How self-contained each ZIP is depends on the set format:

| Set type | What a game's ZIP holds |
|---|---|
| Full non-merged | Game, parent files and BIOS all packed together. Completely standalone. |
| Non-merged | Game plus its parent files. May still need a BIOS ZIP. |
| Split | Only files unique to that clone; requires the parent ZIP alongside it. |
| Merged | Parent plus every clone packed into one ZIP. Not recommended. |

Per the libretro guide, **non-merged** (or **full non-merged**) is easiest if you're picking individual favourites,
while **split** saves space for huge full sets.

Using a split set? Make sure you copy both:

1. The **parent ZIP** alongside any clone.
2. Any required **BIOS ZIP** (`neogeo.zip` for Neo Geo, `pgm.zip`, `decocass.zip`, etc.).

Where BIOS ZIPs go:

- **Right beside your games** in the same folder (`roms/neogeo/neogeo.zip`). Works for both FinalBurn Neo and MAME
  2003-Plus. Folders don't share BIOS files, so games in `roms/arcade` need their own copy there.
- **Or in `roms/bios/fbneo/`** (FinalBurn Neo only, shared across folders; `roms/bios` is RetroArch's system folder).

MAME 2003-Plus requires BIOS ZIPs inside the game folder unless you're using a full non-merged set.

Note: because `neogeo.zip` is a ZIP inside a ROM folder, it may show up as an entry on your shelf. It's just firmware and
won't launch as a game.

## Hard disk and disc games (CHD)

Some 90s/2000s arcade boards also need a `.chd` file (the cabinet's hard drive, CD or laserdisc). It won't show up as a
separate game; place it inside a **subfolder matching the ZIP's exact name** right beside the ZIP:

```
roms/mame/
    kinst.zip
    kinst/
        kinst.chd
```

Without the matching subfolder and `.chd`, the game won't start.

## When a game does not start

If a game crashes within a few seconds, a card at the top of the shelf says "... didn't start". Check these in order,
changing one thing at a time:

1. **Match the set to the core:** press **START** on the game to check **Runs on**. Does your ZIP match that core's
   table row above? Mixing an FBNeo set in `roms/arcade` (MAME 2003-Plus) is the #1 culprit.
2. **Move or switch cores:** pick the matching core under **Runs on** → **Save changes → For this game** instead of
   guessing cores at random.
3. **Check for missing parent or BIOS ZIPs:** make sure clones have their parent ZIP in the same folder and BIOS ZIPs
   sit beside the game or in `roms/bios/fbneo`. For Neo Geo games in `roms/neogeo`, pocket-ui explicitly flags a missing
   `neogeo.zip` if it's absent from the game folder, `roms/neogeo`, `roms/bios` and `roms/bios/fbneo`.
4. **Verify the ZIP:** keep the exact original filename, keep it zipped, and recopy if it might be corrupt.
5. **CHD games:** confirm the `.chd` sits inside a subfolder named after the ZIP.

Boots up with broken graphics or missing audio? That almost always means a slightly mismatched set version. Use the exact
set built for your core.

Between the two big cores, **MAME 2003-Plus** is older and lighter on modest CPUs, while **FinalBurn Neo** is newer,
more accurate and libretro's top pick for most players.

## Vertical games

Classic shmups and vertical cabinets (Pac-Man, Galaga, DoDonPachi) render narrow with side bars by default on RetroArch.
pocket-ui's [Screen orientation](options-screen.md#screen-orientation) only covers pocket-core; on RetroArch, press
**SELECT + X** mid-game to open RetroArch's menu and rotate there:

- **FinalBurn Neo** (`fbneo`, `neogeo`, `cps1` to `cps3`): Quick Menu → Core Options → **Vertical mode** → Enabled (or
  Alternate to flip the other way).
- **MAME 2003-Plus** (`arcade`, `mame`): Quick Menu → Core Options → **TATE Mode** → Enabled.
- **Any core:** RetroArch menu → Video → Output → **Video Rotation** (90° steps).

Those rotate the image, not the D-pad. Remap directions under Quick Menu → Controls → Port 1 Controls so Up matches how
you hold your handheld (with D-pad on top: Up → Right, Right → Down, Down → Left, Left → Up). ROCKNIX handles this
automatically for MAME 2003-Plus when TATE mode is enabled in `system.cfg`. Save per game via Quick Menu → Core Options
→ Manage Core Options → Save Game Options.

## What has been checked

Details here are drawn from ROCKNIX 20261001's system list, core info files, ROCKNIX launch scripts and the libretro
arcade guide. **Arcade games haven't been live-tested through pocket-ui on device yet.** If anything behaves differently
on your handheld, let us know.

See also: [Systems](systems.md), [Troubleshooting](troubleshooting.md), [Patches and MSU-1](patches-msu1.md).
