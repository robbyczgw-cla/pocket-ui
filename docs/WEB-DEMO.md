# pocket-ui in the browser (prototype)

The real pocket-ui, compiled for the web: the shelf with its boxes and spines, the console pages, the game card and
box, search, Settings (Retro, Cocoa and Pea included), the start animation and the pause menu, in a page on a computer
(keyboard) or a phone (touch and an on-screen pad). No game runs and no ROM ships: a start plays pocket-ui's own start
animation and then shows a card, "your game would start here". Status: prototype on `feat/web-demo`, not on the site.

## How it works

The same sources (`src/`) are built for `wasm32-unknown-emscripten` from a manifest of their own, `web/Cargo.toml`; the
device builds' `Cargo.toml` and `Cargo.lock` are not read and not changed. Emscripten gives the UI what it expects of a
Linux box: a file system (in memory, filled with the demo library), threads (Web Workers over shared memory), the clock
and POSIX calls. What can't work in a page fails the way it fails on a device without it: no Wi-Fi, no `nmcli`, no
sway; the mock hardware (`POCKETUI_MOCK`, on by itself off Linux) answers for brightness, battery and the rest.

| Part | What it is |
|---|---|
| `web/Cargo.toml`, `web/build.rs` | The web crate: `../src/lib.rs` and `../src/main.rs`, the root build script's C parts, and `assets/gamedb.bin` cut to the demo's systems (`POCKETUI_WEB_GAMEDB`) |
| `web/shims/winit`, `web/shims/softbuffer` | Stand-ins with the parts of winit 0.30 and softbuffer 0.4 the App uses: one window that is the page's canvas, an event loop that turns as winit's does (events, the frame, `about_to_wait`, a sleep as the control flow says), and a buffer that becomes the page's frame |
| `web/web_audio.rs`, `web/demo/page/sound.js` | Sound: `platform::audio_backend` for the web. pocket-ui's UI mixer writes 48 kHz stereo into a ring in shared memory, an AudioWorklet plays it. So the sounds, Retro's blips and Settings → Sound & feel → Sounds (off, how loud) are the device's own. The page starts its AudioContext with the first press or tap (the browsers' autoplay rule); until then the samples are dropped at their own pace, so nothing plays late. The writer stays at most 45 ms ahead |
| `web/app_web_demo.rs` | Compiled into `pocket-ui` only for the web (`mod` at the end of `src/main.rs`): what the page calls (`pocket_web_button`, `pocket_web_touch`, `pocket_web_resize`, `pocket_web_frame`, …), the web's `spawn_game`, and a stand-in for pocket-core |
| `web/make-demo-library.py` | The demo library in `web/demo/storage` (not in git): fourteen GBA, SNES, N64 and PlayStation games with their covers, screenshots and title screens from libretro-thumbnails, one line of text where the ROM would be, settings past the first start |
| `web/build.sh` | `release` (what a site gets), `fast` (no LTO, for trying things), `data` (only the library again); writes `web/demo/dist`: the library and `pad.js` at the top, the program in `simd/` (wasm SIMD) or `base/` (`WEB_SIMD=0`, for browsers without SIMD such as Safari before 16.4). A site gets both; the page picks one. Source paths in panic messages are remapped (`/pocket-ui`, `/cargo`, `/rust`), and the library is packed with relative names, so no folder of the build machine is in the files |
| `web/demo/page/` | A plain test page and `pad.js`, the page's side: canvas, keyboard, on-screen pad, the frame loop, the card |
| `web/demo/test/serve.mjs`, `drive.mjs`, `journey.sh` | A local server with the needed headers; a headless Chromium driver (steps, screenshots, frame rates, checks of `/tmp/pocket-ui.status` and of what the canvas shows); the journeys a visitor makes on a computer and a phone, which fail when a step doesn't come |
| `web/check-device-source.sh` | The proof that device builds are unchanged: the expanded Linux source of `<ref>` and of the working tree compared |

**A frame.** The App paints exactly as on a device (`App::draw` → `frame::Presenter` → the softbuffer stand-in). Its
`present` turns the 0RGB buffer into RGBA; the page takes the newest one on each `requestAnimationFrame` and puts it on
the canvas. The App paints at the H700 handhelds' 640 × 480 (`POCKETUI_DEVICE=rg35xx-h`); CSS scales the canvas.

**Input.** Keys and the pad's buttons become `UserEvent::Pad` button events in their roles (as the command FIFO's: A
confirms); a finger or the mouse on the canvas becomes `UserEvent::Touch`, the touch screen's path (`app_touch`).

**A start.** pocket-core "is installed" (an empty `/storage/pocket-ui/pocket-core`), so a game starts the way it does on
a handheld, with the controls card on a system's first start and the case going into the console; N64 runs on
ROCKNIX's RetroArch as on the H700s (`hostsys::H700_STAYS_ON_ROCKNIX`, the demo's `es_systems.cfg`). The web's
`spawn_game` starts nothing and runs a thread that plays the game's side. It reads what pocket-ui writes to
`/tmp/pocket-core.cmd`, a device file the page makes (`pad.js`) that keeps the commands in order as the FIFO does:
`shot` and `preview` get a plain screen saying "Game picture shows here on a handheld" (behind the pause menu, in Options → Picture's previews, under Zoom), `save <n>` leaves a real slot in memory (a state file
saying there is no game in it, and the card as its picture, so Load shows it), `quit` ends the "game" with
`GameExited`; load, restart and the rest change nothing, as a game would just go on. Every other way of ending a game
(RetroArch's quit, closing a window) shows as `game=quitting` in `/tmp/pocket-ui.status`; after 0.7 s the stand-in
ends then too, as a game that saved and closed. The card is pocket-ui's own: `spawn_game` paints it on the App's
thread with `kit::empty_state`, the console's picture and the page hint for HOME, so it has the type, colours and
appearance in use (Barlow, or Retro's pixel letters in Cocoa or Pea). The page puts it up once the start animation is
over (`game=playing`, `place=game`); HOME (or a tap) opens the real pause menu over it.

**What the device builds see.** The changes in `src/` are `cfg` conditions that add `target_os = "emscripten"` to
existing `target_os = "linux"` ones, written on the same line, the web's `gamedb` file, and the `mod` at the end of
`main.rs`. No line of a device build moves: for `target_os = "linux"` the expanded source of the library and the
binaries is the same as main's (compared with `-Zunpretty=expanded`, default and `--features brick`).

## Build and try it

```sh
python3 -I web/make-demo-library.py <libretro-thumbnails dir> web/demo/storage
. <emsdk>/emsdk_env.sh
sh web/build.sh fast            # or release
node web/demo/test/serve.mjs web/demo/dist 8790 &
PLAYWRIGHT=<dir of playwright-core> CHROME=<chromium> sh web/demo/test/journey.sh http://127.0.0.1:8790/ /tmp/shots
sh web/check-device-source.sh main          # after changing anything in src/
```

Needs emsdk (emcc), the Rust toolchain's `rust-src` (std is built again with atomics: `-Zbuild-std` under
`RUSTC_BOOTSTRAP`) and its `wasm32-unknown-emscripten` target. A cold release build takes about 12 minutes on a 16-core build machine,
`fast` about 4 after a change. The page must be served with `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp` (shared memory for the threads) and a CSP that allows `'wasm-unsafe-eval'`.

## Measured (2026-11-09, release build)

| | |
|---|---|
| Download | 8.5 MB gzip, 6.7 MB brotli in all: `pocket_ui.wasm` 7.3 MB gzip (5.4 MB brotli, 15 MB unpacked), the library 1.2 MB (pictures, already packed), the loader 0.03 MB. `web/build.rs` keeps only the demo's systems of the game facts (1.2 MB less) |
| Painting | opt-level 3 with wasm SIMD (the default): 2.2–3.7 ms a frame on a desktop core, at most 7.5 ms (the case going in). opt-level z: 5–9 ms, at most 25 ms, for 1.5 MB gzip less. Scrolling with a held button: 33 frames a second, pocket-ui's own pace for held steps |
| Start | First frame 0.1 s after the program is loaded; on a throttled "phone" (12 Mbit/s, 70 ms) about 7 s from the click, nearly all of it the download |

SIMD needs Safari 16.4 or later (2023); `WEB_SIMD=0` builds without it.

## What isn't there

Game sound (no game runs), the Cabinet's TV and other looks were not walked
through, the update, Wi-Fi, Bluetooth and Tailscale pages show the mock hardware, and the settings live only as long as
the tab (a reload starts fresh).
