# Interface pictures: sources and licences

The small pixel-art pictures of the interface (`src/ui_art.rs`): the vignettes above an empty page's words, the
handhelds on About, first start and the manual's Controls page, the Settings categories' icons and the Year in
review's stickers. None of them is traced from a photo or copies a product's artwork, and none carries a logo,
a brand name or lettering. Words that go with them (the labels of the Controls picture) are drawn by pocket-ui, so
they are translated like every other text.

## Generated from text prompts (PixelLab)

Made with [PixelLab](https://www.pixellab.ai) `create-image-pixflux` from a text prompt alone (no input image) on
2026-10-27: transparent background, `single color black outline`, `medium shading`, `medium detail`, view `side`,
text guidance 8. Every prompt ended with ", cozy, calm, warm muted colours, soft light, single object, centered,
front view, no text, no letters" (the vignettes, and the `emblem-*` pictures of 2026-10-29 on the fronts of the shelf's
boxes that are not a game) or ", die-cut vinyl sticker with a thick white border, cute, warm
muted colours, single object, centered, no text, no letters, no numbers" (the stickers). Every request, the ones not
used included, with its seed: `pixellab-assets.json`.

| File | Prompt (before the common ending) | Seed | Size asked |
|---|---|---|---|
| `empty-no-games.png` | an empty wooden shelf with a small green plant in a pot on top | 11 | 160x120 |
| `empty-no-favorites.png` | a small paper star resting on an empty soft velvet cushion | 21 | 160x120 |
| `empty-history.png` | an open blank diary notebook with a pencil lying on it | 31 | 160x120 |
| `empty-offline.png` | a small wooden radio with its antenna pushed down and a little moon and stars above it, sleeping | 44 | 160x120 |
| `empty-hotel.png` | a shiny brass service bell on a small wooden reception desk with an hourglass beside it | 53 | 160x120 |
| `empty-screenshots.png` | a retro instant camera with a blank photo lying in front of it | 63 | 160x120 |
| `empty-search.png` | a magnifying glass next to a small empty open cardboard box | 72 | 160x120 |
| `empty-store.png` | a small closed shop stall with an awning and empty shelves | 81 | 160x120 |
| `empty-nothing-played.png` | a retro handheld game console resting on a cushion with a small blanket, sleeping | 91 | 160x120 |
| `sticker-completed.png` | a small golden trophy cup with a star | 201 | 96x96 |
| `sticker-hundred-hours.png` | a golden hourglass with sparkles | 211 | 96x96 |
| `sticker-ten-games.png` | a small row of game cartridges standing on a shelf with a ribbon | 222 | 96x96 |
| `sticker-streak.png` | a small flame like a candle flame, warm orange | 231 | 96x96 |
| `sticker-came-back.png` | a small bird returning to its birdhouse | 242 | 96x96 |
| `emblem-curator.png` | a tidy little wooden crate holding a row of plain coloured game cartridges, a small brass label holder on its front | 304 | 160x120 |
| `emblem-surprise.png` | a small wrapped gift box with a ribbon bow | 311 | 160x120 |
| `emblem-streaming.png` | a small desktop computer monitor with a game controller resting in front of it | 321 | 160x120 |
| `emblem-sticker.png` | a few round colourful paper stickers on a peeled sticker sheet | 331 | 160x120 |

What was changed (`tools/finalize.py`): cut to the picture with a 1 px margin, transparency made on/off (solid from
half on); the stickers got the same edge each, 2 px of paper white and 1 px of dark outline, in place of the uneven
borders PixelLab drew. The handheld under the blanket is a generic one with blank buttons.

Licence: PixelLab's terms (read 2026-10-22, see `NOTICE.md`) give the user the copyright in what is created and let
it be used and distributed for any purpose. The pictures ship with pocket-ui under GPL-2.0-or-later.

## Drawn for this project

| File | What | Tool |
|---|---|---|
| `device-miniv2.png` | Retroid Pocket Mini V2, front | `tools/draw_devices.py` |
| `device-miniv1.png` | Retroid Pocket Mini (V1): the same shell, a 4:3 screen | the same |
| `device-rp5.png` | Retroid Pocket 5 | the same |
| `device-flip2.png` | Retroid Pocket Flip 2, opened | the same |
| `device-rp6.png` | Retroid Pocket 6 | the same |
| `device-nova.png` | Retroid Pocket Nova | the same |
| `anchors.json` | Where each control of the Mini V2 picture is, for the Controls page's labels (copied into `src/ui_art.rs`, a test keeps them equal) | the same |
| `icon-*.png` | The Settings categories: a half-dark circle (Look & feel), a game pad (Playing), a cartridge (Games), a Wi-Fi fan (Connections), a cog (System); 16 x 16, white on clear, painted in the row's colour | `tools/draw_icons.py` |

The handhelds are drawn in pixels from their published proportions (body size, screen size and aspect ratio, where
the D-pad, face buttons, sticks and small buttons sit), in the style of `assets/systems`: one dark outline pixel,
three or four tones a material, a light rim where the shell is dark. No photo was traced; the screens show
pocket-ui's own shelf; there are no logos and no letters on the buttons (the Controls page writes them on). Licence:
GPL-2.0-or-later, like pocket-ui. No PixelLab generations were spent on them.
