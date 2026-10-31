# pocket-ui visual design: "the shelf"

## Subject, audience, job
- **Subject:** a personal retro handheld. Its games used to be physical things: cartridges, discs, tapes,
  each recognisable by its spine on a shelf.
- **Audience:** one owner who cares about physical media and craftsmanship (the hi-fi/record-shelf world).
- **Primary job:** get into a game within two presses, and back into the last game with one.

## Concept
Every list is a shelf of **spines**. In the Shelf look (the default) every game stands on one shelf as its
system's real case, seen from the spine: a SNES game's cardboard box, a PlayStation jewel case, a PS2 DVD case. The
game itself (the cartridge, card or disc) is inside: it comes out when the box is opened and goes into the console when
the game starts. The selected
one is **pulled out**: it rises out of the row and is shown large above the shelf, turned to its front with the
cover. This is the one memorable element; everything around it stays quiet. (The other looks keep the older idea,
a list of spines with an edge stripe.)
(Socket inspired the physical-media idea; NextUI inspired the flat, minimal list and "continue first".)

## Design language
One product, two kinds of surface (decided 2026-10-29, "cohesion"; the parts are in `src/kit.rs`). Snapshot states in
brackets show each rule (`pocket-ui --snapshot <state>`, docs/DEVELOPING.md).

- **The shelf** is where games are: spines, cases, the paper label, wood, props, the game box. It is pixel art and
  physical (`home`, `cabinet:dd`, `box:gba:advance`).
- **The sheet** is where things are set: Settings and its pages, quick settings, the pause menu, dialogs, safe mode.
  It is flat and calm, built for reading (`settings:0:0`, `sys:quick`, `main`).
- **Where they meet:** the selection is the same paper on both (the pulled-out label's `#ECE9E1`, `select` =
  `label`; the pause menu's title pill too), the neutrals lean warm like paper and walnut (never blue-grey), and the
  sheet borrows the shelf's pixel art only for things: an empty page's vignette, the handheld, the category icons,
  the emblem on a box that is not a game, the memory card or cartridge the save slots stand on (`slot-save:psx`,
  `slot-load:snes`, `src/save_media.rs`). Text is never pixel art; a physical thing is never a flat drawing.

**Type.** Barlow Condensed SemiBold (Title) for names and headings, Barlow Medium (Body) for everything read. Shelf:
Title 67 / 50 / 37, Body 28, hints 21. Sheet: page title Title 50, the pause menu's items Body 46, rows Body 36,
explanations, pills and hint labels Body 30, notes 26. Sentence case, no capitals for emphasis. Every size goes
through `type_scale` (Text size).

**Grid.** Multiples of 6 px. Shelf: a 72 px margin, hints 52 px above the bottom edge. Sheet: pills and bands 30 px
from the edge (`kit::PAD`), text 36 px inside them (`BTN_PAD`, so 66 px from the edge), rows 66 px around Body 36
(`kit::row`), pills 90 px high, 12 px from the top pill to the first row, 24 px between a description and the pill.
The top line of a sheet page: its title in Title 50 at the left, the status corner in its pill at the right, the title
cut with "…" 24 px short of the pill (`App::sheet_header`).

**Radii.** A pill, a band, a key and a row's selection are round at half their height. A panel (a live readout or
a state, not a choice: Performance & fan's "Now", safe mode's "What happened", the status of Tailscale and
RetroAchievements) is the pill colour with 24 px corners (`kit::panel`). A sheet over a page (a dialog, a picker) is
square paper with a hairline and, in Light, the label's 6 px shadow (`kit::sheet`), like a card laid on the table.
Over a game: the power menu 40 px corners; a toast (a game that didn't start) the OSD's surface and corners at the
OSD's height, clear of the top line's ends or across the whole width, never half over the clock.

**Colour roles** (tables under [Appearance](#appearance-dark-and-light-srcpaletters-srcappearancers)): page
(`void`), surface (`spine` on the shelf, `pill` on the sheet), paper (`label` = `select`: the selection, the
pulled-out label, the pause title), ink on paper, `text`/`chalk` on the page, `quiet`/`dust` for what explains,
`band` behind a selected row (a warm grey; over a game the game's system stripe), `amber` for Favourites and
suggestions, `alert` for what went wrong. Colour that means something comes from content (system stripes, covers),
never from a UI accent.

**Lists.** One row style per surface. Sheet: the name 36 px in, the value at the right in the row's colour, the
selection a band across with the name on a paper pill (`kit::selection`, `kit::row_label`), a 4 px scroll bar at
the right edge when the list is longer than its window (`kit::scrollbar`). Shelf lists (search, the Lens, first
start): spine-coloured rows, the selected one paper with ink, the label's lift shadow in Light. The sheet's lists
wrap at their ends; the shelf stops at its ends (it is a physical shelf with bookends).

**Hint bar.** Shelf pages: loose round key caps with Dust labels at the bottom left, A first (`kit::page_hints`).
Sheet pages: one pill at the bottom right, A last, next to the face buttons (`kit::pill_hints`). The explanation of
the selected row stands at the bottom left, never under the pill: its last line on the pill's line, the two lowest
lines in the column beside the pill, longer ones across the page above (`kit::footer`: `settings:0:0`, `sys:quick`,
`options`, `picture`). Every page has hints, the pause menu's pages too: B Back first in the pill, then A Open (a
row that opens a page), A Choose (an action) or "< >" Change (a value). Same word, same action: Back goes up a level,
Close shuts an overlay (quick settings), Cancel leaves a dialog. Labels go before keys when the line is full; at
most five keys.

**Empty pages.** The vignette above the words, left aligned with them on the text column; the heading in Title 37,
what to do next in Body 30 Quiet, at most three lines (`kit::empty_state`: `gallery:empty`, `tools:empty`). A page
whose words are placed already puts the vignette in the free room under them, aligned with them (`tailscale:offline`).
The search puts it right of the words (the keyboard may take the room below). "Loading…" pages have none.

**Fronts without a cover** are a printed title card: the console (or the box's emblem: Curator's crate of cartridges,
Surprise me's gift, PC streaming's monitor, a sticker's sheet, Favourites' star on a cushion, the search's and the
Lens's glass, a smart shelf's row of cartridges, the history's notebook) over the name, both
centred, the pair a little above the label's middle; the name on two lines of about the same width, never a lone
word or number on the second (`home`, `curator:hub`, `sys:osd-warmth`).

**Icons.** Drawn, one colour, 16 x 16 painted twice their size in the row's colour (Settings' categories); status
marks are drawn (dot, bars, tick, dash); keys are their printed names in chips ("A", "SELECT", "< >"), never glyphs.

**Motion** (details under [Motion](#motion-srcmotionrs-srcapp_motionrs)): only in answer to input, ease-out, no
bounce. A row's selection glides ~150 ms, a page comes in 28 px in 150 ms, the shelf lifts 120 ms and turns 220 ms,
the game box flips in 280 ms, the launch puts the cartridge in at 180 ms and is done at 300 ms, toasts and OSD come
down 20 px in 150 ms, hints crossfade in 120 ms. Motion Off: everything at once.

**Sound** (`src/ui_sound.rs`, the sample set in `assets/ui-sounds`): one sound per kind of feedback, the same on
both surfaces. Selection or value moved: Nav. Chosen, switched, the box turned: Confirm. Up a level or closed: Back.
Pause menu or quick settings opened: Menu. Switcher: Switcher. Refused: Error. Vol+/Vol-: Volume. Screenshot:
Shutter. X on a game in the Shelf or Cabinet: CaseOpen (plastic) or BoxOpen (a cartridge's cardboard box), instead of
Confirm. The launch, Quick: Insert, a quiet click (-6 dB) that lands with the motion at 180 ms. Cozy: the case opens at
A, the game goes in with its medium's sound (CartIn, DiscIn, CardIn), its click on `IN_AT_COZY`, and CrtOn with the
old-TV power-on (pocket-core plays it into the game's sound). Never during play.

**Voice.** Plain gamer words, short, sentence case; say what happens, then what to do ("No screenshots yet" / "Hold
HOME and press A while you play to take one."). No hi-fi or engineering jargon, no exclamation marks, "…" for
something under way. German: Du, the glossary's words (docs/i18n-glossary.md: Spielstand, WLAN, Regal), „…“
quotes, "45 %" with a space.

## Tokens
| Name | Hex | Use |
|---|---|---|
| Void | `#000000` | background. True black, because the panel is AMOLED and black pixels are off |
| Spine | `#1F1F22` | unselected spine body |
| Label | `#ECE9E1` | pulled-out label paper |
| Ink | `#151515` | text on the label |
| Chalk | `#D9D6CE` | text on black |
| Dust | `#8A877F` | secondary text and hints |

System stripes (content-driven colour, not a single UI accent): NES `#C8102E`, GB `#8A9A3B`,
GBC `#2E8C9A`, GBA `#5B4AA6`, SNES `#7A6CB0`, Mega Drive `#2B2B2B` + `#D4AF37` rule,
PS1 `#9AA3AD`, N64 `#2E7D32`, PSP `#3B3B3B`, others hashed from the system name into a muted range.

## Type
One family in two widths: **Barlow Condensed SemiBold** for spine titles (condensed type fits spines, like
printed cartridge and label-maker type) and **Barlow Medium** for meta text and hints. Both are OFL, embedded.
Scale, a perfect fourth (×1.333) from 21 px: 21 / 28 / 37 / 50 / 67. Pulled-out titles are 67, spines 37,
meta 28, hints 21. Sentence case everywhere, never all caps. The panel is 3.92" at 1240×1080 (about 16 px a
millimetre) and held at arm's length, so Body 28 (capitals about 1.2 mm high) is the smallest size for text that is
read rather than glanced at (a fact, an explanation, a note under a list); 21 is for the hint bar, credits and short
notes beside larger text. Section headings on a page are Title 37 (the console page, credits, the game card).

### Font styles (`src/font_style.rs`)
Settings → Look → Font (settings.json `"font"`: `barlow`, `pixel`, `rounded`; Barlow when unset or wrong) swaps the
two faces; everything above (the scale, Body 28 as the smallest read text) holds for each:

| Style | Title (names, spines, labels) | Body (meta, hints) |
|---|---|---|
| Barlow | Barlow Condensed SemiBold | Barlow Medium |
| Pixel | Pixelify Sans SemiBold | Barlow Medium, so that small text stays readable |
| Rounded | Nunito ExtraBold | Nunito SemiBold |

All OFL 1.1, static Latin instances compiled in (about 100 KB for the two new families; `assets/fonts/OFL.txt`). Each
alternative face is **fitted to Barlow's capitals**: it is rasterised at a size scaled so that its "H" (bounds, top to
bottom) is as high as the Barlow face's at the same size (the idea of `text_fallback::Fit` for Japanese letters), so baselines,
row heights and `type_scale` keep working; only the widths differ, and Rounded's are wider than Barlow Condensed's.
Painters therefore measure (`Text::width`, `fit`, `wrap`) and never assume Barlow's widths. A pixel font is crisp
near whole multiples of its pixel grid and a little soft in between; that is accepted. Letters a style's font lacks
come from the CJK fallback as before. A store theme's `fonts/title.ttf` and `body.ttf` replace the face they name
whatever the style says; safe mode is always Barlow. `tests/font_styles.rs` runs the layout-risk states at 640×480, Large,
German for Pixel and Rounded with the same clipping checks as `tests/text_size.rs`.

### Japanese, Chinese and Korean names
Barlow has no kana or kanji. A letter it lacks comes from the system's **Noto Sans CJK** (ROCKNIX installs
`/usr/share/fonts/truetype/noto-cjk/NotoSansCJKsc-Regular.otf`; we don't ship it), so "ファイナルファンタジーVI" and
"スーパーマリオワールド" read as they should on the shelf, the card and in search. It is fitted to the face it stands in for
(`text::fits`, `text_fallback::Fit`): an ideograph ("国") is as high as Barlow's capitals and a sixth, its middle on
the capitals' middle, so kana and Latin letters share a line; and it is painted bolder (the outline drawn again up to
0.04 em right and up next to Barlow Condensed SemiBold, 0.018 em next to Barlow Medium; on a small circle above 1.5 px,
so a big letter keeps clean diagonals). Kana keep their full width. Only letters Barlow (or a theme's font) lacks come
from it; Latin letters and "?" stay Barlow to the pixel. The virtual device has the font too (its root is the ROCKNIX image's). Without it (a desktop computer) a letter
is painted as "?" and a mark (a tick, a star, an emoji) is left out, never an empty box, as before; the same for a
character neither font has.

**Cost.** fontdue turns every glyph of a font into an outline when it loads it, for this font seconds and well over
100 MB, so the CJK font is read another way (`src/text_fallback.rs`): the file is memory-mapped (only the pages a
glyph touches are read, and the kernel can drop them), `ttf-parser` outlines one glyph when it is asked for and a small
rasteriser turns it into a coverage bitmap like fontdue's. Opening it costs 0.08-0.09 ms and a glyph 20-45 µs at 37 to
67 px (the build machine while other builds ran, file in the page cache; `cargo test --lib fallback_cost -- --nocapture`). It is opened at most once per
process, the first time a letter needs it. Once the first frame is on screen, and when the library changes or the text
size does, a thread (`Text::prewarm_names`) draws the CJK letters of the games' names (the 1,200 most frequent) at the
sizes names are painted at (Title 37, 44, 50, 67; Body 28, 30), so a shelf of Japanese titles finds them in the
cache; the UI thread only picks out the names that aren't plain ASCII. A library without such names starts no thread
and never opens the font. The CJK glyphs have a cache of their own, at most 8 MB (`FALLBACK_BYTES`, about 3,000
letters at those sizes); past that it is emptied and letters are drawn again as they come.

### Text size
Settings → Look → **Text size**: **Normal** (the scale above), **Medium** or **Large**, saved as `"text_size": "medium"` or `"large"`
in settings.json (Normal is not written; a file from before Medium reads as it did). One rule for every size, applied in one place (`src/type_scale.rs`): the painters
keep asking for the design's sizes and `Text` paints, measures, fits and wraps each at `type_scale::px(size)`, so a
width that a layout measures is the width it gets. Large is a quarter more, at most 10 px more and never past 64;
Medium is 1.12 times, at most 6 px more, never past 64, rounded to the nearest pixel:

| Design | 20 | 21 | 24 | 26 | 28 | 30 | 36 | 37 | 44 | 46 | 50 | 67 and up |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Medium | 22 | 24 | 27 | 29 | 31 | 34 | 40 | 41 | 49 | 52 | 56 | the same |
| Large | 25 | 26 | 30 | 33 | 35 | 38 | 45 | 46 | 54 | 56 | 60 | the same |

Medium was added after trying Large on the device ("an extra step in between would be ideal"): Body 28 becomes 31 and
the hint bar's 21 becomes 24, about half the way to Large, and the rows (`row`: 70 px for Settings' 66) follow by the
same rule. Large's Body 28 becomes 35 (capitals about 1.5 mm high instead of 1.2; the 2.2 mm of reading glasses would need 44-52 px, too
big for the panel's rows). Titles of 64 px and more (a pulled-out label, the switcher, the battery figure, the letter
of a jump) are large already. A third, larger step was left out: in Large the densest pages (Sync, Performance & fan,
the look step) already scroll their list or shorten a note to stay above the hints, and half as much again would not
fit them.

The layouts follow the same rule: a row that was `h` px around type of `font` px is `type_scale::row(h, font)` (taller by
the type's growth and a fifth of it again: Settings' 66 px rows at 36 px are 76, a 72 px spine 82), the step between two
lines is `type_scale::line(font, gap)` (the line grows, the gap stays), a baseline in a row's middle is
`type_scale::mid(font)`, and a column of labels is `type_scale::wide(w, font)`. So a page shows fewer rows in Large and
scrolls sooner; nothing else moves. The pause menu's Picture page shows as many of its rows as leave its live preview
at least 30 % of the screen's height (`preview::rows_above`, `LARGE_MIN_SHARE`; three rows at least, the selected one
and its neighbours, the list scrolling with a bar at the right edge) and keeps no more than the five lines its
explanation shows free below; with the 7 rows of a GBA game it would otherwise be a strip about 40 px high. Normal
shows every row, as before. Where there is no room, text is cut with "…" or wrapped (`Text::fit`,
`Text::wrap`, `Text::paragraph`, which ends the last line it has room for with "…"), never in the middle of a word without
it, and a hint bar lets the last labels go and keeps their keys. The loose Shelf-family bar keeps the gaps between
keys when labels go; its width budget includes those gaps, so the last key stays inside native 1024×768 at Large too.
A Settings list's description of the selected row
sits beside the hint bar on its line; when it does not fit there (German, Large) it takes up to two lines above the
bar, and the category keeps that room for all its rows, so the list does not move as the selection does. Fixed pictures keep the design's sizes
(`type_scale::unscaled`/`fixed`): the titles on the Cabinet's case spines and its shelf cards (as thick as the case),
the Tour's pictures of screens. pocket-core's overlay over a game is not scaled.

**Cost.** Normal is the design to the pixel and costs one thread-local read per string. The glyph cache is keyed by
the size painted, so Normal and Large sit side by side: switching draws the new size's glyphs once (on the prewarm
thread, which starts again at the switch), nothing is thrown away or drawn twice. The manual lays its pages out again
for the new size (its layouts are kept per width and size).

**Checked by measuring, not by eye.** `POCKETUI_TEXT_SIZE=medium` or `large` overrides the setting for a snapshot, and
`POCKETUI_TEXT_BOXES=<file>` makes the text painter record where each string's ink went (`text::boxes`). The snapshot
writes one line for text that runs into another string or into a hint bar (`O`), for two strings on one line closer than
a space (`T`) and for text cut by the left or right edge of what it is painted into (`E`). `tests/text_size.rs` renders
every state of `tests/i18n-states.txt` in Medium and in Large, English and German, and fails on any line. Pages mark what is covered
(a dialog, a sheet), painted over (Up next's card), covered by an overlay of their own (the letter an L2/R2 jump shows
over the list) or cut on purpose (the Cards look's neighbours).

## Phones (Android)

The same shelf and sheet, held in one hand and touched (`android/src/phone.rs`, docs/ANDROID-MVP.md). Sizes are in
dp from the phone's density instead of handheld pixels: Body 16, sheet rows 56 dp, list rows 64, buttons 48-52 dp
high and round at half their height, 16 dp margins in portrait and 24 in landscape. The tabs go to the bottom with
the Settings icons' style (16 x 16, one colour: cartridge, star, glass, crate, cog); the current tab sits on a paper
pill. The primary action of a page (Play, Continue, Show games, Choose your ROM folder) is paper with ink across
the width; the others are pills in the sheet's colour, a favourite's in amber. Hint keys only show while a
controller is the input. Portrait turns the shelf into stacked boards of fronts; landscape keeps the handheld shelf.

## Small-panel presentation

On panels up to 720 px tall, compose the same supported layout on a logical canvas, then reduce the completed
0RGB frame (`scaled_layout`, `frame::Presenter::draw_scaled`). 640×480 uses 1280×960, 720×480 uses 1440×960,
and 720×720 uses 1280×1280. This keeps margins, shelves, panels, text and hit areas in one coordinate space;
page painters must not mix physical panel coordinates into their layout. `App::screen_size` returns logical dimensions.
The actual output wins over the profile, so a large HDMI output keeps its native layout. `pocket-core`'s game
rendering and touch mapping do not use this UI scaler.

Use a rounded four-pixel box average for exact 2:1 reduction and exact area coverage for fractional reductions.
Keep the canvas and coverage tables between frames, and reduce only when the UI already needs a frame. Text is
rasterised on the logical canvas with the rest of the page; there is no second text pass that could move glyphs away
from their measured layout. Normal, Medium and Large remain the player's text-size choices. Snapshots use the same
logical dimensions and reduction; their text-box findings describe the logical canvas before reduction.
[H700 scaled-layout measurements](H700-SCALED-LAYOUT.md) record JPEG inspection and performance limits.

## Layout (1240×1080, left aligned, 72 px margin)
```
 place (left, Dust)                    13:28  ((·  76 % (status corner, right, Dust)

 ┌────────────┐  Continue                          ← kicker (Body 28, Dust)
 │  the case's │  Chrono Trigger ★                  ← name, Title 67, two lines at most
 │  front with │  Last played 2 days ago            ← meta line(s), Body 28
 │  the cover  │  (a system: its fact sheet; Curator, Favourites: their covers)
 └────────────┘
 ┃┃ ┃┃┃┃▲┃┃┃┃┃┃┃┃┃ ┃┃┃┃┃┃     ┐▒   (plant)            ← spines standing on the board; the selected one lifted
 ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀        ← the shelf board (walnut dark, oak light)
 (A) Play   (B) Back                                  ← hints, bottom left, Dust
```
- **Home:** Curator and Search, the recent games (the last one is "Continue"), Favourites and the pinned rows,
  PC streaming, then the systems; a wider gap on the shelf where one group ends. Systems lead to that system's game
  shelf (same component). Home opens on Continue.
- **One visual language** (the owner's decision, 2026-10-28, after the trailer showed lists of bars with a small
  cover beside a cabinet of tiny cases): every entry is a spine standing on the same board, the same light and the
  same baseline for all; only the case's shape changes, and heights differ only where the real media do. Games stand
  as their system's case (`media::Medium::of`), systems, Curator, Favourites, Search and PC streaming as plain
  DVD-size boxes in their colours with their names (a system's short name: "SNES").
- **Scale** (`app_cabinet.rs` `shelf_layout`): the tallest case (a DVD case, 120 template pixels) takes about two
  fifths of the room between the top line and the hints, in half steps so the pixel art stays even: three screen
  pixels a template pixel at 1080 lines (a DVD case 360 px, a SNES cartridge 66 px thick), 2.5 at 960, 1.5 at 720, one
  at 480. The same on every list of a screen, nearest pixel, never smoothed.
- **The shown case** (`show_k`): its front as large as the room above the row's tallest case lets it, at most 45 % of
  the screen's width and six template pixels a pixel, in half steps; a shelf of cartridges leaves it more room than a
  shelf of DVD cases. It is never cut: `shelf_tests` checks every medium at 1240x1080, 1920x1080, 1280x960, 1280x720,
  1024x768 and 640x480. The cover is in the front's label (`media::front`); a game without one, a system (its console
  above its name) and the rest get a printed title card in the cover's place, never an empty hole. While a cover is
  still being read the label stays plain for a moment.
- **Spine titles** (`media::spine_with`, `spine_px`): the cover's main colour on the label (`media::cover_colors`, the
  system's colour without a cover), the title read top to bottom at the text size's size (Normal 30 px, Large 37 on a
  wide label at 1080 lines), never wider than the label with paper on both sides; a long title ends in "…", nothing
  scrolls on a spine. Sizes are the ones the app paints anyway, so a spine made in a frame draws no letters of its own.
- **Where the shelf rests:** the selection keeps a fifth of the shelf beside it; the row glides (220 ms, Motion Off:
  jumps) and fades into the page where it goes on past an end. A row that fits stands still from the left; where it
  ends a bookend holds it up and a plant, lamp or cactus (`media::Prop`, the same for the same list) stands further
  along, so a system with three games is not an empty board.
- **On a wide screen** (1920x1080) the shelf stands in the 1440 px column in the middle, as Settings does
  (`app_column`): more spines in view than on the Mini V2, the same sizes.
- **Status corner** (`src/status_corner.rs`): the top line's right end on Home (every look; Chalk over the Cards
  wash), the game card, console pages, the search and Curator's pages. The time (24 h), a Wi-Fi symbol only while
  connected (a fan of a wedge and two arcs, 45° to either side, drawn with the canvas, as big as the figures), the
  battery ("76 %", "76 %, charging"), 24 px apart, Body 28. A part that is not known is left out, not shown as a
  placeholder. The place at the left is cut short 32 px before the corner, and so is a note left of it ("Saves
  synced"), so longer words (another language) push it aside instead of running into it. Settings, its pages, the
  quick settings and the pause menu's pages, whose top line is a row of pills, show the same parts in a pill
  (`status_corner::Pill`: Body 30, Soft on the pill colour, 36 px inside); a heading left of it stops 24 px short of it.
- **Settings rows** have their value at the right in the row's colour: a switch its state ("On", "‹ Home ›" while
  selected), a page a short value and ">" ("Home >", "Shelf · Dark >", "332 games >"), or ">" alone where there is
  nothing to say or the value is still being looked up (`settings_values`). A value takes at most 440 px and the room
  the name leaves, and is cut with "…" past that; the ">" stays.
- **Empty pages have a picture** (`src/ui_art.rs`, `assets/ui-art`, sources in its SOURCES.md): a small pixel
  vignette above the words, twice its pixels (once on a screen under 900 lines), its bottom 28 px above the heading's
  capitals; the words move down only as far as it needs (`ui_art::above_words`). An empty shelf with a plant (no
  games), a handheld under a blanket (nothing played), a camera with a blank photo (no screenshots), a magnifying glass
  by an empty box (no match; in search right of the words, the keyboard may take the room below), a market stall (an
  empty store), a radio asleep under a moon (no network: Wi-Fi, Tailscale, a store that can't be reached), a bell and
  an hourglass (a hotel's sign-in), an open blank notebook (no history), a star on a cushion (no favourites, in the
  Favourites panel). "Getting ready…" and "Loading…" have none. Where a page has no room left the picture is left
  out, never the words. The same house style as the console pictures: one dark outline pixel, three or four tones a
  material, transparent, warm and muted; each picture is decoded the first time it is painted and kept (a vignette
  costs about 20 µs a frame).
- **Settings' categories have icons**: 16 x 16, one colour, painted twice their size in the colour of the row's name,
  so one picture fits dark and light, selected or not.
- **The handheld:** first start's welcome and About show the device pocket-ui runs on (`device::current()`, the Mini
  V2 where it isn't known), drawn in pixels with pocket-ui's shelf on its screen; About only where the room above the
  hints holds it. **pocket-ui's mark** (the cartridge with the four system stripes and a "p" on its label,
  `ui_art::Art::Logo`): on the welcome it stands beside the handheld on its bottom line at twice the handheld's pixel
  scale, so both are about as tall (`ui_art::above_words_row`); on About it sits before the version on the Version row,
  one pixel a pixel, where it clears the row's name. The boot screen shows the mark alone over the wordmark ([BOOT-SPLASH.md](BOOT-SPLASH.md)). The manual's Controls page shows the Mini V2 three or two times its pixels, every control named
  beside it: button names as the text's chips, "D-pad" and the sticks in the interface's language, the face buttons
  lettered as Settings › Playing › Button layout has them (`manual::controls_picture`, from the line of
  `docs/user/controls.md` whose image line points at `pocket-ui:controls`).
- **Never a blank where a figure goes.** A preview, panel or side text that has nothing to show yet says so in words
  ("Getting ready…", "Nothing this week", "No PC paired"), and the Lens with nothing picked shows what it looks
  through ("332 games · 18 filters"). Split's panel shows the side text large, so it is never an empty slate.
- **Launch:** the label stays where it is and the rest of the shelf fades to black, then the game takes over.
  On return, the shelf comes back with the same spine pulled out.

## Motion (`src/motion.rs`, `src/app_motion.rs`)
Motion is there to make the device feel quicker, like the well-damped needle of a good amplifier: it shows
where something went, never makes anyone wait for it. Only in answer to input; nothing moves on its own, with
one exception: a selected title too long for its row scrolls (rest at least 1.5 s with an ellipsis, then about 1.7 × its font size in
pixels per second, `src/marquee.rs`). A single text layer travels to the end, pauses, and returns to the
full beginning. Changing selection or available width resets it; fitting titles also reset the selection key.

**Rules.**
- *Input first.* A press changes the state at once (selection, page, value); the animation is only a picture of
  the way there, worked out from the state on every frame. Pressing again mid-way retargets from wherever the
  picture is; nothing queues, nothing waits for an animation to end. No animation delays a game's start (the
  launch fade and the cabinet's cartridge belong to the start path, not to this module), a page or a press.
- *Ease-out, 100-200 ms.* A glide approaches its target exponentially (time constant 28 ms: a row settles in about
  150 ms, a five-row jump in about 200 ms), the most movement in the first frame. Values are taken for the moment
  the frame reaches the screen, one refresh after it is painted, so the first frame after a press already shows
  about 40 % of the move.
- *No frame without movement.* Every animation knows when it has settled and reports it to one signal
  (`motion::busy`), which `App::animating` asks; then the loop sleeps until the next input (idle CPU and
  wake-ups of the UI thread are the same as without motion). Animations draw at most once
  per 60 Hz refresh (`FRAME` in main.rs).
- *Calm.* Short distances (28 px for a page), fades rather than flights, no bounce, no overshoot, nothing that
  spins or zooms past its place.

**What moves.**
| Where | What | Time |
|---|---|---|
| Shelf look | the selected spine rises out of the row (the one before goes back), the case above the shelf turns from its spine to its front (`SHELF_TURN`); the shelf glides along under its faded ends, a spine at a time and five on L1/R1 | lift 120 ms, turn 220 ms, glide 220 ms |
| Minimal, Split, Settings list | the pill / the system-coloured bar / the band glides to the new row, the pill's width follows the name; a name turns to the highlight's text colour as it comes under it; the list scrolls under its edges | ~150 ms |
| Cards look | the carousel slides; size, shade and place of every card follow its distance from the middle; the card in the middle is lifted (3 % larger, a soft shadow on paper); the wash behind turns from one card's colour to the next | ~150 ms |
| Pages | a deeper page (a system's shelf, Settings and its pages, the console page) comes in from 28 px to the right and fades up from the page colour, its list rows one after another (16 ms apart, at most four steps); going back comes in from the left, all at once; the game card rises out of the selected case (the Shelf look) or label; the switcher fades in | 150 ms (+ up to 64 ms) |
| Curator, Search | Curator's list, the Lens, Echoes, Versions and the search are pages like the others (pushed in, popped back; sheets over the card or the list are not pages); their lists scroll smoothly under their edges (the Lens's values start at rest when another facet is picked), the selected row turns to paper at once | 150 ms |
| Letter jump (L2/R2) | the big letter shows, then fades out as one layer (box and letter together) over 220 ms; one frame is drawn after it has gone, so nothing faint stays behind | 700 ms in all |
| Hint bar | when the hints change, the old ones fade out quickly while the new ones fade in | 120 ms |
| Toasts, OSD | come down 20 px into place and fade in, fade out at the end; an OSD that is up stays put while its level changes | 150 ms |
| Switcher | the row of screens slides (retargeted mid-slide) | ~200 ms |
| Cabinet | case pull/push and shelf scroll as before (`cabinet.rs`), and they follow the Motion setting | 160-240 ms |
| Split's preview | after 2.5 s on a game the box art fades to its screenshot, 3.5 s later to its title screen, and round again (`picture_cycle`); a fade is not movement, so Reduced keeps it, Off keeps the box art still | 400 ms |
| Appearance | dark ↔ light crossfade as before; Off switches at once | 600 ms |

**Settings → Sound & feel → Motion** (R1): **Full**; **Reduced**, only fades (pages fade in without sliding, glides and
the pull-out jump, toasts fade); **Off**, everything at once. Saved as `"motion"` in settings.json.

**How a page transition is painted.** No second picture is kept: the old page is gone with the press and the new
page is painted complete in the first frame; the transition then shifts and fades the finished frame towards the
page colour (taken from the frame's right edge), row by row for a list (the painters register their rows). The
fade is branch-free and works on two channels at once, so the compiler vectorises it; a stretch of a line that is
nothing but the page colour is only read, never written (it would come out the same), which is most of a page; a full
frame is a fraction of the paint time. Toasts and the hint bar use `motion::layer` (keep the area, paint, mix), whose cost is their
area. The Cards look fits pictures in its two resting sizes only and scales them on the way (pixel art in five
fixed steps), so a slide never fills the picture caches.

**Measuring.** `POCKETUI_FRAME_LOG=1` logs "press to frame" for every button press (the time from the event
to the frame that shows it) and, when an animation ends, the paint time of its frames by kind (page
transition, glide, other). `POCKETUI_MOTION_SCALE=10` slows every animation of this module ten times down, for
frame sequences on the virtual device (grim is slower than a frame).

## Game switcher (`src/switcher.rs`)
SELECT on the shelf, HOME + SELECT in a game. A row of screens rather than spines: each recent game
is shown as the picture it was left at, in a dark bezel like the handheld's own screen (4:3 box,
760 × 570, neighbours 34 % smaller and dimmed, peeking in at the edges). The selected picture,
shrunk to 16 × 12 and stretched back with bilinear filtering, lights the background as a soft glow,
darkened 70–92 % from top to bottom so the title stays readable. Title 67 px, meta 28 px, one dot per
game, NextUI pills for the heading, status and hints. A game without a picture gets a title card in
its system colour. Slides with an ease-out glide (about 200 ms), retargeted from where it is when pressed again. Favourites: a gold star after the name (`#F2C14E` on dark,
`#B07A12` on the paper label), and a "Favourites" row on Home in amber `#D39A2C`.

## Game pictures (`src/game_pictures.rs`, `src/picture_cycle.rs`, `src/picture_loader.rs`, `src/app_pictures.rs`)
The shelf's second and third picture of a game, the collection's screenshot ("In the game") and title screen. Calm
and rare: they never add a frame, a row or a badge to a list.
- **Split's preview** is the only place that changes by itself: box art alone for 2.5 s, then each picture holds
  3.5 s, 400 ms of it a crossfade, and round again. Selecting another game starts over from its box art. A crossfade
  between two shapes (a portrait box, a landscape screenshot) mixes each picture with the panel colour where the other
  is not, so nothing of the old one stays. Motion Off draws no cycle at all (the box art, still); Reduced keeps the
  fade. The cycle is a function of the time since the selection came to rest (`picture_cycle::step`), evaluated per
  frame; nothing runs between changes but one wake-up (`about_to_wait`), and nothing is read while the selection moves
  (pictures are looked for 400 ms after it rests).
- **The game card** shows them under the two columns of facts, side by side in 220 px high boxes with their name in
  Dust 28 above ("In the game", "Title screen"); a missing one is simply not there, with no heading and no gap. While a
  picture is still being decoded its place is a plain spine-coloured tile of the final size (the size comes from the
  PNG header), so nothing moves when it arrives.
- **Never an empty frame.** A game without a screenshot cycles nothing and shows its box art; without box art but
  with a screenshot it shows the screenshot; with nothing, the console and the count as before. "Box art"
  or "Off" (Settings → Games → Pictures & extras → Pictures) hides them all.
- **Cost.** PNGs are decoded and scaled on one loader thread into the box art's image cache (16 pictures at most),
  never on the UI thread; the files are stat'ed by their 24-byte header once.

## Idle slideshow (`src/slideshow.rs`, `src/app_slideshow.rs`)
A shelf left alone for a minute (half the screen-off time when that is shorter) becomes a calm slideshow of the games'
pictures: box art, scraped screenshots and title screens, and the player's own screenshots and save pictures. Each fades in
over 1.6 s and then moves slowly (zoom 1.0 to 1.12, a drift in one of eight directions) for its 10 s; the first fades in over
the shelf itself. The game's title (Title face, 44 px) and its system (Body, 26 px, "Your screenshot" or "From your save" in
gold after it for the player's own pictures) sit small and centred at the bottom on a dark band, with a shadow; the clock and
battery stand in the top right corner as on Home. A picture the shape of the screen fills it, any other (a portrait box) is
shown whole on a dark backdrop of its own colour. Motion Reduced and Off: no zoom or drift, only the cross-fades. 15 frames a
second while something moves, none between slides when still. Any button or touch ends it and does nothing else; no
pictures, no show. It is Off in every look until the player switches it on in Sleep & power (Slideshow before screen off) ([SLEEP.md](SLEEP.md#idle-slideshow-2026-10-30-srcslideshowrs-srcapp_slideshowrs)).

## Home panels (`src/home_panel.rs`, `src/app_home_panel.rs`)
Where a game's label has its box art, the rows that are not one game show games too, so no pulled-out label is an
empty field. The same 176 px box at the label's right end (at most 600 px wide), and Split's panel (260 px high,
centred).
- **Curator**: "Your library by feel" over the name in the place of "Continue" (Body 28, label Dust), the day's pick
  as a cover with "Today" under it, and a fan of up to three of the games waiting, "4 to rediscover" under it (Body
  21, label Dust: a short note beside larger text). The fan's covers stand a little lower and behind each other, the
  ones behind mixed a third towards the paper. The line under the name names the pick and why; it loses its last
  " · " parts before a word is cut, and the fan gives way before the reason does.
- **Favourites, pinned smart shelves and stickers**: the first games' covers side by side, 112 px slots 12 apart,
  each standing on the box's foot as a case stands on a shelf (a landscape SNES cover is short, a DVD tall), in the
  list's order. No caption: the line under the name says how many.
- **A game without box art** is a tile in its system's stripe colour darkened 70 % towards black (the cabinet's
  coverless case), with its name in Body 21 label paper in whole words, at the design's size in every text size (it
  is a picture). A cover being decoded is a faint tile (the page mixed 40 % towards label Dust); before Curator's index
  three of them stand where the covers will be, with "Getting ready…". An empty list says in words how to fill it.
- **Calm**: nothing moves but the label's own pull; covers come from the image cache and the picture loader's thread
  only, so a panel never decodes on the UI thread. Minimal and Cards show no panel (Cards keeps the spine's line
  large on the card). The Cabinet has its own: Favourites are cases on their shelf, and the Surprise me box has the
  day's pick on its front.

## Game card (`src/game_card.rs`)
X on a game. The game stays a pulled-out paper label (300 px, its system's stripe, title 67 or 50 on two
lines, box art at the right end, the player's status and time at its foot); everything under it is Chalk
and Dust on black in two columns, "About the game" (facts, label column in Dust) and "Your game" (status,
time, note), then the description and "More by …" / "Other games in …" as ordinary spines, each part under a
heading in Title 37 Dust. Only that part
scrolls (up/down, L1/R1), with a 4 px bar on the right and a 36 px fade where text runs under an edge.
The status picker is a short shelf of five spines on a black sheet over the darkened card; the selected
one is paper. Under them, a little apart, the **My rating** spine: a star where the statuses have their marks and five
stars at its right end (Left / Right change them). On the card the rating is a row of "Your game": five small stars,
the given ones in the text colour, the others a dim mix of Dust and the page; never gold, which is the Favourites star.
A card opened by Surprise me has one line over its columns, "Surprise me" in Dust and the reason in amber (the colour of
the card's suggestions). Status marks are drawn, not glyphs: a dot (Playing), two bars (Paused), a tick (Completed),
a dash (Dropped), a thin ring for Not started in the picker only; on the shelf they sit at the spine's
right end in Dust. The one colour outside the stripes is the amber of Favourites, for the "Playing? Press Y"
suggestion. Credits are Dust 21 under what they credit ("Text: Wikipedia, …", "Facts: Wikidata, CC0").

### Info page (`src/game_info.rs`)
X on a game in the Minimal, Cards and Split looks (Shelf and Cabinet keep the box: the collector's looks). One
screen, Chalk and Dust on the page: the cover at the left in the room `game_info::cover_room` gives it (a printed
title card with the console when there is none, the label's lift shadow in Light), the player's lines under it (the
status with its mark across the column, then Played, Last, Rating, Unlocked, a port's Speed and notes, a note), the
text column beside the cover or the player's lines, whichever is wider. In it the title in Title 50 on two lines at
most, system · year in Dust (and "Favourite" in amber), the facts one line each with their keys in Dust, and the
description in the room left above the hints: it scrolls there (up/down, L1/R1) with the card's 36 px fade and 4 px
bar. The data is the box back's (`CardData`, `game_box::about` for the description and its credit). Narrower than
900 px the cover goes on top and the player's lines under the facts. Hints: A Play, B Back, Y Status, START This game,
SELECT Details (the full page; SELECT there comes back). Snapshots: `info:<look>:<system>:<name>`.

## Console page (`src/console_page.rs`)
X on a system with a fact sheet (`assets/data/consoles.json`). Built like the game card, without the paper label:
the system's stripe runs down the whole left edge, the console's name is Title 67 with the fact sheet's headline
under it, and the rest is Chalk and Dust in two columns. Left: the summary, **Timeline** (release per region, written
out as dates, then Discontinued with its note in Dust 21, then Sold), **On this Pocket** (games and the last one
played, time, emulator, BIOS). Right: the console's picture on a box of its colour (none when there is no picture),
**Hardware**, **Launch price**. Under both, across the page: **Family** and the note on the **Sales figure**.
Headings are Title 37. Only the part under the headline scrolls, with the card's fade and bar. The page takes the
Split look's page colour in Split and the plain page elsewhere. The publishers of the sources are named at the
bottom right in Dust 21; Y lists every source. Under a selected system the Split look shows the headline and up to
six label/value rows (46 px apart, values at 28 px, 24 px if that is what fits); rows the room can't take go by
importance (Inside first, Released last).

## Credits & links (`src/credits_page.rs`)
X on a console page. The console page's frame (stripe down the left edge, the status corner at the top right, hints at the
bottom left), "Credits & links" in Title 67 with the console's name under it in the quiet colour, then one scrolling
column: our two-sentence note in Chalk 28, and **People**, **Made by**, **Chips**, **Family**, **Best-selling game**,
**By the makers, in your library** as Title 37 headings with label/value rows (labels in Dust, a 300 px column). The
links are spines as on the game card (64 px, 8 apart, the target's stripe, its relation or shared chip in Dust 21 at the
right end) that turn to paper when selected, each group introduced by a line of Dust 21 ("On this Pocket"). There is
always a selected link while one is on screen: up/down go from link to link and scroll past the text between them, a
link is scrolled clear of the 36 px fades. "Facts: Wikidata (CC0), Wikipedia · summaries written for pocket-ui" in
Dust 21 at the bottom right. No toggle in Settings: the page only exists behind X, and the console page gains one
hint ("X Credits").

## Finding a game (`src/app_search.rs`, `src/search.rs`, `src/search_page.rs`, `src/letters.rs`)
**Search** is the row of Home above Continue in the list looks (under Curator's, which is on top since 2026-10-26), a
plain spine ("All games" at its right end); Home opens on Continue, so Search is one press up and nothing moved for
the player who doesn't use it. The Cabinet
has no row for it: Up on its top shelf opens it, with "UP Search" in the hints there.

The search screen is built from the shelf's own parts. "Search" in Dust 28 where a list's title goes, the query
in Title 50 on a paper field (the label: this is what is being worked on) with an ink caret, the count of games
found at its right end in label Dust ("3 close matches" when a typo in the first letter had to be forgiven to find
anything). Under it the results as spines (88 px while typing, four of them over the
keyboard; 100 px when the keyboard has stepped aside): the system's stripe, the cover in a 72/84 px box (a tile in
the system's colour, darkened towards the spine, with the first letter, while there is none or it is still being
read), the name in Title 37, and the system as a badge at the right end: a pill in the keycap colour with a dot of
the stripe colour and the short name in Body 21. In the list, the selected result is paper like a pulled-out
label, with the label's lift shadow in Light. The keyboard is the setup keyboard's grid (spine keys, the selected
one paper), shorter (66 px keys) and without Shift and symbols: Space, Delete and Results on the last row. With
nothing typed the list is "Recent searches" (Dust 28) over plain 60 px spines, or two lines saying what can be
typed; nothing found is said in words ("No game matches “xyz”", Title 37, and a Dust line), not as an empty list.

**The letter of a jump** (L2/R2) is a 260 px square with 44 px corners in the middle of the screen, the inverse
of the page (text colour, `pal().text`, at 94 %; the letter in the page colour, Title 200), like the selected
row of the Minimal look: it reads over spines, labels and cases alike. It stays 700 ms after the last jump and
fades over the last 220 ms; nothing else moves.

## Appearance: dark and light (`src/palette.rs`, `src/appearance.rs`)
Settings → Look → Appearance (Y on the look page): Dark (default; the AMOLED's black pixels are off, so it is the battery-friendly one),
Light, or Automatic. Light is warm paper, not white: the same restraint, the polarity turned around. The
pulled-out label stays the brightest sheet; on the paper page it lifts off with a 6 px soft shadow (the label
alone would be only 1.1:1 against the page), and pickers get a paper sheet with a hairline edge.

| Token | Dark | Light | Use |
|---|---|---|---|
| void | `#000000` | `#F3EFE7` | page |
| spine | `#1F1F22` | `#E6E1D6` | unselected spine, row, tile |
| label | `#ECE9E1` | `#FFFDF8` | pulled-out label, selected row |
| ink | `#151515` | `#1A1916` | text on the label |
| chalk | `#D9D6CE` | `#2B2925` | text on the page |
| dust | `#8A877F` | `#625E56` | secondary text, hints |
| label_dust | `#5A5852` | `#5F5B53` | secondary text on the label |
| keycap | `#2A2A2E` | `#DCD6CA` | hint key caps |
| alert / on_alert | `#E0705C` / `#151515` | `#A63A28` / `#FFFDF8` | errors; text on an alert band (a row asking to confirm) |
| text / quiet / soft | `#FFFFFF` / `#AAA79F` / `#D2CEC6` | `#1F1D1A` / `#57534C` / `#3C3A35` | Settings-style lists, pills |
| pill / chip | `#272523` / `#938E85` | `#E3DED3` / `#5E5A53` | status and hint pills, their key chips, scroll bars, panels |
| band / select | `#77736B` / `#ECE9E1` | `#CFC8BA` / `#FFFDF8` | selected row of the Settings-style lists (select is the label's paper) |
| amber / star | `#D39A2C` / `#F2C14E` | `#8F5E0C` / `#B07A12` | Favourites, "Playing? Press Y", stars |
| split_bg / split_dim | `#12161B` / `#AAB3BD` | `#ECE7DD` / `#58544D` | Split look |

System stripes, cover art and console pictures are the same in both (the pictures' dark outlines read well on
paper; checked for every console on page, spine and label). Washes that went towards black (the Cards
background, Split's panel, the switcher's glow, dimmed neighbours, the game card's scroll fade and the scrim
under pickers) go towards the page colour instead. `palette::tests` require WCAG AA (4.5:1) for every text/background
pair the palette is used for, in both appearances and without exceptions (a new token for text, or for something text
sits on, goes into that list). Dark had four pairs below it until 2026-10-24: Dust on a spine and on Split's page (3.5 and
3.9:1: the count at a spine's end, Curator's rows, Split's top line), the white value on the Settings band (3.3:1) and the key letters on
the hint chips (3.8:1). Dust went from `#77746D` to `#8A877F` (the Pixel Path strip's grey; 4.6:1 on a spine, 5.9:1
on black), the band from `#8A8F96` to `#71757B` (4.6:1) and the chip from `#7F7F7F` to `#8F8F8F` (4.7:1); the
rest of Dark is as it was. Light needed nothing but `on_alert` (ink on its red was 2.7:1). On 2026-10-29 Dark's
neutrals turned warm at the same contrast (see [Design language](#design-language)): the band `#77736B` (4.7:1 under
white), the chip `#938E85` (4.7:1 under the pill's letters), the pill `#272523`, its text `#D2CEC6`, and the
selected name's pill is the label's paper `#ECE9E1` instead of a bluish white.

**Which surfaces follow the appearance:** every full screen of pocket-ui: the five looks (Cabinet turns from
walnut on black to light oak on paper, same grain and shading; its spines, cases and brass name cards stay),
game lists, the search and the letter of a jump, the game card and its pickers, the switcher (also over a game: it covers the screen), Settings and its pages (Battery,
Updates, About, Store, Sync, Manual), quick settings and Time, the first start. **Which stay dark in both:**
what is drawn over the game or over another screen: the pause menu and its pages (`palette::scoped(0, …)`),
the power menu panel, the volume/brightness OSD, notices and toasts, the "Saving…" message pill, and everything
pocket-core draws (it never sets an appearance). The switcher's screen bezel stays dark too: it is the
handheld's own screen.

**Colours** (Settings → Look → Colours, `palette::Colours`): the table above is **Warm**, the default.
Four more sets give every token a dark and a light value of their own, named after a feeling, not a product: **Indigo
Cube** (indigo page and surfaces, lavender paper, a lilac accent), **Grey Brick** (a grey handheld's plastic, its dark
green for the chips, the band and the accent), **Green X** (green on near-black: the label itself bright green with
dark ink; on paper, white with deep green), **Blue Station** (cool grey plastic with a deep blue). A set tints the
selection (label, band, select), frames and key caps (keycap, chip, pill), accents (amber, star) and the page and its
surfaces, and in the Cabinet the wood (stained towards the set's colour with its grain and shading kept), the name
plates (holder, card, ink) and the slot's light; covers, case art, console pictures and system stripes keep their own.
Each set passes the same 4.5:1 test in both appearances, its plates too, and has a page colour of its own. The
overlays over a game stay Warm's dark.

**Automatic** is light from 07:00 to 20:00 local time (`localtime_r`, so the zone from the first start), fixed
times rather than a sunrise guess from a zone name. It looks at the clock every 30 s and switches only after
8 s without input (or at once, unseen, while a game hides the menus). The change crossfades over 600 ms by
painting the frame in both palettes and mixing the pictures. `POCKETUI_CLOCK_SHIFT=<seconds>` moves the clock
it reads, to try it on the virtual device.
## Cabinet look (`src/cabinet.rs`, `src/cabinet_art.rs`, `src/media.rs`, `src/app_cabinet.rs`)
The physical idea taken literally: a dark walnut cabinet, one shelf per system (the recent games on top,
then Favourites), every game standing in its real case.

### Media (`src/media.rs`, `assets/media`)
**Shelf and Cabinet always show cases** (0.1.4, the owner's rule): never a bare cartridge. Twenty-one kinds of case, one
pixel-art template each (spine and front, `assets/media/<kind>-spine.png`, `-front.png`), shared by the Cabinet and the
Shelf look (`Medium::of`):
- **Own cases for the popular systems:** the cardboard boxes of the Super Nintendo / Super Famicom, the Nintendo 64 (both
  lie wide and stand on their short side on the shelf, as collectors keep them: the spine is 184 mm long), the NES /
  Famicom (standing), the Game Boy / Color and the Game Boy Advance; the Mega Drive's clamshell, the thin PlayStation
  jewel case, the light Dreamcast case (also Saturn and Mega-CD), the tall black PS2 case, the GameCube's, the white Wii
  case, the PSP's UMD case, the DS case, the original Xbox's black case with its green band. Kept because they already
  look right: the Master System's and Game Gear's case with the white grid, the 3DS case, the Vita case with its blue
  band; a plain DVD case stays for Curator's boxes.
- **One generic pocket-ui case for every other system** (`Medium::is_generic`, the owner, 2026-10-29): warm dark plastic, the
  cover in a clear sleeve, a band at the top of the front in the system's colour with its console picture and short name
  (`media::system_band`; the band alone on the spine). One size per kind of medium: a cartridge system's
  (`GenericCart`, 130 × 180 mm, a generic cartridge inside), a disc system's (`GenericDisc`, DVD size, a disc inside),
  and one for what comes on no medium (`GenericDigital`, 105 × 136 mm, a memory card inside: Ports, PICO-8, TIC-80,
  ScummVM, OpenBOR, WiiWare, PSP minis…, `media::is_digital`).
- **The plastic cases from photos** (0.1.4, `assets/media/case/SOURCES.md`): the PS1 jewel case, the PS2, GameCube,
  Wii, Xbox, DS, 3DS and PSP cases, the Dreamcast / Saturn case and pocket-ui's own generic cases are PixelLab conversions
  of public-domain / CC0 photos (a CD jewel case, a DVD case, Evan Amos's Wii case; Commons has none of a GameCube,
  Xbox, PSP, DS or 3DS case, so those are the DVD and Wii case fitted to their own size and plastic), at twice the
  template size (`Template::res`), the cover's and title's places one flat grey (`case/<kind>.json`). A cover of
  another shape (a DS cover, a European Saturn cover) is fitted whole, never cut. The cardboard boxes, the Mega Drive's
  and the Master System's cases are still drawn (`draw_media.py`). The table is printed by
  `assets/media/case/tools/table.py`.
- **What is inside** (`media::Game`, `assets/media/game/<kind>.png`): every cartridge and game card is made from a
  photo of the real thing (Evan Amos and other public-domain / CC0 photos on Wikimedia Commons, listed with their URLs
  in `assets/media/game/SOURCES.md`), logos and lettering painted out, converted with PixelLab `image-to-pixelart`
  (2–3 variants each, the closest to the photo kept; the owner's decision of 2026-10-29: drawn cartridges did not read
  like the originals): the Super Famicom / PAL and the US SNES cartridge (`Game::of` picks the US one for a game whose
  file says USA or Canada), N64, NES, Game Boy, Game Boy Color, GBA, the DS card, Mega Drive, Master System, Game Gear,
  the PC Engine HuCard, and a neutral cartridge for every other cartridge system (grey, taking the system's plastic).
  No public-domain photo exists of a 3DS card, a Neo Geo Pocket, WonderSwan or Lynx cartridge: those use the DS card and
  the neutral cartridge. The label area of each picture is one flat grey (its bounding rectangle in `<kind>.json`; only
  that grey is printed on, so a label's rounded corners stay plastic); the cover is
  put there at run time edge to edge without the box's printed side strip (`media::trim_system_band`: the Game Boy's and GBA's at the left, the US SNES box's black band and the US N64 box's red strip at the right, the Genesis strip at the left or top; cut at its edge, or at its usual width where the strip runs into dark art) or the even margin round it (`trim_bands`); a GBA label, much wider than its box art, shows the art's top where the title is (`label_part`), or the title written to fit
  (`label_title`: smaller, up to three lines, without the subtitle, only then "…"). Discs and UMDs are printed (below);
  the memory card of the digital systems is still drawn. The table in `src/media.rs` is printed by
  `assets/media/game/tools/table.py`.
- **Printed discs** (0.1.5, `media::DiscKind`, `media::disc_pic`, `assets/media/disc/<kind>.png`): a disc or a UMD shows
  its label side printed like a real one. Each system has its own disc template, a PixelLab `image-to-pixelart`
  conversion of a public-domain photo of a disc (`assets/media/disc/SOURCES.md`; the libretro collection has no disc
  art, ScreenScraper needs an account), every logo and the publisher's print painted out, its hub and rim kept: the
  PlayStation, PS2, GameCube (the 8 cm mini disc, `Game::MiniDisc`), Wii, Dreamcast, Saturn, Xbox, a plain CD for every
  other disc system, and the UMD's shell. Its print is one flat magenta (`#FF00FF`) that the code finds. The cover's art
  fills it (centred, cut round by the disc, without the cover's printed system strip: `DiscKind::band_zones`, the
  PlayStation's at the left, the band at the top of a PS2, GameCube, Wii, Xbox and PSP cover), and a clean ring at the
  rim carries the title along its lower arc, upright (`ring_title`; only from 12 px type, smaller the ring stays plain).
  The ring is the system's own where it has one: the PlayStation's black disc showing past its label, the PS2's blue, the Dreamcast's
  silver GD-ROM area with its line, the Wii's white disc, the dark rims of Saturn and Xbox labels; the others take the
  cover's colour darkened. No cover: the print in the cover's colour with the title above the hole (a UMD: on its
  label), none where it would be under 14 px. The disc is composed at the size it is shown, not scaled up from the template's pixels: a disc's hub rings and rim come from the template's colours by distance from its middle (`DiscTemplate::profile`), a UMD's shell is sampled between its pixels, and the rim, the hole, the print's edges and the ring are anti-aliased circles, so it is as smooth as the case beside it; the launch scales it smoothly (`Pic::scaled_smooth`). It is made once per case and size (`App::box_game`, warmed with the
  box; the launch's first frame) from the cover already on the case's front, so nothing is decoded on the UI thread;
  the templates are decoded on the cabinet's reader thread (`media::warm`). The launch scales the made disc on its way
  to the drive, as a cartridge's picture; over the drive a GameCube disc and a UMD keep their size to a 12 cm one.
- **Real sizes, one scale** (the owner, 2026-10-29): every case is drawn from the real thing's millimetres at 0.63 template
  pixels a millimetre (a DVD case's 190 mm is 120 px; `Medium::mm`, `Game::mm`), so a GBA box stands lower than a Game
  Boy box, a jewel case is wider than tall, a PS2 case tall. What is inside keeps its real size to its case
  (`Game::size_beside`): a SNES cartridge nearly fills its box, a DS card is small in its case, never under 48 screen
  pixels high (`media::GAME_MIN`) so its label still shows on 640 × 480. Spines alone are thicker than life, so a title
  fits. The case out of the shelf is shown at one scale for all, the one the largest case needs to fit the room
  (`show_k`, `show_scale`), never under 0.6 of a case's own (`SHOW_LEAST`): a GBA box comes out smaller than a PS2 case.

| Case (W × H mm) | template px | Inside (W × H mm) |
|---|---|---|
| SNES box 184 × 133, N64 box 184 × 130 (spines 184 long) | 116 × 84 / 82 | Super Famicom / PAL cartridge 130 × 86, US SNES 130 × 84, N64 cartridge 116 × 75 |
| NES box 133 × 184 | 84 × 116 | NES cartridge 120 × 133 |
| Game Boy box 120 × 128, GBA box 118 × 118 | 76 × 81, 74 × 74 | GB cartridge 57 × 65, GBA cartridge 57 × 35 |
| Mega Drive / Master System case 135 × 190 | 85 × 120 | MD cartridge 112 × 84, Master System 108 × 69, Game Gear 68 × 68, HuCard 54 × 85, neutral cartridge 80 × 100 |
| PS1 jewel case 142 × 125, Dreamcast case 142 × 127 | 90 × 79 / 80 | disc Ø 120 |
| PS2 / Wii / Xbox / generic disc case 135 × 190, GameCube 135 × 184 | 85 × 120 / 116 | disc Ø 120, GameCube disc Ø 80 |
| PSP UMD case 104 × 175 | 66 × 110 | UMD 64 × 65 |
| DS / 3DS case 125 × 137, Vita / generic digital case 105 × 136 | 79 × 86, 66 × 86 | game card 35 × 33, memory card 24 × 32 |
| generic cartridge box 130 × 180 | 82 × 113 | neutral cartridge 80 × 100 |
- **Drawn, not generated.** The templates come from `assets/media/tools/draw_media.py` (plain shapes, no logo or
  lettering; it prints the table of label rectangles that `media.rs` keeps). PixelLab was tried for them first (bitforge
  and pixflux with the drawn shape as init image, the console pictures as style): it either returned the drawing
  unchanged or invented consoles, so the media are drawn by the script and only the props are PixelLab's.
- **One template pixel is 2.5 screen pixels** on the Cabinet's shelf (`media::SHELF_SCALE`; it was two until
  2026-10-28, when a cabinet of a few games looked empty), the Shelf look's own scale on its shelf; scaled by nearest
  pixel, so they stay pixel art.
  A cardboard box's front is its printed cover edge to edge: one pixel of card round it (`card_edge`), the tuck flap's
  fold and rubbed corners over the cover (`box_wear`). Its spine is its system's printed spine, the same on every box of
  the system (`spine_print`: the US Super Nintendo's grey with a dark band, the NES's black box, the Game Boy's and the
  Game Boy Advance's white with their band); a US Nintendo 64 box's spine is its own art, so it is printed in its cover's
  colour (taken without the cover's red strip) with the red block and its white logo square at the top (`n64_badge`,
  no logo; the owner, 2026-10-30, after the real boxes); the
  cartridges inside take their system's plastic (`cabinet_art::plastic`: Game Boy grey, GBA indigo-grey, N64
  charcoal…); cases are drawn in their own colours. Every template pixel is solid or empty (the decoder reads
  half-transparent as empty).
- **Sizes** are the real ones (above; in the Cabinet: GBA box 185 px high, PS1 198, Game Boy 203, DS 215, UMD 275,
  NES / SNES / N64 290, GameCube 290, Mega Drive, PS2 and Wii 300); thicknesses are exaggerated so a title fits (33 px for
  the thin jewel case, 60 for a SNES box). Each case stands at its own size, all on the board (one baseline); a shelf
  is as tall as its tallest case.
- **Spines** (`media::spine`): the template, the label in the cover's main colour with a thin printed edge, the title
  read top to bottom in ink or chalk by the colour's brightness, as large as the label takes up to 37 px (28 on a Game
  Boy Advance box, 24 on the thin jewel case; it was 21 to 24, too small at arm's length). A long title is never cut
  while there is another way (`media::spine_fit`, trailer frames of 2026-10-29: "Advance…", "Donkey Kon…"): a little
  smaller (seven tenths of its size); on two lines across a thick spine, a subtitle on a line of its own without its
  dash ("Prince of Persia" over "The Sands of Time"); smaller still, down to 21 px; the same without the subtitle after
  " - " or ": "; in a font wider than Barlow Condensed (Theme → Font: Pixel, Rounded) one or two lines at 18 or 16 px, then the title in Barlow Condensed by the same rules; only then
  with "…" (tests/spine_titles.rs: long official titles at 640 × 480 and 1280 × 720 in all three fonts). Cases with a
  band at the top (Mega Drive, the light Sega case, the generic ones) paint it in the cover's **accent**
  (`media::cover_colors`: a clear colour of the cover far from the main one, at least 1.5 % of it; the main one again when
  there is none). Without a cover: the system's stripe, darkened. The colours are read on the `cabinet-art` thread
  and kept for the session; no frame decodes a cover.
- **Fronts** (`media::front`): the template with the real cover in its label, whole and undistorted
  (`cabinet_art::cover_fit`, on the thread): a cover of another shape than the label (a near-square Game Boy Advance
  box on its wide cartridge label) is scaled down until it fits, the rest of the label in the cover's edge colour a
  little darker, like a printed margin. A sheen of clear plastic over cases; without a cover a title card with the
  console and the name in two lines at most (a smaller size first, then "…").

### Props (`media::Prop`, `assets/media/props`)
A potted plant, a desk lamp, a small cactus and a bookend (PixelLab pixflux, seeds and prompts in
`assets/media/props/pixellab-assets.json`). Every shelf with room beside its games gets the bookend right after the
last case and a plant, lamp or cactus further along (`Prop::beside`; which one follows from the shelf, its system's
name, so it is the same every time), at least 60 px from the games, so a shelf of one game is not a bare board. At most four are painted per screen, only on a shelf that doesn't scroll, never taller
than the shelf's room. Never over a game. The Shelf look uses the same props at its own scale.

### The room: light and decorations (`room_light`, `room_deco`, `app_room_extras.rs`)
Two rows of Settings → Look → Cozy extras, both on from the start, Shelf look and Cabinet only.
- **Light by time of day** (`room_light::at`): the real clock lights the room. Morning (from 07:30) a little cooler and
  brighter, afternoon (13:00–16:30) as made, evening (18:30–21:00) warmer with the lamp glowing, night (22:30–05:00)
  dimmer with the lamp as the main light; between them it eases over one to two hours. It falls on the wood (the
  cabinet's walls, boards and plinth, the Shelf look's board): channel gains between 0.82 and 1.05, half of that on the
  Light appearance. Cases and props get half its dimming and no tint (`Light::case_shade`, at least 230/256); the
  showcase's cover, titles, the name cards and hints none. The lamp's glow is a soft warm ellipse in eight steps round
  its shade, inside its compartment. Cost: the wood is tinted once when it is made and kept in its cache; the light
  changes in steps of under 1 % a channel every few minutes, and only a change makes the wood again
  (`CabinetUi::room_changed`). The app looks once a minute while a Shelf or the Cabinet is on screen
  (`room_tick`); with Motion off it doesn't wake by itself. `POCKETUI_ROOM_TIME=HH:MM` sets the clock (snapshots).
- **Decorations that grow with play** (`room_deco::earned`, from `library.json`: each game's time, status and case
  stickers; no Play history needed): the plant grows from a sprout (under 2 hours in all) to a small plant (2 h), the
  plant as it was (15 h) and a big one with flowers (60 h); a trophy after the first completed game (the status or the
  "Completed" sticker); a framed "100 %" after the first "100 %" sticker (the words painted on the frame's empty paper,
  `media::FRAME_PAPER`); a figurine (robot, knight, dragon, cat, rocket, owl; `Figure::of` the system's name) on a
  system's own shelf after 10 hours on it. They stand in a row after the bookend, before the plant, lamp or cactus,
  each only where it fits whole (`room_deco::place`): never over a case, never cut at the shelf's end, never taller
  than the room. The Shelf look shows the trophy and the frame on every row with room; the Cabinet on the first shelf
  from the top with room for them (`awards_shelf`). Off: the shelves as they always were (the grown plant). Art:
  PixelLab pixflux in the props' style, warm, no characters of any game, no logos (`pixellab-assets.json`).

- **The showcase** (`showcase`, a photo of 2026-10-28: the case held up in front of the shelf above hid the
  spines of two shelves, the bottom row was cut by the hints, the cover was cropped): the cabinet's top board sits
  right over the selected shelf, and the room between the title and that board is the showcase, as high as the case
  out of the shelf needs. Under the selected shelf come the next ones as far as they fit whole, then the plinth; no
  shelf is cut by the hints (only one taller than the whole room is, fading out). The board glides with the cabinet
  when the room changes (`show_top`).
- **Selecting** pulls the case out and turns it to show its cover (240 ms; the last one goes back in 160 ms
  from wherever it had got to, so holding a direction never waits). Out, it stands in the showcase on the top
  board, above its own slot as far as the screen lets it: nothing of the shelves is under it. It is never taller
  than the showcase (`show_scale`). Simple perspective: each face is a run of columns whose height
  follows its distance; at rest the front is copied as it is, sharp.
- **Starting** a game (Settings → Sound & feel → Animations, Cabinet and Shelf looks; `app_cabinet::launch_animation`). The
  numbers below are Quick; Cozy plays the same shape over `cabinet::INSERT_COZY` (1330 ms, in at `IN_AT_COZY` 800 ms; sounds:
  the case opening at A, `ui_sound::going_in` seated at 800 ms, `Sound::CrtOn` from pocket-core with the old-TV power-on), slows every timed animation by `motion::COZY_SLOWER` (1.4) and holds a pocket-core game's window
  until the fade has run (`startup::HOLD_UNTIL`; pocket-core draws nothing and is silent meanwhile, a button ends it):
  the game is started at A, and while it loads the console's pixel-art picture comes up, the case opens and the game
  slides out of it (the cartridge, card or disc, at its real size beside the case), the case fades and the game goes
  into the console, the same way for every system (0.1.4; the cartridge is made in the first frame of the animation,
  after the game's process has started, from the cover on the case's front); in at 180 ms (`cabinet::IN_AT`) with a click (`Sound::Insert`, a
  short latch click, 6 dB under the other sounds, queued at A so that it lands at 170 ms, `ui_sound::play_after`) and a glow in the label's colour, done at
  300 ms (`cabinet::INSERT`). The launch fade waits for the click. Nothing waits for the animation: a game whose
  window comes up first covers it, and any button skips it. Until 2026-10-28 the game was started only when the
  cartridge reached the slot (260 ms after A), which added that to every start in these looks. Cartridges stay
  half out of the console, as they do.
- **The game box** (X on a game; SELECT switches to the page and back, `game_box.rs`): the card as the game's box in its case's style, front and
  back, turned over with Left/Right in 280 ms (Motion Off: at once) by narrowing one face to its edge and widening
  the other. The front is the case as it stands on the shelf, opened: its game slides out of it to its right in
  300 ms (`game_box::OPEN`, Motion Off at once) and comes all the way out beside it, a little apart, at its real size, so its label shows whole
  (`case_height` makes the case smaller where both would not fit across the screen, a SNES box on 640 × 480). The
  frames of the backs (and, while the case's front is not made yet, the cardboard box's front) are PixelLab pixel art
  (`assets/media/back`, `tools/` there), nine-slice at a whole number of screen pixels a template pixel; the back's
  paper is the cover's colour, deep, with white type; missing pictures and text get their own plain places.
- **Playful animations** (Settings → Sound & feel → Playful cases, `src/playful.rs`; on with Cozy, off with Quick until the
  player switches it, nothing with Motion Reduced or Off), after Socket's playful feel in our 2D pixel style. Every one is
  a transform of a picture already made (an offset, a row shear, a nearest-pixel scale, the box disc's turn), nothing
  is made or decoded per frame:
  - *Pulled out*, a case springs a touch past its place and back (`playful::back_out`, about 5 %): the Cabinet's case
    into the showcase, the Shelf's spine up out of the row. Out and turned, it rocks once on its bottom edge
    (`playful::wiggle`, a row shear of at most 2 % of its height, 420 ms).
  - *Going in*, each medium its own way (`playful::Loader`), the medium in at `IN_AT` as before so its sound
    (`ui_sound::going_in`) still lands on it: a cartridge is pushed into its slot and seats flatter and wider for a
    moment with the click, then springs back out a hair (`playful::seat`); a game card clicks all the way in and its
    edge springs back out for a moment; a tray (PS2, Xbox and every disc console not named here) slides out under the
    drive's slot, the disc comes over it, drops in lying down and rides in as the tray shuts with the disc-in sound's
    clunk (126 ms after it is in, `playful::SHUT_AFTER`); a lid (GameCube, PlayStation, Saturn, Dreamcast, Sega CD,
    PC Engine CD, Neo Geo CD) flips up behind the disc, the disc drops in over the console's top edge and the lid
    shuts with the same clunk; a slot-loader (Wii, Wii U, PS3) and the PSP's UMD take it in as before.
  - *In the open box* a disc (not a UMD, in its shell) turns slowly, one turn in 9 s, for 12 s after the box opens or
    turns back to its front, slowing to a stop over the last 2 s (`playful::spin`): no frames after that. A frame per
    refresh, or every other one where the box costs more than half a refresh (the H700, `BoxView::spin_wake`).
  - *Scrolling fast* on the Shelf, the spines lean against the move, at most about 2 degrees, and swing back when it
    stops (`playful::Lean`: a spring pulled by the row's speed, so a single step barely tips them). Each row of the
    strip is copied a little sideways.
- Names of the shelves are on small paper cards in brass holders on the board edges, Body 20 px; a sticker's
  shelf has the sticker itself there instead (round, in the stickers' colour).
- **Curator** (between Favourites and the systems): a shelf of DVD-size boxes in Curator's colour for the Lens, the
  smart shelves and the stickers, the list opened from it as a shelf of its own under it, and the pinned smart
  shelves and stickers as shelves (CURATOR.md, "Curator 2 as built"). The Surprise me box has the day's pick's box art
  on its front, its spine stays Curator's (CURATOR.md, "Curator on Home").
- **Up next** after a game is the same paper label over the bottom of the screen in every look, the cabinet
  included.
- Everything around stays as in the other looks: black around the cabinet, the shelf's name and the battery
  in Dust, the game's title in Title 50 Chalk with its meta line under it, the hint pills at the bottom.

### Case stickers (`src/case_stickers.rs`)
The collector's marks on a case (0.1.4): **Completed** (a green round sticker with a white tick), **100 %** (a gold
seal with a scalloped edge), **Favourite** (an amber star), **Playing now** (an orange-red round one with a pad) and
**Game of the year** (a gold banner with a laurel wreath and the player's year). Printed stickers with a white die-cut
edge, PixelLab pixel art in our style (`assets/media/stickers`, prompts and seeds in `pixellab-assets.json`; the Game of
the year banner's frame drawn around a generated wreath, the two generated banners were unusable); no lettering in the
pictures: "100 %", "Game of the year" and the year are Barlow (Title for the numbers, Body for the words), in a dark
gold ink, centred on the sticker's plain print, as large as it takes. Too small for the words (a case out of a 640 x 480
shelf), Game of the year shows only its year.

- **On the front** (the case out of the shelf, the Shelf look's front, the game box): a round sticker is 15 % of the
  front's width (a 20 mm sticker on a 135 mm case), scaled by nearest pixel; the round ones stand in a row from the
  cover's top right corner leftwards, a second row under it when the cover is narrow; Game of the year at the cover's
  bottom left. Each is turned 3-9 degrees, alternately left and right, and stays a little off the line, the same for a
  game every time (from its file), with a one-pixel shadow under its lower edge. Never off the cover.
- **On the spine**: one small sticker in the spine's top above the title (where its band is), without words, as large
  as the spine's width lets it: the first of Game of the year (as the gold seal), 100 %, Completed, Playing now,
  Favourite. The title keeps all its room.
- The cartridge, card or disc inside has none: they are on the case (`app_case_stickers::bare_front`).
- Set on the card's Stickers row (its sheet's first five rows, before the player's own Curator stickers; Left/Right
  on Game of the year change its year, from the game's release year or this year).
  The sheet's row window fits the available height and text size, including its title and padding above the hints;
  more rows scroll through it (at most eight visible).

### The TV on the cabinet (`src/cabinet_tv.rs`)
A small 1990s portable CRT in warm beige stands on the right end of the top board, in the showcase, and shows the
selected game (`cabinet_tv` module doc). PixelLab pixel art generated from text (Wikimedia Commons had no clean
public-domain photo of a 90s portable; `assets/media/tv/SOURCES.md`), its glass one flat magenta that the code finds.

- **Size**: a fifth of the screen's height, a whole number of screen pixels a template pixel from two on (three on the
  Mini V2: 243 x 213 px), at most a quarter of the screen's width and the showcase's height; left out when its glass
  would be under 60 px high. It is not sized by the case, so it stays put as the selection moves between shelves.
- **The picture**: the downloaded screenshot, else the resume state's preview or the frame kept at Quit, else the
  cover, else the console's picture on the dark tube; cut to fill the glass from its middle. The CRT look: the corners
  pulled in a little (a curved tube), every second line darker (scanlines at the picture's scale), darker towards the
  rim, a little warmer, and a soft glow from each pixel's neighbours. Never animated: no flicker, no noise.
- **Changing**: the next game's picture crossfades in over 220 ms once it is made (Motion Off: at once); until then
  the last one stays. The case out of the shelf keeps left of the TV.
- **Freshness**: picture downloads/library changes and returning from a game discard the TV loader's cache,
  including its pending replies. Jobs and replies carry a generation; invalidation keeps the same single decoder
  thread, and an old reply cannot clear a new request for the same picture. The current picture stays until its
  replacement arrives, even when a resume preview
  or screenshot was replaced at the same path.
- **Waiting**: the common event loop drains completed TV jobs even while Settings, a card or another look hides
  the Cabinet. A hidden TV stops requesting frames once its job ends; returning to the Cabinet uses the ready screen.

## Shelf look (`app_cabinet.rs`, "the Shelf look")
The default look; its layout is under [Layout](#layout-12401080-left-aligned-72-px-margin).
- **Cost.** A spine per entry and scale (`stand`), made the first time it is in view and kept (about 0.2 ms each);
  the shown front per entry and size (`shelf_shown`), made once its cover is read on the `cabinet-art` thread; the
  board and the props once per size. A frame only copies pictures. The row's places are worked out again only when
  the list changes. Covers' colours come from the same thread as the Cabinet's and are kept for the session.
- **Launch:** the shown front stays lit while the rest of the shelf fades to black.
- **Touch:** a tap on a spine selects it, a tap on the selected one (or its front) starts it; a sideways swipe moves
  along the shelf a spine at a time.

## Self-review against defaults
- A black background with one acid accent is a known default. Here black is required by the AMOLED panel,
  and there is **no** single accent; colour comes only from per-system stripes and the paper label.
- Cream is used only as the label sticker, never as the page (in Light the page is a warmer, darker paper and
  the label stays the brightest sheet on it).
- No all-caps eyebrows, no middle-dot meta strings on the shelf (the label meta uses one, since it reads as
  printed label text; revisit), no numbered markers, no mono data labels.


### SM8550 power settings

On experimental Pocket 6 and Nova, Sleep & power uses the existing row pattern with three rows:
Screen off after, Switch off after, and fixed Screen off short press. A muted line explains why real sleep
is unavailable. The normal five-row page stays unchanged on SM8250. The 1280×960 and 1920×1080 layouts
reuse the earlier device work; no shared page dimensions or Mini V2 geometry change.
