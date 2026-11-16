# Painting small panels natively

Read when you touch `src/gfx.rs`, `src/scaled_layout.rs`, text drawing, or code that reads or writes a canvas's
pixels itself. The plan behind it: `fable52/NATIVE-PAINT.md` in the review folder (packages 0 to 6); this is package 1,
the core.

## What it does

Pages keep laying out in design pixels (`c.w` x `c.h`: 1280x960 on a 640x480 panel, 1440x960 on 720x480, 1440x1440 on
the CubeXX's 720x720). By default (since package 5) the `Canvas` divides by a whole divisor `n` while it paints and
writes straight into the panel's buffer; there is no 1280x960 buffer and no 2:1 filter. `POCKETUI_PAINT=scaled` (or
`old`) is the fallback: everything as before, the design canvas, then `scaled_layout::ScaledFrame`'s filter. Safe mode
always paints that way.

| Panel | design canvas | painted (default) |
|---|---|---|
| 640x480, 720x480, 720x720 | 1280x960, 1440x960, 1440x1440 | `n` = 2, painted into the panel |
| 1280x720 (1707x960), other fractional ratios | as before | scaled as before (packages 0 and 6) |
| 721 lines and more (Brick, Retroid, Odin, HDMI) | the panel itself | unchanged, `n` = 1 |

`scaled_layout::native_divisor(panel)` decides; `ScaledFrame::render` hands `paint` a `Canvas` either way. Safe mode
(`app_rescue`) always paints the old way (`Presenter::draw_scaled_as(Paint::Scaled, …)`). The Showcase's partial frames
(`render_partial`, the Cabinet's TV over the last whole frame) need a whole design frame; painted natively there is none,
so every frame is a whole one (`can_partial` says no) until package 2 gives the TV a native partial frame.

## The canvas at n = 2

| Call | How it paints | Against the old way |
|---|---|---|
| `rect`, `fade`, `clear`, `rows` | edges rounded to the nearest screen pixel, half up; a non-empty span keeps at least one screen pixel (the hairline rule: a 1 px line at an odd place stays a whole line in its full colour) | identical at even places; sharper at odd ones |
| `shade_rect`, `darken_rect` | a screen pixel covered in part is mixed by the share of its design pixels covered (package 5) | identical at even places, within a step at odd ones |
| `shade_rows` | a soft edge of 1 px rows, each its own amount: a screen row is mixed by the mean of its design rows' amounts (one `shade_rect` per row mixed a screen row twice where two rows land on it, twice as dark; package 5 found it at the faded edges of scrolled pages) | within two steps |
| `frame` | each side rounded as `rect` rounds it; a side that would round to nothing keeps the screen pixel its line lies in (the hairline rule), and the top and bottom reach across to the sides: closed at all four corners (four separate rectangles left a corner open at odd places) | identical at even places |
| `fill_rects` | small shapes built from pieces that overlap (an arrow's rows, a check mark's squares, an angle's discs as rows): their union once, each screen pixel by how many of its four design pixels it covers | identical wherever they are |
| `fill_rects_at`, `blend_with` | shapes mixed at an alpha or blended pixel by pixel (soft discs, the Wi-Fi mark, rating stars, the battery graph's fill): the union at an alpha, or a coverage mask averaged per screen pixel | within 3 steps of rounding |
| `disc`, `round_rect`, `ring` | spans per design row, each screen pixel `color` over what is there by how many of its four design pixels are covered (`Canvas::spans`) | identical wherever they are (on a background even within each screen pixel) |
| `blit_scaled`, `blit_region`, `blit_sprite(_to)`, `crossfade` | each screen pixel the mean of its four design pixels' samples (`Canvas::cells`), see-through or outside: the screen pixel | identical |
| `blend` | applied for the top-left design pixel of a screen pixel only | nearest pixel: a shape blended pixel by pixel goes through `blend_with` or `mask` instead |
| text (`Text::draw`, `draw_right`, `draw_clipped`, …) | the glyph cached at half size in four places within a screen pixel (`gfx::half_mask`, `Text::half`) | Departure Mono bit for bit; Barlow, Big text, CJK fallback within 2 steps of rounding |
| `get(x, y)` | the screen pixel that shows design pixel (x, y) | |

The tests compare each primitive with the old picture: `cargo test --profile gate --lib gfx::tests text::tests::painted`.

## Interfaces for packages 2 to 4

`Canvas::buf` is private. Code outside `gfx.rs` uses:

- `c.n()`: 1 or 2. `c.stride()`: screen pixels per row. `c.pixels()` / `c.pixels_mut()`: the screen pixels of this canvas
  (a band of `rows` is its own rows). At `n` = 1 these are the design pixels, exactly as the old `buf`.
- `c.screen_x(x)`, `c.screen_y(y)`: where a design edge lands (rounded as `rect` rounds; `screen_y` is relative to the
  band's own first row). `c.cell(x, y)`: the screen pixel holding design pixel (x, y) and where inside it.
- `c.copy_design(x, y, w, px, see_through)`: a block of design pixels copied 1:1 (memcpy at `n` = 1, the 2:1 mean
  straight from two rows at `n` = 2, bit for bit the old picture). The Shelf look's strip uses it (`paint_shelf`).
- `c.cells(x0, x1, y0, y1, sample)`: the general form, each screen pixel the mean of `sample(x, y)` at its four design
  pixels (`None`: what is there). Exact for any nearest-neighbour picture (blits, turned and sheared cases).
- `c.per_pixel(x0, x1, y0, y1, sample)`: `cells` at any `n` (at `n` = 1 each design pixel set to `sample(x, y)`), for
  pictures worked out pixel by pixel: a gradient, a face turned by columns.
- `c.mask(x, y, w, h, alpha, color)`: an alpha mask in design pixels (averaged at `n` = 2); `c.mask_screen(sx, sy, w, h,
  alpha, color)` with a mask already at screen size (`gfx::half_mask`, cache it per picture and place).
- `c.id()`: what `text::boxes` records a canvas by (at `n` = 1 the old `buf.as_ptr()`); `text::boxes::finish_canvas(c)`.
- **`c.design()` / `c.design_area(x, y, w, h)`**: the guard for code not painted natively yet. It returns a `Design`;
  `design.canvas()` is an `n` = 1 canvas of the design pixels, and when the guard goes the part is averaged back into the
  screen (the old filter, so the picture is right). At `n` = 1 it is the canvas itself and costs nothing. At `n` = 2 it
  costs two passes over its area: `gfx::design_passes()` counts them, `gfx::design_sites()` says where from (file and
  line, calls, screen pixels; the benches print the top 12 as `bench-design-site`), and the packages drive it to zero
  on the measured pages. Pattern used at every converted place:

  ```rust
  // Not painted natively yet (docs/dev/native-paint.md, packages 2–4): the design pixels as before.
  let mut design = c.design();
  let c = &mut design.canvas();
  ```

Where the guard is today (each package replaced its own and removed the guard; since package 5 no file outside `gfx.rs`
may use it, and `tests/code-shape.txt` lists none):

| Package | Files with `design()`/`design_area()` |
|---|---|
| 2 Cases and Cabinet | none left: painted natively (below, "Cases and the Cabinet") |
| 3a Transitions | none left (below) |
| 3b Pages | none left (see below) |
| 4 Shapes and lines | done: `kit::arrow` and `kit::chevron`'s angle, `quick_paint`'s triangles, `setup::tick`, the Kids mode's arrows, clock and backdrop (`fill_rects`, `blend_with`), the Wi-Fi mark, stars, the battery graph, the search's soft box; `frame` and the hairline rule |

**The lists in `tests/code-shape.txt`** (`doc_checks::canvas_pixels_by_hand_stay_in_the_listed_files`): `.buf[` only in
the files listed as `only .buf[` (the canvas itself), a guard only in those listed as `only guard`. A package that paints a
file natively removes its line; a file not listed fails the test. Natively painted code reads and writes through
`pixels_mut` with `stride`, which is not listed.

Package 3a (transitions) is native and has no guard left: `motion::Transition::apply` slides and fades the screen's
rows (a row of the screen takes the look of its first design row; a slide of an odd number of design pixels moves half
a screen pixel, each pixel the mean of two neighbours, as the 2:1 filter showed it), the card's rise (`shift_rows`) moves
screen rows the same way, `motion::layer` (toasts, the
hint bar's crossfade) keeps and mixes the screen pixels under its rectangle, the switcher's glow is kept as the screen
shows it (reduced once with `copy_design` when it is made), and `launch_picture::paint` copies the picture with
`copy_design` (the old picture bit for bit) and mixes it in while it fades. Departure Mono stays bit for bit once a
transition is over; during one at most about 1 % of the pixels differ by more than 32 steps, at edges
(`motion::tests::painted_natively_*`, `launch_picture::tests::painted_natively_*`). Frames of a transition old against
new: `POCKETUI_SNAPSHOT_TRANSITION=<push|pop|fade|rise>:<ms>` (and `POCKETUI_SNAPSHOT_LAUNCH_MS` with
`POCKETUI_SNAPSHOT_LAUNCH_PICTURE`) with `scripts/native-paint-compare.sh`.

Already native without a guard: `app_menu_open` (keeps and puts back the screen's pixels), `quick_paint`'s soft look
(a colour per pixel), `app_appearance`'s dark/light crossfade, `game_card` and `console_page` (`rows`), the page colour
read by `app_motion` and `brick_gpu` (`get`).

Package 3b painted the pages natively: the side column of wide screens (`app_column`, `copy_design`), the Kids
background (kept reduced to the screen once, copied each frame), the manual's band and the Where was I? card (they keep
and put back or mix the screen pixels outside or under them, rounded as `rect` rounds), the Picture page's two images
(`blit_region`), the share picture's wall and the feature previews' picture-by-pixel parts (`Canvas::per_pixel`: `cells`
at any `n`, each pixel worked out by a function).

## Cases and the Cabinet (package 2)

`cabinet_art.rs`, the Shelf and Cabinet looks (`app_cabinet.rs`, `app_cabinet_tv.rs`, `app_room_extras.rs`), the box
(`game_box.rs`) and the scenes into and out of the console (`insert_scene.rs`, `eject.rs`) paint at `n` = 2 without a
guard (`design_passes` 0 on both looks' benches). The tools, all in `cabinet_art.rs`:

- `cells_over(c, x0, x1, y0, y1, f)`: each screen pixel under a design rectangle the mean of `f(x, y, bg)` at its four
  design pixels, `bg` what the screen pixel shows. `f` is the old per-pixel code returning its pixel instead of writing
  it, so a function keeps one body for both ways (`face`, `disc`, `glow`, `ellipse`, the bands, `lamp_glow`, …). Bit
  for bit the old picture where what is painted over is even within each screen pixel.
- `Pic::half(fx, fy)`: a picture's half copy for each of the four places within a screen pixel, made the first time it
  is drawn there and kept with the picture (dropped by `Pic`'s own changing methods and `changed()`, not cloned). `blit`
  and `blit_cols` (a spine cut at the cabinet's walls, nothing made) read one value per screen pixel where all four are
  solid, or identical and translucent (one exact alpha blend); mixed cells and the edges take the four as before. The same picture at shade 256, within one
  step below (the shade is applied after the mean).
- The Shelf look's strip: standing still, each tile of the row is kept at half size per place within a screen pixel
  (`half_tile`, beside the tile in `ShelfRow::halves`) and `paint_strip` copies its rows; a cell across two tiles or
  the strip's edge takes the mean of its four. Bit for bit the old picture, and the strip is never put together in
  design pixels (on the H700 that was about 2.8 MB of memory traffic a frame). Leaning (Playful animations), the rows
  are put together as before and copied by `copy_solid` (a solid block at any place, odd rows included) or, with
  columns no tile covers, `Canvas::copy_design`.
- `fade_cols` (where the row goes on past an end) mixes once per screen pixel with the mean share of its four design
  pixels: within a step of the old picture.
- `blit` at `n` = 2 copies solid cells of a whole row from the half copy in one tight loop and blends identical
  translucent cells once; mixed cells and the edges take the four design pixels (package 5: the Cabinet a fifth faster on the Air at 640x480 and 720x720).
- `rows_over`: `cells_over` for a band whose look depends on the row only (`dim`, `fade_band`, `fade_band_to`), worked out once
  per design row instead of once per design pixel; bit for bit the same.
- `soft_shadow` at `n` = 2 multiplies the mean fall-off across by the mean down (exact for the product, within two
  steps of the old rounding).

Where two pictures meet inside one screen pixel (a turning case's spine and front), the second is painted over the
mean of the first: one column of the seam may be a step or two off. Tests: `cargo test --profile gate --lib cabinet_art`.

## Checking

- `scripts/native-paint-compare.sh` renders every state of `tests/i18n-states.txt` both ways (Barlow and Retro, dark and
  light, Normal and Large, 640x480 and 720x480; `SMALL`, `FONTS`, `LANGS` choose others) and writes the share of pixels
  more than 32 steps apart per state, the findings and the text each state shows that differ (must be none: the layout
  is the same; states that read the real clock or made-up saves differ from run to run on their own) and contact sheets
  of the 40 states that differ most. With
  `BIN_OLD` (a build of main) it also checks that the pictures at 1240x1080, 1920x1080 and 1280x960 are unchanged.
- `--snapshot` follows `POCKETUI_PAINT` (`snap_canvas` in `main.rs`): painted natively, the PNG is the panel's own picture.
  `POCKETUI_SNAPSHOT_NATIVE=1` is the older probe (the page laid out at the panel's size) and turns native painting off.
- `bench-shelf` and `POCKETUI_FRAME_BENCH` time the live frame (the reduction included, or painted natively) and print
  `paint=Scaled|Native`.

A box back keeps its finished screen pixels, including film, while read. Content equality (including downloaded descriptions, save progress and facts), picture ownership, console art, scroll/selected link, panel/phase, palette, font/text size, language and the relative save time invalidate it. A changed back is painted by the same native path before being saved; a repeated frame copies the screen cells exactly. Text/layout/navigation diagnostics repaint so their final frame still registers every visible word.
