# Screen frames and the live picture preview

## Screen frame (Options → Picture → Screen frame)
- **None / Auto / Ambient / Shell / Brick / Pocket slim / Clamshell / Wide / Twin / Gloss / Swan /
  Watch / Goggles / CRT / Studio monitor / Portable TV / Woodgrain / Arcade / Glow** (values `none auto
  ambient shell brick pocket clamshell wide twin gloss swan watch goggles crt studio portable-tv woodgrain
  arcade glow`). Every frame but None is drawn behind the picture whenever the picture leaves room around
  it: Integer (`native`), Integer smaller (`integer-smaller`) and Aspect scaling. Fullscreen and Aspect
  screen fill the panel and never get a frame, so stretching to the full screen stays one setting away.
- Sub-choices appear under the frame only while they mean something (`Setting::shown_when`,
  `Group::visible` in `src/options.rs`), each its own key, so every frame keeps its finish:

  | Frame | Key | Values |
  |---|---|---|
  | Shell | `frame_colour` | `grey indigo clear-purple atomic-purple black white glacier` |
  | CRT | `frame_tv` | `plastic wood` |
  | Brick | `frame_brick` | `grey clear red yellow green black` |
  | Pocket slim | `frame_pocket` | `silver clear` |
  | Clamshell | `frame_clamshell` | `platinum cobalt flame graphite` |
  | Swan | `frame_swan` | `pastel skeleton` |
  | Watch | `frame_watch` | `gold silver` |

  The Picture page keeps room for one sub-choice row, so the live preview doesn't jump. Unknown or
  missing values are the first one (`bezel::finish!`), so settings files from before keep working.
- **Twin** is offered for `nds` only: `options::offered`, which `GameSettings::step` skips past for
  other consoles (a saved `twin` elsewhere is kept and draws the lens).
- Like every Picture setting they are kept per console and per game, and pocket-core applies them
  live (`set <key> <value>`, `VideoSettings::set`).
- Defaults (the owner's choice, 2026-10-23): None on every console, and no screen effect either — a clean picture out of the box.
  Settings → Look & feel → Screen frames (`settings::Frames`: Off, the start setting; Handhelds; Every console) puts Auto below the
  console's and the game's own choice (`GameSettings::load` in `src/options.rs`; `options::HANDHELDS`).

## Integer, smaller
Integer scaling at GBA 5× leaves 20 px left and right; a frame gets 16 of them. **Integer, smaller**
(`Scaling::IntegerSmaller`) takes one whole-number step below the largest that fits (never below 1×;
a picture too big for 1× is scaled to fit, as with Integer): GB/GBC 6× 960×864 at (140, 108), GBA 4×
960×640 at (140, 220), SNES and NES (224 lines) 3× 896×672 at (172, 204). Pixels stay square-exact.
- The frame follows the picture: it is placed so that its *hole* lands exactly on the integer-scaled
  game rectangle, so a different scale or core geometry moves and resizes it with the game.
- Code: `src/bezel.rs` (art, packs, GL layer), hooked into `video::Renderer::draw`. The frame is drawn
  first, on the black clear; the picture goes on top, so a frame never covers game pixels.

### Built-in frames
Drawn in code at the exact output size (no scaled bitmaps, so they are sharp at every scale), original
designs: generic shapes, materials, lines, grilles, hinges, knobs and dials after the shape language of
classic handhelds and TVs, but no logos, lettering, brand names or exact outlines. Auto picks one per
console (or an installed pack, which still wins); the others are chosen in the menu and ignore packs.
Code: `src/bezel.rs` (Handheld, Monitor, layout, Auto, the GL layer), `src/frames.rs` (Shell, CRT,
Arcade, Glow), `src/frames_more.rs` (the handhelds: Brick to Goggles), `src/frames_tv.rs` (Studio
monitor, Portable TV, Woodgrain), `src/frame_led.rs` (the live power light), `src/ambient.rs` (Ambient).

**Auto** (`bezel::style_for`): gb → Brick (grey), gbc → Pocket slim (clear), gba → Wide, nds → Twin,
psp → Gloss, wonderswan → Swan (pastel), wonderswancolor → Swan (skeleton), gameandwatch → Watch (gold),
virtualboy → Goggles, atari2600 / atari7800 → Woodgrain, gamegear / ngp(c) / lynx / pokemini → Handheld,
everything else → Monitor. Before 2026-10-23 Auto drew Handheld for gb, gbc, gba, wonderswan(color) and
virtualboy; explicit choices are stored as their own values and are not affected.

| Style | Reach | Look |
|---|---|---|
| Handheld | 8.5 % | Dark charcoal screen lens; the LCD sits in a recessed well (dark walls, lit lower lip); bevelled outer edge lit from above; faint diagonal sheen; a small lit power light where the band is wide enough. |
| Monitor | 7 % | Neutral graphite surround with rounded corners, a thin black gap at the picture and a faint glow of the picture's light on the bezel. |
| Shell | 26 % | The Handheld lens (up to 42 % of each band) set into a moulded body with large rounded corners: a rim lit from above, a soft contact shadow round the lens, a parting seam, a faint sheen, and a small round-hole speaker grille low on the right where the body is tall enough. Clear purple, atomic purple and glacier blue are see-through: darker insides behind an inner wall, screw posts in the corners, milky rims, more gloss. |
| Brick | 26 % | Matte body (or clear, with insides, screw posts and the screen module's dark frame showing) round darker blue-grey glass whose bottom-right corner is as round as the band allows (`frames_more::corner_max`); two thin printed lines, deep purple over navy, across the glass above the picture (where the glass is 12 px or more); live power light left of the picture. |
| Pocket slim | 20 % | Slimmer: brushed metal (streaks along x, a broad band of light) or clear plastic, tighter glass (about 40 % of the band), live light top left (or left). |
| Clamshell | 24 % | The lid of an opened clamshell in metallic paint (smooth, a broad highlight, fine flake), black gloss glass, and along the bottom (when the band below is 22 px or more) the hinge: a cylinder across the width in three parts, the middle one darker, with the live light in it. |
| Wide | 30 % | Landscape body whose outline is `|x/a|^p + |y/b|^q = 1` (flat top and bottom, the wings as round as the glass allows); grey shoulder parts peek out round the top corners and thin out smoothly; black glass; a round dot grille low on the right wing (or below); live light on the left wing. |
| Twin | 25 % | Only for two stacked DS screens (`frames_more::stacked_gap`: 256 px wide, 384 to 510 high). Lid and base each with black glass round their screen, speaker slits beside the top screen, and a hinge cylinder between them: beside the picture where the screens touch, and in the gap between them when the core leaves one (melonDS DS screen gap): the frame's **inlay**, see below. Other layouts get the Handheld lens. |
| Gloss | 27 % | Piano black (near black, the room's light as a gradient, one broad and one thin diagonal streak, a sharp lit top edge), ends as oval as the room allows (with no room at the sides, e.g. PSP at Aspect, the bands above and below curve down to the picture's corners); matte black border with a fine bright edge; live light low left. |
| Swan | 20 % | Minimal: pastel matte or smoky see-through skeleton, a plain glass, live light top right. |
| Watch | 24 % | Thin dark plastic rim, brushed gold or silver faceplate, the window cut into it (dark, a lit lip below) and one thin printed line round it. |
| Goggles | 26 % | Matte black visor with a fine grain, a dark light shield falling away into the picture (a faint red from it), a fine red trim line, a glossy red band along the top and a notch for the nose at the bottom (when the band below is 40 px or more). |
| CRT | 16 % | Tube glass (near black, a faint glow of the picture and a soft reflection top left) in an opening that bulges like a curved screen (a superellipse as round as the room allows while the picture's corners stay inside); a sloped inner bezel, dark on top and lit below; a groove; the cabinet in dark plastic or walnut veneer (procedural grain, satin finish); a slight vignette. |
| Studio monitor | 16 % | Warm grey plastic, flat-ish tube with crisp glass (a fine bright reflection under its top edge) in a nearly rectangular opening, a narrow sloped inner bezel, and below the picture (when there are 30 px) a recessed strip with three push buttons and five ribbed knobs with pointer lines. |
| Portable TV | 24 % | Charcoal body, rounded tube; on the right (when the side is 64 px or more) a brushed panel with a knurled tuning dial (a window of ticks, a pointer) and fine speaker slots, else both below; a telescopic antenna (three chrome sections, collars, a ball tip, a swivel) lying low inside the band above, which the body leaves free for it (when that band is 44 px or more). |
| Woodgrain | 20 % | Walnut veneer (the CRT's grain) at the sides and below, a ribbed black band along the top with a fine bright trim under it (when the band is 16 px or more), black gloss bezel round the picture. |
| Arcade | 13 % | Gloss black with the room's light as a long gradient and one diagonal streak, a bevel and a fine trim round the picture, and two frosted light strips (blue to magenta along their length) with a soft halo, at the sides, or above and below when the sides are thinner than 18 px. |
| Glow | 6 % | Black, and a thin rounded line a few pixels off the picture, glowing softly in the console's shelf colour (`bezel::system_colour`, shared with pocket-ui's `stripe`). |
| Ambient | whole border | Live: see below. |

Details that need room (lines, grilles, slits, knobs, dial, antenna, hinge, notch, lights) are left out
where their band is too narrow, so Integer at GBA 5x (16 px at the sides) still gets a clean frame.

### The live power light (`src/frame_led.rs`)
Brick, Pocket slim, Clamshell, Wide, Gloss and Swan put a power light where they have room
(`Bezel::led`): green above 20 % battery, red at or below 20 %, a slow blink (1 s on, 1 s off) at or
below 5 %, and a flash (450 ms, brighter and with a wider halo, in three steps) after a slot save or an
auto save (`pocket_core.rs`: `save_slot`, `save_auto` → `Renderer::flash_frame_light`). A save from the
pause menu happens while nothing is drawn, so the flash starts with the next frame drawn, as the game
comes back. Without a battery (desktop, virtual device without its mock) it is steady green.
- The frame is still drawn once per size and cached. With it the generator keeps a copy of the light's
  square (about 6 light radii each way, ≤ 66 × 66 px) without the light (`frame_led::capture`).
- `bezel::Layer::draw` asks `frame_led::Clock` what the light shows (time arithmetic per frame; the
  battery's `capacity` file is read every 10 s, `POCKETUI_BATTERY` points elsewhere) and only when that
  changed paints the square again (`frame_led::paint`, skipping picture pixels) and uploads it with one
  `glTexSubImage2D`: never while steady, once a second while blinking, three times in a flash.
- `frame_led::look_at` is the whole rule set, a pure function (tests in `frame_led::tests`).
- `--frame-demo` shows other states: `FRAME_DEMO_LIGHT=<battery %>[,flash|,off]`.

### Inlays
A frame may own a strip *inside* the hole that holds no game picture: `Bezel::inlay`, only Twin's
hinge in the gap between stacked DS screens (whole rows inside the gap, so no game pixel is covered).
The renderer draws it right after the picture with one scissored quad (`Layer::draw_inlay`, in
`video::Renderer::draw`), so it costs that strip's pixels per frame and nothing for other frames;
`bezel::compose` (desktop previews) does the same.

Each side is as wide as the space beside the picture allows (at most the reach, as a share of the
picture height; `bezel::bands`), keeping a strip of true black at the panel edge, so the frame is never
cut off by the screen edge. Dark gradients are dithered; without it they band visibly on the OLED panel.
Frames are drawn with a scissor on the four bands around the picture (`bezel::draw_around`), so the
part the picture covers costs nothing.

### Ambient
The border shows the game's own picture, heavily blurred, dimmed and a little more saturated, like a TV
with ambient light; Ambient is drawn on the GPU every frame from the live picture:
1. Every 3rd frame (`ambient::CADENCE`) the picture is box-filtered (4×4 taps per texel) into a tiny
   image 48 px on its long side, and blended into a running average with a weight of 0.33
   (constant-alpha blending, half-float texture where the GPU can render to one). A scene cut settles
   in about a fifth of a second; flashes don't strobe the border. The first frame, a new geometry and
   every preview take a full sample.
2. A separable 9-tap Gaussian (5 bilinear taps per direction) blurs the tiny image.
3. Every frame, the bands around the picture show it upscaled with a cubic B-spline (4 bilinear taps:
   smooth, no creases), centred on the picture and just large enough to cover the screen (`ambient::cover`),
   so the colour next to each edge is the colour at that edge. It is saturated ×1.5, dimmed (more for
   bright scenes), shaded darker within 26 px of the picture (keeps its edge crisp) and towards the panel
   edges, and dithered.

4. The graded glow is composed at a third of the screen's resolution (it is smooth throughout), only
   when a sample was taken. Every frame then costs one bilinear tap plus dither per border pixel, the
   same as a static frame.

### Trying frames and measuring them
`VDEV_NAME=frames scripts/frame-shots.sh <system> '<settings json>' <name> [seconds] [shot seconds…]`
runs a game in the virtual Mini V2 with the given Picture settings, takes screenshots and prints
pocket-core's bench lines. In bench runs pocket-core also reports `draw … ms/frame` (picture, frame and
HUD until the GPU finished, `glFinish`); compare a style against `{"frame":"none"}`. `KEYS="3:START 5:A"`
presses pad buttons on the way (past title screens). On the virtual device the GPU is llvmpipe (the
CPU), so only the relative cost means something there; the device numbers come from the same bench.

Measured 2026-10-22 in the virtual Mini V2 (llvmpipe, host busy, Integer smaller; median `draw` ms per
frame, the whole frame drawn on the CPU, so this mostly counts pixels):

| | None | Auto | Ambient | Shell | CRT plastic | CRT wood | Arcade | Glow |
|---|---|---|---|---|---|---|---|---|
| GBC uCity | 3.2 | 3.7 | 6.5 | 5.0 | 5.6 | 4.1 | 4.0 | 4.0 |
| GBA MegaMan & Bass | 2.7 | 4.8 | 9.2 | 4.1 | 3.2 | 3.8 | 2.9 | 2.9 |
| NES Nova | 1.8 | 2.9 | 5.2 | 3.2 | 3.8 | 4.7 | 5.9 | 5.8 |

The static frames cost one texture tap per border pixel, as Auto always did (the NES Arcade/Glow runs
came last, with more host load). Ambient adds the tiny passes and, every 3rd frame, the composed glow
(149 000 pixels at 4 taps); on llvmpipe that is 3 to 6 ms, on the Adreno 650 it is a fraction of a
millisecond by the pixel counts (check on the device: the `draw` line of a bench run at 587 MHz).
Drawing a static frame on the CPU (only when the geometry or the setting changes and it is not in the
[cache](#cache), `bezel: prepared in … (drawn …)`), spread over the cores: Auto 5-14 ms, Glow 4-13, Arcade 14-16,
Shell 15-75, CRT 28-48. Handheld and Monitor were drawn on one core until 2026-10-23 (`bezel::classic`, 46-57 ms
for the SNES's Monitor on the Mini V2); they use `frames::Canvas::paint` now, like the others, which also skips
the picture's span of each row in one step (desktop, all styles at six geometries: 2.1 s → 1.6 s; Handheld and
Monitor 2-3× faster).
The newer frames, measured the same way on the desktop (`--frame-demo`, Apple M-series, 2026-10-23, so
only the order matters; Shell clear purple 22.5 ms for comparison): Brick 11.5 (clear 15.5), Pocket slim
6-13, Clamshell 12.7, Wide 33.5 (its outline is a power function per pixel), Twin 15.3, Gloss 11.0,
Swan 13.9, Watch 15.9, Goggles 21.0, Studio monitor 15.5, Portable TV 15.8, Woodgrain 15.3. The power
light adds one ≤ 66 × 66 px repaint and upload when it changes (check on the device: `bezel: prepared in`).

At Native on 1240×1080:

| System | Picture | Frame sides (left/right, top/bottom) |
|---|---|---|
| GB / GBC 7× | 1120×1008 at (60, 36) | 51 px, 31 px; power light on the left |
| GBA 5× | 1200×800 at (20, 140) | 16 px, 68 px; power light top left |
| NES 4× | 1170×896 at (35, 92) | 30 px, 63 px (only with Frame set to Auto) |
| SNES 4× | 1195×896 at (22, 92) | 18 px, 63 px (only with Frame set to Auto) |

### Cache
A frame is drawn on the CPU and uploaded to a texture once per picture geometry and setting (`bezel::Layer`).
Since 2026-10-23 it is also kept in RAM between game starts (`src/frame_cache.rs`), and at a game's start it is
never on the way to the first frame:

- **Off the first frame.** pocket-core starts with `Renderer::set_frame_async(true)`: a frame that isn't on the
  GPU yet is read from the cache or drawn on a thread (at a lower priority than the game's threads), and the game's
  first frames are drawn without it: the bands stay black, as pocket-ui's launch picture left them
  (`launch_picture.rs`). The veil that fades the frame in (`Overlay::draw_veil`, 250 ms) starts when the frame is
  there, not at the first frame, so it never pops in: it fades in a frame or two later than before. Once it has
  faded in, pocket-core goes back to waiting for a new frame (a new picture size, a frame chosen in the pause menu);
  the pause menu's preview (`render_offscreen`) always waits for its frame. Before, the frame was drawn on the
  render thread before the first frame: `first frame` in pocket-core's start line was 57-81 ms for GBA (Wide) and
  SNES on the Mini V2 instead of 15-20 ms.
- **Kept between starts.** One file per frame in `$XDG_RUNTIME_DIR/pocket-ui-frames/` (else
  `/tmp/pocket-ui-frames-<uid>/`; RAM on ROCKNIX; `POCKETUI_FRAME_CACHE=<dir>` or `off`), named after a hash of
  everything the frame depends on: the style with its finish or colour, the screen's size, the picture's rectangle
  and scale (which also fixes the DS gap the Twin frame covers), for a pack frame the PNG's path, size and time,
  and `frame_cache::CODE`, a hash the compiler makes of `bezel.rs`, `frames*.rs`, `frame_led.rs` and
  `frame_cache.rs`, so a new build never shows a frame an older one drew. The file repeats the key (a collision is
  a miss), holds the hole, inlay and power light, the pixels with the transparent runs left out (about 1-2 MB
  instead of 5.4) and a checksum; a short, damaged or foreign file is deleted and the frame drawn again. Writes
  are a temporary file renamed over the name, so processes and threads only ever read whole files. The folder is
  kept under 24 MB (`frame_cache::LIMIT`), the least recently used frames go first. Nothing is written to the card:
  after a boot the first start of each frame draws it (on the thread), which costs the first frame nothing.

Log lines (pocket-core, in `/var/log/pocket-ui.log`): `bezel: prepared in X ms (from the cache)` or `(drawn, kept
in the cache)` (on the thread), `bezel: ready X ms after it was asked for` (when the first-frame path picked it
up), `bezel: uploaded in X ms` (the texture). Two `prepared` lines and one `ready` per start are normal: the first
frame comes before sway's resize to the full screen, and the frame for that size is not wanted once the full size
is known (it is still kept). `ready` comes a refresh after `prepared` at the earliest: the result is picked up when
the next frame is drawn.

`cargo test --release --lib frame_fingerprints -- --ignored --nocapture` prints a fingerprint of every style
at six geometries (pixels, hole, light, inlay) and how long each took: run it before and after a change to the
frame code to show the pictures stayed the same pixel for pixel (the cache and the threads above changed none).

## Frame packs (for the store)
A pack is a folder in `/storage/.config/pocket-ui/frames/<pack-id>/`:

```
frames/
  night-handhelds/
    pack.json
    gba.png
    gbc.png
```

`pack.json`:

```json
{
  "name": "Night handhelds",
  "author": "Someone",
  "license": "CC-BY-4.0",
  "frames": [
    { "systems": ["gba"], "image": "gba.png", "hole": [120, 140, 1000, 667] },
    { "systems": ["gb", "gbc"], "image": "gbc.png", "hole": [60, 36, 1120, 1008] }
  ]
}
```

- `image`: a PNG in the same folder (plain file name; paths are refused). RGBA, straight alpha; grey,
  RGB and 16-bit are converted, palette PNGs are not supported. Pixels are composited over black.
- `hole`: `[x, y, w, h]` in image pixels, where the game picture goes. The image is scaled per axis so
  the hole matches the game rectangle exactly, then drawn behind the picture (linear filtering). Draw
  the hole in the console's display aspect (GBA 3:2, GB 10:9, NES/SNES 4:3 with 224 lines ≈ 1.52).
  Anything of the image that ends up beyond the panel is cut off, so design for the margins in the
  table above, or make the art at the size of the screen with the hole at the Native position.
- `name`, `author`, `license` are for the store's listing. Only art whose licence allows redistribution
  can be shipped; record it there.
- Selection: Auto uses the first installed pack (by folder name) with a frame for the console, otherwise
  the built-in frame. A per-pack choice in the menu can come with the store; pocket-core already reads
  the pack folder once per game, so no other change is needed for that.
- Template: `pocket-core --export-frame gba 240 160 <dir>` writes the built-in GBA frame as a pack
  (1240×1080 PNG with the hole at the Native position), a starting point for designers and the test
  pack for the loader. `pocket-core --frame-demo <system> <game.png> <out.png> [scaling] [frame] [colour|cabinet]`
  shows a built-in frame around a raw game frame (e.g. a save-state preview) on the desktop, without
  effects (Ambient is GPU only), and prints how long drawing the frame took.

No third-party art is included. The built-in frames are generated by our own code (GPL-2.0-or-later, like the rest).

## Live preview in the pause menu
Picture settings used to be a loop of change, resume, look, open the menu again. Now:

1. **No SIGSTOP for pocket-core games.** HOME sends `pause` over `/tmp/pocket-core.cmd`: pocket-core
   stops calling `retro_run`, fades the sound out to silence (the stream keeps running, so nothing
   underruns), stops drawing (pocket-ui covers it) and waits for commands. `resume` continues; queued
   sound plays on with a short fade-in, frame pacing and the frame-time statistics restart, and buttons
   still held from the menu (the one that closed it) are hidden from the game until released. Other
   emulators are still paused with SIGSTOP, RetroArch by losing focus.
2. **`preview <w> <h> [<seq>]`.** pocket-core draws its last frame with the current, unsaved settings
   into an offscreen framebuffer at the screen's real size (1:1 output pixels, the same shader, frame
   and geometry as in play), reads it back and writes `/tmp/pocket-core-preview.bin` (temporary file +
   rename): an exact `w`×`h` crop from the screen centre and the whole screen box-filtered to height `h`.
   Format in `src/preview.rs`. Several queued requests are drawn once, after all `set` lines before them.
3. **pocket-ui** asks for a preview when the Picture group opens and after every change, polls the file
   header for its sequence number from a helper thread, and shows **Whole screen** (scaling, frame,
   layout) and **Actual size** (scanlines, LCD grid, CRT mask exactly as in game) between the settings
   and their explanation. The previous preview stays up until the new one is in, so nothing flickers.
   Only the newest answer counts.

Latency is logged on both sides: pocket-core `preview N: … in X ms (render and read back Y ms)`,
pocket-ui `preview N: read after X ms, on screen Y ms after the change`.
