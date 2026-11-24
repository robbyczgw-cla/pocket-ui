# Nintendo DS

DSperate is the default DS core on TrimUI Brick and Smart Pro, H700, RK3566 and
RK3326, including the dArkOS port. Android and SM8250/SM8550 retain melonDS until
a comparison on those devices supports changing it. A game's **Runs on** page
offers both cores, with a system-wide choice and a game override.

The core uses the existing player and pause menu: MENU pauses it; Save, Load
with previews, Earlier, Save & quit, Options, volume and brightness use the
same paths as other libretro cores.

The player's libretro audio-buffer feedback lets DSperate skip a rendered
picture before a slow software-rendering burst starves the sound. Automatic
frame skipping is the default; it keeps emulation and audio running. Skipped
pictures are reported as repetitions rather than counted as new frames. The
audio queue uses the existing 48 ms reserve for heavy renderers, including
software DS rendering. The Brick's cycle route already uses that reserve. On Linux, a repeated
DSperate software picture does not trigger a redundant draw or swap while no
overlay, launch animation or pending picture needs presentation. The player
still waits for its next refresh. A late heavy core that cannot catch up also
avoids an extra wait on a refresh rounded into its future. These changes reduce
missing PCM; they do not make every demanding scene run at full speed on H700.

## Controls

In **Options → Controls**, assign **Stylus mode** and **Screen layout** to any
button, for the system or a single game. On the Brick the defaults are L2 and
R2. Stylus mode makes the D-pad move the pointer and the mapped DS A button
touch; press the mode button again to return to the DS buttons. The right stick
can also move the pointer. A short press of the layout button swaps the two
screens of the current arrangement: top and bottom, left and right, or top only
and bottom only. Held for 0.4 s, it shows the next arrangement, as before, once per
hold: DSperate cycles through both vertical arrangements, both horizontal
arrangements, top only and bottom only; melonDS through its four configured
layouts, already swapped if a short press swapped them. Both cores and all
platforms use the same rule (`src/ds_pad.rs`); the player swaps through the
core options (`dsperate_screen_layout`, `melonds_screen_layout1..N`), so the
cores themselves are unchanged. Layout and pointer position are included in
DSperate moments.
On a touchscreen, a touch reaches the bottom screen in its displayed position.

## Existing saves

Both cores use the same raw battery save: `.sav` on Linux and `.srm` inside
Android's existing save folder. Switching cores keeps that in-game progress.
DSperate and melonDS moments are different formats and cannot be converted
reliably. Existing melonDS Auto, slots and Earlier files stay in place;
DSperate's equivalents live in `.cores/dsperate/` beside them. A launch notice
and Runs on explain how to return to melonDS to load its moments. Continue,
Earlier and previews use the selected core. Each core keeps its own ROM identity
so replacing a ROM cannot silently resume its predecessor's moment.

The case's progress reader reads the shared battery save. Its existing list
of supported DS games is unchanged; a game without a reader has no invented
progress line.

## Build and distribution

The engine is [beebono/DSperate](https://github.com/beebono/DSperate),
commit `99618eb956c66083618395dbae9bb5cbf0241d16` (v3.3.0). The libretro wrapper is
[thwonp/dsperate-lr](https://github.com/thwonp/dsperate-lr), commit
`27b9d76fb4c900d29f9629806f316527b5da90c2`. Both are GPL-3.0-or-later. The libretro
header is MIT; the FreeBIOS and fonts retain the notices listed in NOTICE.md.
No ROM, proprietary BIOS or downloaded game is required or included.

`scripts/build-dsperate.sh OUT WORK` uses the normal pinned core builder. Its
ARM64 JIT/software/NEON build targets glibc 2.17 for all Linux devices, with the
same instruction-cache synchronization used by the other JIT cores. No host
profile-guided data is applied. The two patches in `scripts/dsperate/` add
cross-compilation fixes, Android allocation/page-size handling and the common
pointer/layout controls. On an Android kernel with 16 KB pages the incompatible
4 KB alias mapper is disabled; the normal JIT memory path remains available.

`scripts/dsperate-files.py OUT DEST_CORES` stages the verified core, full
corresponding source, licence texts, patches and recipe. Release archives and
SD-image builders take `--ds-core OUT`; the dArkOS packager takes `DS_CORE_DIR=OUT`.
The normal Brick card builder stages it from its core output. Updates carry
these versioned files with the application, including rollback. Package checks
reject a missing source, notice, wrong architecture, newer glibc or changed
binary. Android uses the same pins and patch hashes in `android/cores.lock.json`
and the normal build, licence, source-archive and 16 KB alignment checks.

The real-core contract test (`tests/dsperate_core.cpp`) takes a privately
supplied ROM and optional raw save. It checks actual pen/button registers,
layout edges, pointer coordinates, SRAM import and a RAM/SRAM/layout state
roundtrip. Device measurements must be silent and dark; no benchmark ROM may
enter the owner's library or recent list.
