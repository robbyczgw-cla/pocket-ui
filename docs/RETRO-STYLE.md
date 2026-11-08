# Retro style (`src/retro.rs`)

Settings → Look → **Style** → **Retro**: every menu in the manner of a 90s/2000s handheld's. It is the fourth value of
the Style row (settings.json `"font": "retro"`, `font_style::FontStyle::Retro`), so the whole look is one switch; the
looks (Shelf, Cabinet, Easy, Kids, …), the box art, the cases and Continue stay as they are. Painters ask
`retro::on()`: one atomic load.

| Part | Clean styles | Retro |
|---|---|---|
| Colours | Settings → Look → Colours | Two palettes of its own, each dark and light: **Cocoa** (warm brown and cream, a tangerine selection; the default) and **Pea** (four LCD greens; the icons are mapped to them). While Retro is on, the Colours row steps through these two (settings.json `"retro_colours"`) and keeps the other choice |
| Type | Barlow, Pixel or Rounded | Pixelify Sans SemiBold for titles, Regular for body text at 0.9 of Barlow's capitals (`font_style::RETRO_BODY`: Pixelify runs 21 % wider), drawn on its own pixel grid: the size rounded to whole screen pixels per font pixel, no grey edges, letters whole font pixels apart (`text::raster`, `font_style::PIXELIFY_PIXEL`). Digits from a 5x7 LCD set at Pixelify's widths (`retro::LCD`), and B, C, S, Z likewise (`retro::lcd_letter`): Pixelify's read as 8, O, 8, 2 ("12:21" as "18:81"). The bundled Pixelify files themselves are patched for the letters the LCD set doesn't cover (`scripts/pixelify-glyphs.py`, OFL modification noted in `assets/fonts/OFL.txt`): an open small c (it read as o, "desoription"; a capital C is open too), a diagonal Z, a flat-topped 5, with advances unchanged; the unit tests `retro_letters_are_crisp_and_digits_tell_apart_from_letters` and `pixel_font_look_alikes_tell_apart_at_every_size` keep the look-alike pairs apart from 21 px to 240 px `POCKETUI_RETRO_DIGITS=pixelify\|plain` paints Pixelify's own or Barlow's digits instead, for comparing |
| Selection | Paper pill | Flat bar with notched corners, a light top and dark bottom pixel row and a pixel cursor (`retro::bar`); lists whose names start near the bar's end (Split, Minimal, sheets, the power menu) take it without the cursor (`plain_bar`, `select`) |
| Panels, sheets, hint bar, status corner, notices, the save preview, quick settings, the power menu, the switcher's titles, pocket-core's notes and OSD over a game | Rounded | Pixel windows: a shadow, a light outer line, an inner line, faint scanlines (`retro::frame`, `window`) |
| Pills, toasts, chips, progress bars, tiles | Rounded | Notched blocks (`retro::pill`) |
| Key caps | Round | Notched blocks with a darker bottom row (`retro::cap`) |
| Easy's and Kids' round buttons | Discs | Circles built of frame pixels (`retro::disc`) |
| Settings categories, pause menu rows | 16 px glyphs / text only | 32 px colour icons at 2x (`retro::icon`, `assets/ui-art/retro/`, made with PixelLab, prompts in its SOURCES.md) |
| Menu sounds | Sample files | Synthesised blips for Nav, Confirm, Back, Menu and Error, 1.5–3 dB under the files they replace, never above −1 dBFS (`ui_sound`, [HOST.md](HOST.md)) |

One frame pixel is `retro::U` = 4 screen pixels on the Mini V2's 1240x1080 and on the small screens painted at 1280x960
(2 real pixels at 640x480). Art keeps its own shapes: the Cabinet, the cases, box backs, screen frames, the controls
card's device, the switcher's bezel.

pocket-core reads the Style and the palette from settings.json once at a game's start (`settings::style`), so its notes
and the volume/brightness OSD over the game follow.

**Pause menu:** the rows' window is as wide as its longest name with its value at rest needs (Load: the longest of its
stops, so it keeps its width while stepping), from half the screen less 80 up to what leaves the preview window a third
of the screen; the preview's top lines up with it, the note under the picture stays within the window's width (two
lines at most, the second ending in "…"), and the game's name keeps what it needs beside the battery line, which takes
two lines. pocket-core's notes keep their padding inside the frame's inner line, with frame pixels of half `U` on
panels under 800 pixels across (`retro::frame_at`).

**Tests:** `retro_palettes_are_readable_wcag_aa_in_dark_and_light` (`palette.rs`: every text pair of both palettes at
4.5:1 in dark and light, among them `label_dust` on the selection bar, where a selected row's quieter part sits, such as
the pause menu's slot "‹ 3 ›"; the frame's line and the selection bar at 3:1 against the page), the text-size test's Retro runs
(`tests/text_size.rs`: every state, Large German, 1240x1080 and 640x480; and the pause menu, Save/Load, Options, quick
settings and pocket-core's notes in Large German and Spanish at 1240x1080, 640x480 and 1024x768 behind a long real
name, where the windows' lines count as text no string may touch, `retro::frame_lines_are_no_text`, and the longest
names must show whole; and Help & tools → Test buttons, every case in Large English, German and Spanish at the same three
sizes, where the last-press panel's lines count as text too and the hints before the first press must show whole),
`text.rs`'s `retro_letters_are_crisp_and_digits_tell_apart_from_letters`, `retro.rs`'s own
(icons decode, the switch and palette steps), `ui_sound`'s blip tests.

Snapshots: `POCKETUI_RETRO=cocoa|pea|off` overrides settings.json.
