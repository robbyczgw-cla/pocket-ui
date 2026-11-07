# Additional cores on the standalone Brick card

The card builds seven additional software cores (and three 3D cores, below) from the ROCKNIX 20261001 commits linked in `NOTICE.md`:
Beetle NeoPop, Beetle WonderSwan, Beetle PCE Fast, Handy, Stella, ProSystem and FBNeo.
`scripts/brick-build-cores.sh` targets aarch64/glibc 2.33, includes source archives and licence files,
normalises the Beetle filenames to pocket-core's `beetle_*` names, and links with `-z nodelete`.
The builds were checked with Zig 0.17.0. Use `BRICK_CORES` to select a subset and `JOBS=1` to `4`.
Source commits alone do not pin the toolchain: use the same Zig version to reproduce the recorded hashes.

The standalone Brick profile adds routing without changing ROCKNIX's verified systems or the NextUI pak.
`deploy/brick/es_systems.cfg`, the card builder and the updater include the ROM folders and cover binds.
The default build excludes the new routing through `cfg(feature = "brick")`; the profile additionally
requires the Brick device and `POCKETUI_BRICK_STANDALONE`.

FBNeo allows free noncommercial redistribution subject to its source/licence conditions; it is not a
free-software licence. It also prohibits soliciting donations for a project using FBNeo; check this
condition before distributing a card as part of such a project. Its ROM sets must match commit `f3b7749`. No ROMs or BIOS are included.
CPS3 and large arcade games are especially dependent on the game, CPU speed and memory.

The 3D cores (`mupen64plus_next`, `ppsspp`, `flycast`; GLES 3, no Vulkan) are built by the same script with zig 0.17.0 against the Brick's own
`libGLESv2.so.2` / `libEGL.so.1` (link time only, never shipped) and the pinned Khronos API headers; they run through the GLES context of
`src/brick_gles.rs`. mupen64plus_next uses its `arm64_cortex_a53_gles3` target (aarch64 dynarec), PPSSPP its libretro CMake target (`USING_GLES2`, ARM64
JIT, bundled ffmpeg for linux/aarch64), Flycast `-DLIBRETRO=ON -DUSE_GLES=ON`. Their licences are mixed (GPL-2.0-only parts, GPL-3.0 xBRZ), see `NOTICE.md`.
melonDS DS has a software renderer, but full-speed A53 performance has not been
established and the Brick has neither a touchscreen nor sticks for the current pointer mapping.
DraStic performance is not evidence for melonDS performance.

Verification: `scripts/test-brick-card.sh`; `cargo test --profile gate --features brick --lib brick_profile`;
`scripts/brick-core-check.sh CORES_DIR SYSROOT ROMS_DIR` with the Brick's own loader and C libraries in
an aarch64 container. The check now initialises and unloads every core, rejects symbols newer than the
Brick's libraries and provides a read-only VFS for current Stella. Optional game tests run only when their
ROMs exist. `pce-test.pce` can be a legally obtained PC Engine homebrew; WonderSwan and Atari 2600 tests
come from `scripts/testroms.py`. These are compatibility checks, not Brick performance measurements.


## Recorded compatibility results (2026-11-03)

All seven cores passed `dlopen(RTLD_NOW)`, init, deinit and dlclose using the Brick's
`ld-2.33.so` and C libraries in an ARM64 container. The highest required GLIBC symbol is 2.33
(2.17 for WonderSwan, 2.30 for Stella). All carry NODELETE. Two clean builds produced identical
core hashes and source archives; the hashes are in `brick-morecores-tested.txt`.
PC Engine (Reflectron), WonderSwan (original test program) and Atari 2600 (original test program)
each delivered 120/120 pictures. NGP, Lynx, 7800 and FBNeo had loader tests only, without game ROMs.
The probe measured about 12 MiB peak RSS without content for the six smaller cores and 83 MiB for FBNeo;
these are container probe measurements, not the memory use of pocket-core on the Brick.

The host card tests pass, including after rebasing onto the GPU-by-default card base.
The default ROCKNIX library suite passed 2967 tests (8 ignored); the UI binary passed 94 tests,
and the two other binary tests passed. The Brick profile passed its five targeted tests.
The only Rust source changes are in the feature-gated Brick module and a `cfg(feature = "brick")`
routing expression; no default routing, core list or Cargo dependency changed. The Brick UI compiles with `cargo check --features brick --bin pocket-ui`.
No real-device speed, controls, audio, saves or frame pacing were measured by this work.

## Second software-core addition

These standalone-card folders were added second. Core selection is confined to the
Brick feature, Brick device and standalone profile. In particular SG-1000 keeps Gearsystem instead
of passing through the older Master System/Game Gear → PicoDrive substitution.

| Folder | Core | BIOS in `Roms/bios` | Status / limits |
|---|---|---|---|
| `fds` | FCEUmm (already built) | `disksys.rom` | `.fds`; missing BIOS produces the existing named error |
| `sg-1000` | Gearsystem | none | `.sg`, `.sg1`, ZIP |
| `segacd` | PicoDrive (already built) | `bios_CD_U.bin`, `bios_CD_E.bin`, or `bios_CD_J.bin` | CUE with BIN tracks, CHD, ISO, M3U; use the game's region BIOS |
| `sega32x` | PicoDrive | optional | `.32x`, BIN, ZIP; AArch64 SH2 JIT with explicit cache sync; game speed unverified |
| `pcenginecd` | Beetle PCE Fast | **`syscard3.pce`** | CUE with tracks, CCD, CHD, M3U; Brick checks the exact filename this core opens |
| `supergrafx` | Beetle SuperGrafx | none | `.sgx`, `.pce`, ZIP |
| `pokemini` | PokeMini | optional `bios.min` | `.min`, ZIP |
| `virtualboy` | Beetle VB | none | `.vb`, `.vboy`, ZIP; games needing the right D-pad need a suitable button mapping |
| `pico-8` | FAKE-08 | none | `.p8`, `.p8.png`; free emulator, no proprietary PICO-8 runtime; compatibility varies |
| `tic-80` | TIC-80 | none | `.tic`; Lua enabled; other cartridge languages are not enabled by this build |
| `nds` | melonDS DS | built-in replacements for DS | `.nds`; **experimental** CPU renderer, ARM64 JIT; DSi needs its own firmware/NAND |
| `pc` | DOSBox Pure | none | ZIP/DOSZ, EXE, COM, CONF; demanding PCs/3dfx are outside the speed promise |
| `scummvm` | ScummVM | bundled engine data | `.scummvm` launch files or supported game folders; supply original game data |
| `saturn` | Yabause | `saturn_bios.bin`, `sega_101.bin`, or `mpr-17933.bin` | **experimental** software renderer/interpreter; expected too slow for general Saturn play |

The card builder copies ScummVM's declared upstream data files into `Roms/bios/scummvm/{extra,theme}`.
The Brick profile keeps valid user paths/game sections and directs missing paths there, with saves
under `Roms/scummvm/.saves`. No commercial games or BIOS are bundled. Existing Sega CD/FDS/Saturn
missing-BIOS messages run before loading the core. PC Engine CD has an additional Brick-only exact-name check.

Licences: Gearsystem, PokeMini, melonDS DS and ScummVM are GPL-3.0-or-later; Beetle SuperGrafx,
Beetle VB, DOSBox Pure and Yabause are GPL-2.0-or-later; FAKE-08 and TIC-80 use MIT (bundled dependencies
have their own notices). PicoDrive remains noncommercial. Pins are in `brick-build-cores.sh` and `NOTICE.md`;
Yabause's software-port pin is separately recorded in `deploy/brick/core-licences.txt`.
PicoDrive additionally enables the AArch64 SH2 recompiler and replaces Zig's cache flush with
`brick-clear-cache.h`, as gpSP already does. A silent original 32X bootstrap starts the adapter and
runs both SH2s through their boot path; this is a JIT smoke test, not a game benchmark.
The DS dependency revisions are also locked to full commits in `scripts/brick-melonds-deps.txt`.
The build archives tracked source from nested CMake/ScummVM dependency repositories as well as git
submodules, and collects dependency licence files. Partial builds preserve the other manifest entries.

The DS build disables OpenGL at compile time. Both DS and DOSBox Pure provide their own ARM64
instruction-cache flush. No assumption is made that the Brick has a touchscreen, analog sticks or a
physical keyboard: DS touch, mouse-driven ScummVM/DOS games and some Virtual Boy controls remain
limited with the current pad mapping. Use games that work with the available buttons. Saturn's
YabaSanshiro port explicitly requests a GL context, so this addition uses Yabause's software port.

For frame checks, `scripts/brick-testcarts.py OUT` writes original silent PICO-8, TIC-80 and DOS test
programs. `scripts/testroms.py OUT` supplies SG-1000 and Virtual Boy fixtures. Optional DS frame
checks accept `delve-ndsmode.nds` and `MinecraftDS-ndsmode.nds` homebrew from the earlier DS research.
The probe implements directory VFS, option defaults and frame-time callbacks and reports wall time
(including picture hashing) plus peak RSS. Container timing is **not A133P timing**.
Existing H700 measurements of another melonDS DS build showed 50–60% speed on two 3D homebrews;
this is a warning about the similar A53 CPU class, not a measurement of this Brick build.

### Compatibility results for the second addition (2026-11-03)

All ten new cores and the rebuilt PicoDrive passed `dlopen(RTLD_NOW)`, init, deinit and dlclose
with the Brick's loader and libraries in an ARM64 container. All carry NODELETE; no required
GLIBC symbol exceeds 2.33. The tested hashes are in `brick-morecores2-tested.txt`.
The full melonDS dependency pins match the dependency trees used by the tested binary.

SG-1000, Virtual Boy, the original 32X bootstrap, PICO-8, TIC-80 and DOS test programs each
returned 120/120 video callbacks. Both DS homebrews (Delve and Minecraft DS, patched into DS
mode in the earlier research) returned 120/120 pictures at 256×384 through the software renderer
and ARM64 JIT, including successful unload. ScummVM's no-content launcher returned 120/120
pictures at 1280×720. No original ScummVM game was tested. SuperGrafx, PokeMini and Saturn have
loader tests only; no commercial CD game, real FDS BIOS or full 32X game was available for this check.
FCEUmm separately produced 120/120 moving pictures from the original test disk and the original
`fds-bios-standin.rom`, mounted as `disksys.rom`. That verifies the BIOS filename/load path, not
compatibility with Nintendo's BIOS or commercial FDS disks.
The bootstrap verifies SH2 startup/JIT execution, not 32X game compatibility or speed.

The seven Brick profile tests, all five language/source/snapshot tests and the card tests pass.
All 25 source archives in the combined card input pass gzip integrity checks. UI and player compile with and without
the Brick feature. The export scan has exactly the same 28 findings as the unchanged base,
ignoring shifted line numbers; a clean full gate is not claimed. The source-archive helper was
checked with nested dependency repositories and filenames containing spaces: it excludes build
objects/git metadata and produces identical gzip archives at different output filenames.
The new binaries do not have a two-clean-build reproducibility claim.

ScummVM's archive command includes `cru`; its initial and resumed builds must use the same NEON
setting. The final link rejects undefined symbols. The portable data helper reads upstream's
asset manifests directly, avoiding BSD/GNU grep differences that otherwise omit themes on macOS.
The card builder requires both `sky.cpt` and `scummmodern.zip`, and its tests verify copying them.

No Brick was contacted. No A133P speed, physical controls, sound, saves or frame pacing were
measured. In particular DS and Saturn are not presented as confirmed playable on the device.
