# Android core package evidence

The internal test APK bundles PPSSPP, melonDS DS and Mupen64Plus-Next alongside mGBA, Nestopia, bsnes, Gearsystem, Beetle WonderSwan, Stella, ClownMDEmu, Gambatte, Beetle PCE Fast, Beetle NGP and Handy. Build success and runtime smoke evidence are recorded separately; the wider requested set is still open.
No Snes9x, Genesis Plus GX, PicoDrive, FBNeo or old MAME binary is bundled.

| Core | Source revision | Licence evidence | Package |
|---|---|---|---|
| mGBA (GB/GBC/GBA) | [libretro/mgba 7a12d6d4b9acb14c0ae62c9166b6a2f3d08007f6](https://github.com/libretro/mgba/tree/7a12d6d4b9acb14c0ae62c9166b6a2f3d08007f6) | [LICENSE: MPL 2.0](https://github.com/libretro/mgba/blob/7a12d6d4b9acb14c0ae62c9166b6a2f3d08007f6/LICENSE); source notices retain their own copyright | ARM64 API 26, NDK 27.2, `libmgba_libretro.so` |
| Nestopia (NES) | [2110d21b7174455ec98948bb474147dea0e3cad0](https://github.com/libretro/nestopia/tree/2110d21b7174455ec98948bb474147dea0e3cad0) | [COPYING](https://github.com/libretro/nestopia/blob/2110d21b7174455ec98948bb474147dea0e3cad0/COPYING) plus [NstFds.hpp: GPL 2 or later](https://github.com/libretro/nestopia/blob/2110d21b7174455ec98948bb474147dea0e3cad0/source/core/NstFds.hpp); libretro-common MIT notices retained | `libnestopia_libretro.so` |
| bsnes (SNES) | [63ee22f7af8f5d1ada295c2c98fd7bf261fa62a7](https://github.com/libretro/bsnes/tree/63ee22f7af8f5d1ada295c2c98fd7bf261fa62a7) | [LICENSE.txt: GPL 3 only; support libraries ISC](https://github.com/libretro/bsnes/blob/63ee22f7af8f5d1ada295c2c98fd7bf261fa62a7/LICENSE.txt), [internal GB: MIT](https://github.com/libretro/bsnes/blob/63ee22f7af8f5d1ada295c2c98fd7bf261fa62a7/bsnes/gb/LICENSE). The full GPL 3 text also accompanies Gearsystem | `libbsnes_libretro.so` |
| Gearsystem (SMS/GG) | [6032d9d722707ce08ccac2462b021b77173ab1d4](https://github.com/drhelius/Gearsystem/tree/6032d9d722707ce08ccac2462b021b77173ab1d4) | [LICENSE: GPL 3](https://github.com/drhelius/Gearsystem/blob/6032d9d722707ce08ccac2462b021b77173ab1d4/LICENSE) plus [GearsystemCore.cpp: or later](https://github.com/drhelius/Gearsystem/blob/6032d9d722707ce08ccac2462b021b77173ab1d4/src/GearsystemCore.cpp); miniz MIT notice retained | `libgearsystem_libretro.so` |


| Beetle WonderSwan | [4b01295838ea89e3f1355bbe4cb5cf98aa6108cd](https://github.com/libretro/beetle-wswan-libretro/tree/4b01295838ea89e3f1355bbe4cb5cf98aa6108cd) | [COPYING](https://github.com/libretro/beetle-wswan-libretro/blob/4b01295838ea89e3f1355bbe4cb5cf98aa6108cd/COPYING) and `mednafen/state.c`: GPL 2 or later | `libmednafen_wswan_libretro.so` |
| Stella (2600) | [ba52c43b9eda950eb0c0eec69cda9b17dee8c39b](https://github.com/libretro/stella/tree/ba52c43b9eda950eb0c0eec69cda9b17dee8c39b) | [Copyright.txt](https://github.com/libretro/stella/blob/ba52c43b9eda950eb0c0eec69cda9b17dee8c39b/Copyright.txt), `License.txt`: GPL 2 or later | `libstella_libretro.so`; NDK uses `-pthread`, no separate `libpthread` |
| ClownMDEmu (Mega Drive cartridges) | [d43c2708b0a31c285ce16724b6c4a2e92af07346](https://github.com/Clownacy/clownmdemu-libretro/tree/d43c2708b0a31c285ce16724b6c4a2e92af07346) | [LICENCE.txt](https://github.com/Clownacy/clownmdemu-libretro/blob/d43c2708b0a31c285ce16724b6c4a2e92af07346/LICENCE.txt), `README.md`: AGPL 3 or later; recursive pinned dependencies and their notices accompany the exact source | `libclownmdemu_libretro.so`; Sega CD stays external until disc/BIOS staging exists |
| Gambatte (optional GB/GBC) | [d9d6cd06382d1ced30de34d56d3609452323dab1](https://github.com/libretro/gambatte-libretro/tree/d9d6cd06382d1ced30de34d56d3609452323dab1) | [COPYING](https://github.com/libretro/gambatte-libretro/blob/d9d6cd06382d1ced30de34d56d3609452323dab1/COPYING), `libgambatte/src/gambatte.cpp`: **GPL 2 only**, not relabelled or later | `libgambatte_libretro.so`; separate state namespace from mGBA |

`scripts/android/build-cores.sh` builds from the immutable revision. `android/cores.lock.json` is the package list and build recipe. The Python builder records licence-file/inline-header evidence, static NDK runtime notices, the binary's SHA-256 manifest,
full upstream licence and an exact corresponding-source archive in `android/build/cores/` when `ANDROID_RELEASE_SOURCES=1` is selected. Normal test builds skip source tarballs. The APK contains the
manifest and licence. For a public release, distribute the exact source archives next to the APK; do not replace them with moving branch links. Internal test checkpoints keep the generated archives on the build machine.
No core binary or source checkout is committed. All builds run locally, without GitHub Actions.

The player is a **separate native library and private `:player` process**, in the same APK. It does not link the
launcher's winit/NativeActivity/softbuffer code. `scripts/third-party.sh --manifest-path android/player/Cargo.toml
--target aarch64-linux-android --check` audits this actual dependency graph, independently of the launcher.
All 92 linked Rust crates in the first split build have a GPL-2.0-or-later-compatible licence option. The missing
registry-package MIT notices are supplemented from exact upstream commits in `android/licences/`.

This addresses the known Apache-only frontend dependency boundary; it does not by itself audit a core's licence,
third-party code, BIOS, build configuration or distribution obligations. The requested extra cores still need
individual pinned-source review and Android build. The existing core audit's GPLv2-only/mixed files in Gambatte,
PPSSPP and Mupen64Plus-Next are recorded rather than relabelled "or later". Those cores must remain in the separate
GPL-compatible player process if enabled. mGBA is the first runtime-proven core; the other three have built successfully and their per-system smoke is recorded separately.
ClownMDEmu is the GPL-compatible Mega Drive choice; Sega CD still requires multi-file/BIOS staging.
Only one core is loaded in a player process. GPLv2-only Gambatte does not link the launcher or another core.
Beetle PCE Fast and Beetle NGP are GPL-2.0-or-later. Handy combines Zlib, LGPL-2.1-or-later (Blip_Buffer) and GPL-2.0-or-later (Stereo_Buffer); the complete LGPL text is included in supplementary notices. The exact revisions and source-header evidence are pinned in `cores.lock.json`. These three cores are included in the test APK; each receives a quick emulator launch before being reported as runtime-tested.
Heavy-core downloads remain disabled until immutable
first-party release artefacts, checksums, licences and corresponding sources exist.

The smoke game's ROM is fetched separately from the existing `assets/starter-pack.json` with its pinned hash
and licence. It is not shipped in the APK. The audio diagnostic ROM is generated from our own test program.

AAudio binding note: `ndk` 0.9's `AudioStream::write` uses the status-code decoder for its positive frame-count
return. The Android sink recovers the original signed result (`i32::from(AudioError)`) and accepts only a positive
count no larger than requested. Negative native error codes still trigger recovery. This avoids interpreting every
successful write as a disconnection. The source is the pinned `ndk` 0.9.0 crate's `src/audio.rs` (`write`,
`AudioError::from_result`); this is API adaptation, not a licence change or vendored crate patch.

## Heavy-core test milestone

The lock also pins PPSSPP `fa50bb1976065c4f8b1b47af227d367fe9771555`, melonDS DS
`bc4e4b67d2d470d7c682810a1e892cafd6f9082b` and Mupen64Plus-Next
`f275caf4b2bfa1e6d1c51636746ea793f3d80320`. All are bundled in the current internal test APK;
there is no download service yet. The combined APK is about 19 MB, below the 100 MB limit.

The recorded patches remove xBRZ from PPSSPP and Mupen64Plus-Next's linked sources and advertised
filter choices. PPSSPP's old stored xBRZ setting has an original nearest-neighbour fallback.
PPSSPP uses GLES and disables FFmpeg and Discord. Its PSP system assets are not included yet;
a single-file homebrew PBP starts, but this does not establish compatibility with games that need them.
Mupen uses GLES3, the AArch64 dynarec and HLE RSP. Its first smoke is a CPU-rendered Hello World
homebrew, not evidence for 3D game compatibility. melonDS DS uses the threaded software renderer
and ARM64 JIT; all CMake-fetched dependencies have full immutable revisions in the lock.

These are internal development binaries, not a public release clearance. The Android ARM64 PPSSPP
patch also replaces Apache-2.0 `cpu_features` with an original NDK `getauxval`/HWCAP probe and removes
that static library from the linked target. SPIRV-Cross's compiled source headers explicitly offer
`Apache-2.0 OR MIT`; this build selects MIT and includes its `LICENSES/MIT.txt` notice. The glslang
parser/generator has its own permissive notices; it is not blanket-labelled Apache-only. The complete public-release
licence/source review happens once before publication. Dependency notices are retained in the APK now.
`ANDROID_RELEASE_SOURCES=1 scripts/android/build-apk.sh` additionally packages corresponding sources,
including melonDS's fetched dependencies; build and licence evidence stays in the manifest.
