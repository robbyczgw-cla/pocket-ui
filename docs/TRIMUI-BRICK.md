# TrimUI Brick under NextUI (design and reference; the Brick's first version)

TrimUI Brick support is pocket-ui on a card of its own, see [TRIMUI-BRICK-STANDALONE.md](TRIMUI-BRICK-STANDALONE.md):
your own microSD card, the Brick's stock firmware untouched, updates over Wi-Fi. This page is the design it grew from, the pak that runs on
top of NextUI, and the reference for the framebuffer bridge and the test kit.

Behind the `brick` Cargo feature. No ROCKNIX handheld builds any of this. A default build must retain the standard ROCKNIX code and embedded assets.
The current compile-time isolation and its verification status are in [BRICK-CFG.md](BRICK-CFG.md).

**Target (2026-11-02):** the Brick runs NextUI (tg5040, TinaLinux, kernel 4.9.191, **glibc 2.33**, ALSA card `audiocodec`, no
PipeWire, no libwayland). pocket-ui runs on top of it as a **pak** (`Tools/tg5040/pocket-ui.pak`), without changing the card's
system. Knulli was the first target and is dropped: the owner's Brick runs NextUI, and a second system image means flashing.

## Licence rule (binding)
NextUI changed from GPL 3.0 to PolyForm Noncommercial 1.0.0 on 2026-08-15, which our GPL cannot take in; MinUI has no licence at
all. **Never copy code, scripts or assets from NextUI or MinUI** into the repo, the pak or the kit, not even short snippets.
Reading them to learn how the launcher loop and the pak format work is fine; `launch.sh`, `start.sh` and the kit are written from
scratch. The pak holds only our own code and libraries whose licences we list; **no NextUI files and none of NextUI's cores**.
At run time it uses the cores already on the card by path (`.system/tg5040/cores`, read only).

| Part | What |
|---|---|
| `src/brick_profile.rs` | The Brick profile in one place: name, screen 1024x768, detection (`/usr/trimui/bin/MainUI` + `/mnt/SDCARD/.system`), the pad's name and its codes (read with the owner: A 305, B 304, X 308, Y 307, L 310, R 311, Select 314, Start 315, Menu 316, D-pad as hat axes 16/17; **L2 and R2 are analog axes ABS_Z/ABS_RZ, 0 to 255, not keys**). Only the Brick Pro is still to confirm |
| `src/device.rs` and friends | `Model::Brick` (every addition on an existing line, so a default build keeps every line number): no fan, no touch, no sticks, no Wi-Fi page, 1 GB; Settings leaves out radios, store, updates, tools, performance & fan, time, tailscale, haptics, sync, USB transfer |
| `src/pad.rs` | The pad `TRIMUI Player1` crosses X and Y like the H700's (printed X reports BTN_WEST 308, printed Y BTN_NORTH 307) |
| `src/bin/pocket_fb.rs` (`pocket-fb`) | The smallest compositor: Wayland (`wl_shm`, `xdg_shell`) and sway's IPC on `/dev/fb0`. `--picture`, `--message` (text for a missing piece), `--ipc-send` (one IPC command, for the pause test) |
| `src/soft_screen.rs` | `POCKET_CORE_SOFTWARE=1`: pocket-core without GL, the CPU copies the core's picture into a `wl_shm` window |
| `src/brick_picture.rs` | `pocket_picture`/`pocket_shaders`: pocket-core's renderer (shaders, screen frames) inside `pocket-fb --gl`, on the picture a game hands over at its own size |
| `src/brick_gl.rs` | The GPU through the system's EGL on PVR's Null WSI: a context, and a client's picture through a texture onto the screen (`pocket-fb --gl`) |
| `src/bin/brick_egl.rs` (`brick-egl`) | The GPU question on its own: `GL_VERSION`, frame times, `--probe` (can it draw here), `--panel` (the moving test a person looks at, with a read-back) |
| `scripts/brick/` | `start.sh` (the one start script), `env.sh` (the environment), `pocket-ui.pak/launch.sh` (a thin entry into `start.sh`) |
| `scripts/brick-kit.sh`, `scripts/brick-run.sh` | The first device hour in stages (POSIX sh; the Brick has no bash, python or `timeout`); `brick-run.sh` holds `/tmp/brick-device.lock` for a whole stage |
| `tests/brick_fb.rs` | `pocket-fb` on the host with a file as the framebuffer |

## How it starts
NextUI's loop starts a pak once `nextui.elf` has ended and starts `nextui.elf` again when the pak ends, so the screen is ours
while we run and the way back needs no code of ours. `start.sh` checks battery (`POCKET_MIN_BATTERY`), `/dev/fb0`, its binaries and
libraries (a missing one is painted by `pocket-fb --message` and written to `logs/start.log`), builds a **view** in `/tmp`
(this system has no `/storage` and its root stays untouched: `mount --bind` of the system folders plus our own `/storage`, then
`chroot`), starts `pocket-fb`, then `pocket-ui` inside the view; on exit it unmounts and removes its sockets.

All our files live in one folder (`/mnt/SDCARD/.pocket-ui`, the test kit's copy in `.pocket-ui-test`): `start.sh`, `env.sh`,
`bin/`, `lib/` (Debian bullseye's `libwayland-client/cursor/egl` and `libxkbcommon`, glibc 2.31 builds, plus the system's own
`libffi.so.7`), `data/`, `storage/`, `logs/`. The pak is `Tools/tg5040/pocket-ui.pak/launch.sh`; remove pocket-ui by deleting both.

## The shelf, the games and the way out
`start.sh` writes `storage/.config/emulationstation/es_systems.cfg` and `storage/.config/system/configs/system.cfg` in our folder at every
start (gb, gbc, gba, nes, snes, psx, megadrive, mastersystem, gamegear on pocket-core; gambatte, gambatte, gpsp, fceumm, snes9x, pcsx_rearmed, picodrive three times: the cores NextUI's card has) and `env.sh` points
`POCKETUI_CORES` at NextUI's core folder (read only, by path) and `POCKETUI_HOST_BIN` at our own pocket-core. Games are in our own
`storage/roms/<system>` (the kit's `rom` stage copies two official test games there); pocket-core keeps saves next to the game and states
in `storage/roms/savestates`, so **nothing writes to `Roms` or `Saves`**. **Hold SELECT + START** in pocket-ui for the dialog "Back to
NextUI?": it ends pocket-ui with exit 0, `start.sh` unmounts the view, and NextUI's loop starts NextUI again. On the Brick's own card
(no NextUI, `POCKETUI_BRICK_STANDALONE`) the same dialog is "Restart pocket-ui?" with Cancel and Restart pocket-ui: `start.sh` starts
it again (`rescue::BRICK_CARD_LEAVE`). On the Brick, lines that call the handheld "the Pocket" say "the Brick" in every language
(`i18n::brick_words`, after the translation, beside HOME → MENU); About has no Open EmulationStation…, the power menu no Android line,
Updates no System line (nothing on the card updates TrimUI's firmware), and the first start does not call the Brick untested. The defaults chosen after the settings walk (06.11.): About says "TrimUI Brick" without "(experimental)" (`Model::experimental`);
Settings → Playing → **Performance** has only Games: performance (`perf_fan_page::rows`, no fan, no fan readout); Sync offers no
PS2, GameCube & Wii or 3DS folders (`sync_page::rows`); Add games, Game check, the phone page and Backup name the card's folders as
a computer sees them, `Roms/<system>` (`library::load`'s `folder`, `app_backup::shown`), and Backup says nothing about flashing; the
sleep line is "The screen goes off and the game waits; any button wakes it."; Test buttons draws F1 and F2 on the Brick's pad; the
phone page says "Brick"; the manual starts with `docs/user/brick-index.md.inc` and leaves out `manual::BRICK_SKIPS`. In a game, Menu (316) opens
the pause menu (save, load, quit) through pocket-ui and pocket-core as on every other handheld.

PicoDrive plays all three Sega systems (`brick_profile::core_for` turns the Mega Drive and Master System/Game Gear cores of the other devices into
`picodrive`, also for the verified systems that `hostsys::route` picks). Games go in `storage/roms/megadrive` (`.md .gen .smd .bin`), `mastersystem`
(`.sms`) and `gamegear` (`.gg`). Not routed: Sega CD and 32X (unverified in `hostsys.rs`; PicoDrive can play them, but a BIOS and a real check are missing).
Licence: PicoDrive's `COPYING` is non-commercial (free to pass on, not to sell or use commercially), like snes9x; the card uses NextUI's copy by path, nothing is shipped.

## A slow first start of a game
A PlayStation disc (`.cue` + `.bin`, NextUI's `pcsx_rearmed`) took 111 s from the start of `pocket-core` to its first frame on 2026-11-02, with the
main thread in disk wait. The code was read (pocket-core's file interface is plain `open`/`read`/`lseek`; `pcsx_rearmed` reads a few sectors of the
disc at load, its read-ahead is 12 sectors, no whole-disc option is set anywhere), and none of it reads the disc in bulk, so **the cause is not proven**.
`pocket-core` now prints `load: N s so far, X MB asked of the files, Y MB from the card, main thread S` every five seconds from the third second of a
load, and `load: core and game ready after T, ...` at its end (`watch_load`): the next run on the Brick tells a read of the whole disc (megabytes grow
steadily) from a stalled card (megabytes stand still, thread state `D`). The log of the first frame also splits the time (`core` in `first frame after`).

## Night sessions, sound, and what runs
`scripts/brick-kit.sh night <stage>` pauses `nextui.elf` with SIGSTOP (never TERM: that powers the Brick off) and continues it with SIGCONT
at the end and from a detached dead-man. Nothing in a session turns sound on: the speaker and headphone switches of the audiocodec card
stay off (`mutewatch` forces them every 5 s, a session ends when one reads on), pocket-ui and pocket-core play to ALSA `null`. The end of a
session proves that no program of ours is alive. Volume, brightness and the battery reach Settings through `scripts/brick/hwroot` (our
stand-ins) and `brick-hw`; the PlayStation game is bound read only from the owner's card (`POCKET_PS1=<folder name>`), its saves and
states go to our folder (`POCKET_CORE_SAVE_DIR`).

## The GPU
Private and switchable: `POCKET_BRICK_GL=1` before `start.sh` (default off). Then `start.sh` asks `brick-egl --probe` in a process of its own, starts
`pocket-fb --gl`, and pocket-fb reads back that what it drew is on the page the kernel shows. Whichever of them fails, a line in the log (`logs/start.log`,
`logs/pocket-fb.log`) and the CPU copy runs, the same if a swap fails later.

**Read on the device, 2026-11-02:** PowerVR Rogue GE8300, OpenGL ES 3.2, build 1.19@6345021, EGL 1.4. `eglCreateWindowSurface` with a NULL window goes to
`libpvrNULL_WSEGL.so`, which allocates display buffers through PVR's display class and flips them (`DoPresentFlip`; `NULLWS_BUFFERS_COUNT` is a hint of
its). During an `egl` run `fb0/pan` moved from `0,0` to `0,768` and that page of fb0 held the GL picture, upright and in the right colours (the triangle's
tip on top). So the GPU draws straight into fb0's pages at 60.25 fps without a copy: a real flip. The `fbdev` window gives `EGL_BAD_NATIVE_WINDOW` (0x300b).
`/dev/dri/card0` is there, but the system has no libdrm or libgbm: no KMS way, and none is needed.

**What limits the UI.** On the A133, 1024x768, off screen against a file as framebuffer, mock hardware, 81 made-up games, CPU at 1.2 to 1.8 GHz:

| | ms |
|---|---|
| Paint of a settled page (thread CPU, `POCKETUI_FRAME_BENCH`, median) | home 3.7, settings 2.8, main 2.1 |
| Copy to fb0 (`pocket-fb`, real sessions) | 2.9 to 3.2, worst 8.7 to 19.7 |
| Frame of an animation, paint and handover (`POCKETUI_FRAME_LOG`, no vsync; includes waiting for the bridge's copy, up to about 3.6) | page transition 11 to 12.5, worst 20.6; shelf glide 12.7, worst 30.5; marquee and such 8.3 to 9.5 |

So the copy is about a fifth of a refresh and the paint of an animation frame two to three times the copy. On average a frame fits (about 12 ms of 16.7), but paint and copy run one after the other and the worst frames (probably the first of a move, the CPU still at 1.2 GHz) miss a refresh. **Replacing the copy gives at most 3 ms and lets paint and flip overlap; it can't make the paint itself quicker.** The number that decides is the `while moving:` line pocket-fb now writes (gaps between frames, late over 20 ms), copy against GL with the same presses: the `glui` stage.

**Why the GPU only behind `pocket-fb`:** pocket-ui's painter is CPU pixels (`Canvas`, 277 `paint` functions in 100 files, some reaching into the buffer directly): painting with GLES would be a new UI renderer, a redesign. pocket-fb is one place, keeps Wayland, the pause menu's focus changes and the fallback as they are, and clients change nothing. The `gltest` stage measures what the texture upload costs against the copy.

**Stages** (`scripts/brick-run.sh <stage>`, with the owner next to the device):
- `gltest`, 20 s, ends by itself. On the panel: **red square top left, green top right, blue bottom left, white bottom right**, on dark blue; **a white bar sweeps from left to right every 2 s; an orange triangle turns in the middle; a yellow bar along the bottom fills up until the end.** For the last 4 s the background is **moving violet stripes** (uploaded from the CPU each frame, what the UI would do). Then NextUI comes back. In the log: frame times with and without upload, upload and swap ms, glReadPixels ms, the fb0 pages seen and the number of page changes, four read-backs of the corners; `readback-gltest.raw` and `.raw.late` on the card.
- `glui`, about 140 s. The same presses (shelf, a shelf opened and left, Settings) with mock hardware and no sound on the copy, then on GL, with the `while moving:` numbers for both. The UI moves by itself for about 50 s. Then 90 s of the real pocket-ui through the GPU to press around in; SELECT + START held ends it earlier.

**Transitions on the GPU (built 2026-11-03).** With `pocket-fb --gl` pocket-ui asks the bridge once (`pocket_gpu` over the sway IPC) and then
hands every page transition to it (`pocket_transition <kind> <from_y> <top> <page colour> <cozy> <moves>`, `src/brick_gpu.rs`) instead of
painting the slide and fade itself; the bridge slides and fades the page's picture on the GPU at the screen's rate, the same look as
`motion::Transition` (`Transition::look`) without the rows' stagger. A change of window (the pause menu over a game, a game starting or
ending) crossfades on the GPU for 150 ms. Each animation writes a line `pocket-fb: gpu page push: N frames in M ms, worst gap G ms`. Without
the GPU the bridge answers no and pocket-ui keeps its CPU transitions. The glide along the shelf is still painted by the CPU each frame.

**Paint and present side by side, and the colour look on the GPU (2026-11-16, BRICK-60FPS.md).** Measured in the real UI on the device,
pocket-ui's frame took 13.7 ms of CPU and pocket-fb's present 11.4 ms (texture upload 7.9, draw and swap 3.5), and they ran one after
the other: softbuffer paints a frame only into a buffer pocket-fb has released, and pocket-fb sent that release after its upload and
swap. So every animation frame took two or three refreshes (25-35 fps). pocket-fb now sends its answers, the release first, before the
screen work, and pocket-ui paints the next frame while this one goes on the screen. Second: the Soft colour look (temperature, night
light, extra dim; the Brick has no gamma ramps) was a pass over every pixel of every frame, 5 ms on the A53 even as one multiply per
channel, because that is the memory's speed there. With the GPU, pocket-ui sends the look's multipliers when they change
(`pocket_tint <r> <g> <b>`, 16-bit fixed point, 65536 = unchanged; `brick_gpu::tint`) and leaves the pass out; pocket-fb multiplies
the sender's pictures in its shader (each texture keeps its own, so a crossfade with a game is right), and in the CPU copy when the GPU
is gone. Screenshots (`pocket_grab`) are the window's own pixels, without the look, as under sway's gamma. pocket-ui and pocket-fb come
from the same `app/bin` and restart together: an older pocket-fb would answer yes to `pocket_tint` without drawing it. pocket-fb's
exit report splits its time: `of that, upload X ms, draw and swap Y ms`.

**Only the changed rows into the texture; cases turn row by row (2026-11-16, BRICK-60FPS.md part 2).** With pocket-fb's GPU, pocket-ui
hashes every row after painting (`damage::RowHashes`, 2-2.5 ms on the A53: one read of the frame) and says the changed spans with the
picture (`wl_surface.damage_buffer`). pocket-fb keeps per window the rows said since its picture last went into the texture and uploads
only those (`brick_gl::Presenter::upload_rows`); each of its two textures knows whose picture it holds. A picture without damage, a new
size or a texture that holds another window's picture get the whole upload, so an older pocket-ui (or a game) is uploaded whole as
before, and an older pocket-fb ignores the damage. On the Shelf 190 of 768 rows a picture, the upload 2.7 ms instead of ~7; the exit report
says `N rows a picture into the texture`. The bigger part of the moving frames was elsewhere: a turning case (`cabinet_art::face`, the
Shelf's large front, the Cabinet's case coming out) was painted column by column at 1:1, a cache miss per pixel on the A53 (12.7 ms of
a 22 ms Shelf frame); now row by row (2.7 ms). Keep pixel loops row by row on the Brick (`timing_a_turning_case_costs_the_same_on_a_wide_screen`).

**The Cabinet's moving frames (2026-11-17, BRICK-60FPS.md part 3).** After the frame rework (one frame per cabinet) a moving Cabinet
frame took 12.8 ms of paint (p50; p90 17.9) and 2.5 of row hashing, and 14 % of all frames went over a refresh. Four changes:
Home cleared the page and the Cabinet cleared it again (1.1 ms; the snapshot line's `cleared=` and `tests/cabinet_frame.rs` keep it at
once); the row hash runs on three threads (`damage::HASH_THREADS`, 2.5 to 1.7 ms); `art::blit` paints solid runs of eight pixels
without a test per pixel (the walls 1.9 to 1.3 ms); and the hint bar's 120 ms fade, which starts with nearly every move because the
hints change from game to game (Play/Continue, Star/Unstar, Up for the search on the top shelf), mixes only from the left end of the
longer pill, with `motion::blend` and a kept buffer. Each of its two layers is still about 2 ms: saving, painting and mixing the
pill's ~78k pixels is memory-bound on the A53 (about 1 GB/s). Paint of all frames p50/p90/p99 9.1/15.1/24.2 to 7.9/12.9/18.6 ms,
frames over a refresh 202 to 88 of ~1470; dropped refreshes a second, Cabinet taps 2.45 to 1.9, shelf to shelf 3.0 to 2.5. Left in a
slow frame: the case coming out (`paint_out`, 4 ms with its two soft shadows), the
hint fade (4 ms), the TV (1.6), the walls and spines (2.6), the clear (1.1). Since part 4 the hint bar does not fade during a move
(`App::moving_now`: a step less than 250 ms ago, a held key, a gliding list); only a change at rest (Star on the same game) fades.

**The launch animation (2026-11-03).** On the standalone card the cartridge's insert froze and the game came before the cartridge was
in. pocket-fb gave the screen to pocket-core's window at its configure, before it had a picture; pocket-core draws only after the
launch's hold, so pocket-ui lost its frame callbacks and its animation stopped. Now a window takes the screen with its first picture, as
under sway, on the copy and on the GPU alike. Its paint times are in the `cabinet:` lines (`POCKETUI_FRAME_LOG`); if they show the insert
too heavy for the A133, the Brick gets a simpler insert (fewer layers or shorter), never one cut off.

**3D cores (2026-11-03, `src/brick_gles.rs`).** On the Brick pocket-core draws the picture on the CPU (`POCKET_CORE_SOFTWARE`), so a core
that asks for hardware rendering (`SET_HW_RENDER`, GLES 2 or 3) gets a GLES 3 context of its own on the vendor's EGL with a small off-screen
surface. It renders into our framebuffer (`get_current_framebuffer`); after each `retro_run` the frame is read back (`glReadPixels`, turned
upright when the core's origin is the bottom left) and goes on as an ordinary picture: the window to `pocket-fb`, scaling, the pause menu's
picture and the save-state previews as for a 2D core, states as before (they are the core's own data). `get_proc_address` gives the GLES
library's own symbols first, then `eglGetProcAddress` (an EGL 1.4 need not return core functions, and the Brick's returns NULL for GLES 3
ones). pocket-core also exports that resolver as `eglGetProcAddress` itself (build.rs: `--export-dynamic`, our C compiled hidden so
rcheevos' `rc_*`/`md5_*` never stand in for a core's own): a core that asks EGL by name instead of the frontend binds to it. PPSSPP's GLES 3
loader (`gl3stubInit`) does, and before this every PSP game crashed at its start on `glGetStringi` (20.11.2026).
`scripts/brick-make-card.sh` refuses a pocket-core without the export (`scripts/brick-elf-exports.py`). A core that wants desktop OpenGL, or
GLES 3 where only GLES 2 is, doesn't start, with the reason in the log. The read uses the order the driver gives without converting
(`IMPLEMENTATION_COLOR_READ_FORMAT`: BGRA is XRGB8888's own order; the GE8300 says RGBA, swapped in one pass), and
`soft_screen::show_native` takes the tight XRGB picture as it is (no copy into `scratch` first). The log has `[hw] N frames read back
(rgba|bgra, WxH max), X ms each: read A, convert B; worst W ms`. Measured on the Brick (2026-11-21, `--bench 30`): the read is mostly
reading the GPU's memory, which the CPU doesn't cache, not waiting for the GPU. Pack buffers (PBO, a frame later) were tried and dropped:
mapping waited ~0 ms but reading the mapped buffer took 10.8 ms for PSP's 480×272 and 24 ms for 640×480, two to three times the plain read;
a `glFinish` before the read added ~4 ms.
Test on a computer: `cargo test --features brick --test brick_gles` (a stub GLES core from C, Mesa as the GPU, the picture upright on the
screen file). Only the device can say whether a second process may use the vendor's EGL while `pocket-fb --gl` holds the screen (on the
copy it is the only one), and what the read-back costs on the GE8300.

**Shaders, screen frames and picture effects (built 2026-11-03).** With `pocket-fb --gl` a game gets the same
Picture options as on a ROCKNIX handheld: screen scaling, sharpness (sharp, crisp, soft), the effects (LCD grid with the GBA's colours,
scanlines, CRT), every screen frame (the console's own, Ambient, the handhelds, TVs, cabinet, glow), and screen orientation.
- *The way:* pocket-core still draws no GL (the Null WSI has one screen). It puts the core's picture in its window **at the core's own size**
  (`soft_screen::show_native`, XRGB8888, no CPU scale) and sends `pocket_picture <system> <aspect> <turns> <frames dir> key=value ...` over
  the sway IPC (`brick_picture::Request`: the console, the core's display aspect, SET_ROTATION, the Picture settings as the menu keeps
  them, Kids Mode's picture over them). `pocket-fb` runs pocket-core's own `video::Renderer` (the very shaders `src/shaders/video.frag` and
  the frame cache of `bezel.rs`) in a second `glow::Context` on its EGL context (`Egl::another_gl`), on the window of the process that sent
  it, straight onto the page that flips. A change in the pause menu (`set <key> <value>`) goes out again and the picture on show is drawn
  again at once. The pause menu's own window (`pocket-ui`) and every other window go through the plain presenter. The shader variant a
  setting needs is compiled when the setting arrives (`Renderer::warm`), not in the middle of a frame.
- *Without the GPU:* `pocket_shaders` and `pocket_picture` answer no (CPU copy, or a GPU that was lost later), pocket-core keeps the CPU scale
  (`soft_screen::blit`, whole-number, centred) and **pocket-ui leaves the rows out that would do nothing**: the pause menu's Picture group and
  the This game page's Picture rows, and Settings > Screen frames (`brick_profile::picture_rows_out`, one answer for all three, asked once).
  With a working GPU they come back. If the GPU is lost mid-game, the bridge shows the native-size picture the way the CPU path would
  (`show_copy`) and the next `pocket_picture` is refused. What the bridge would draw over the game (the FPS line, the badge, toasts and
  notes, the volume and brightness bar) pocket-core then blends into its CPU copy itself (`soft_hud`, the same pills), only while one is up.
- *Still CPU on the Brick:* the pause menu's preview of the game (`write_shot_soft`) is the plain picture, without effects or frame; the night
  light and the colour temperature tint the native picture in pocket-core before it goes out (`soft_screen`'s tables), so they show
  under the shaders too; the picture over the game (Pixel Path) stays out (a compositor effect, not these shaders). A change of window from or to a
  game with shaders cuts instead of crossfading (the fade needs the game's drawn picture as a texture).
- *Cost:* `brick-egl --shaders [--frames N] [--window null|pbuffer]` draws every sharpness and effect and the frames over a GBA and a SNES picture
  with the GPU finished after every draw and prints `shader: <scene> <settings>: <ms>`. On the Brick it needs nothing else on the screen
  (a kit stage). `POCKET_CORE_GPU_TIMER=1` for `pocket-fb` logs the GPU's own timer-query time for the picture pass where the driver has them.
  The host numbers and the estimate for the A133's GE8300 are in `docs/BRICK-SHADERS.md`.

**What comes after, not built:**
- *Quicker animations:* page transitions and glides drawn by the GPU from the finished picture (a texture slid and faded instead of `Transition::apply` on the CPU), one module, only if `glui` says those frames are the late ones.
- *3D cores:* `hw_render.rs` needs a GL context in pocket-core. The Null WSI has one screen, so pocket-core would make its own context on it and pocket-fb must let go while a game shows; whether two processes may hold display contexts one after the other has to be tried. Software cores need nothing of this.
- *Which cores:* `pcsx_rearmed` is on the card and draws on the CPU with NEON: it runs on today's way, to be measured. N64 (`mupen64plus_next`, GLES 3) and PSP (`ppsspp`, GLES 3) need the 3D way and a core built by us (glibc 2.33); the GE8300 with four Cortex-A53 should manage 2D PSP titles and lighter N64 ones, heavy ones not. An estimate, not measured.

## Build
    cargo test --features brick --test brick_fb
    cargo test --profile gate --features brick --lib brick_profile
    cargo zigbuild --release --features brick --target aarch64-unknown-linux-gnu.2.33 \
      --bin pocket-ui --bin pocket-core --bin pocket-fb --bin brick-egl
    # the GPU's way on a computer, Mesa's software GPU: tests/brick_fb.rs (skips with a line when there is none)
    KIT_FAKE=1 KIT_ROOT=<folder with the kit> EGL_PLATFORM=surfaceless sh brick-kit.sh gltest

Lines of `src/bin/pocket_core.rs` and the shared files with a `brick` addition keep it at their end: no line moves, so a default
build keeps every panic location and its bytes. Keep it that way, and compare with a build of `main` in the same folder.

## Type size on the 3.2" panel

The panel is about 15.75 px a millimetre (400 ppi), the RG35XX's logical 1280×960 canvas about 18 and the Mini's 1240×1080 about 16.5,
so a design size is not smaller on the Brick than elsewhere, but all of them are under what is comfortable at arm's length (Body 2.0 mm,
a note 1.6 mm). The Brick's Normal text size is therefore a step above the design for the small sizes (note 21 → 25, Body 28 → 31,
Settings' explanations 30 → 32; rows, titles and the pause menu stay), and Medium and Large are never smaller than it. The rule is
`type_scale::brick_base`, the table is in docs/DESIGN.md ("Text size").

## Cabinet navigation on the 1024×768 panel

Cabinet draws only whole shelves below the selected game preview, so a library with five systems need not
show five shelves at once. Up/Down selects every shelf and scrolls it into view; the position and arrows beside
the game details now make the shelves outside the view apparent. This applies to every profile (also the code
on main), not just Brick. `tests/cabinet_navigation.rs` renders a five-system fixture at 1024×768 and checks
NES, PlayStation, the final shelf, and return navigation. The scanner is unchanged.

### Launch feedback and printed buttons (development build 8)

A pocket-core load still busy after three seconds shows “Loading the game…” and a spinner. The due timer requests a redraw
before moving its next wake; otherwise the launch fade has already stopped painting and pocket-fb keeps the old black picture.
`app_frozen::loading_tests` covers the timer, and the RAM CUE/FIFO device check covers the real framebuffer.
The Brick's printed menu key is **MENU** throughout hints and help. Text uses that device name after translation,
before measuring and wrapping, including the manual; learned game names stay untouched. Other profiles retain HOME.
`brick_settings_walk::printed_menu_key_is_used_in_the_brick_hints_in_both_appearances` covers both appearances.
