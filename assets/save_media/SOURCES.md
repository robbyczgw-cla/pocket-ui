# Save media pictures (`src/save_media.rs`)

The memory cards the pause menu's Save and Load show beside their blocks. Made on 2026-10-29 with
[PixelLab](https://www.pixellab.ai) (API v2), cut out and finished with `assets/systems/tools/` (`prep.py`, `cutout.py`,
`finalize.py`, `rim.py`), the same way as the console pictures in `assets/systems/` (PIXEL-STYLE notes there). No
logo, brand name or model lettering is on any of them.

## From public-domain photos (`image-to-pixelart`, output = input / 5, `text_guidance_scale` 8)

| File | Photo (Wikimedia Commons, public domain) | What was done before the conversion |
|---|---|---|
| `ps1.png` | [File:PSX-Memory-Card.jpg](https://commons.wikimedia.org/wiki/File:PSX-Memory-Card.jpg) by Evan Amos | every logo and the lettering painted over with the plastic around it; after: the colour set back to the photo's grey (the conversion had turned it blue) |
| `ps2.png` | [File:PS2-8MB-Mem-Card.jpg](https://commons.wikimedia.org/wiki/File:PS2-8MB-Mem-Card.jpg) by Evan Amos | logos and lettering painted over, the photo brightened (levels 0–55 %) so the black plastic keeps its edges; after: white balance to the photo, a light rim (`rim.py`) |
| `vmu.png` | [File:Sega-Dreamcast-VMU.jpg](https://commons.wikimedia.org/wiki/File:Sega-Dreamcast-VMU.jpg) by Evan Amos | the maker's logo painted over (the buttons' small labels stay); after: white balance to the photo |

Licence read from the Commons API (`extmetadata`: LicenseShortName "Public domain", Artist "Evan-Amos") on 2026-10-29.

## Generated from a description (`create-image-pixflux`, no photo in)

No public-domain photo of these exists on Commons. Each was made from text alone, three variants or more, and the one
closest to a reference picture (used only to compare, never as input) was kept. They are in the style of the others
and close to, not exact pictures of, the hardware.

| File | Seed | Size | Description |
|---|---|---|---|
| `gamecube.png` | 47 | 128 x 128 | a Nintendo GameCube memory card 59, small light grey plastic memory card seen straight from the front, portrait, slightly taller than wide, a big recessed rectangular label area covering the upper two thirds with a raised rim around it, a tiny triangle arrow at the top edge, a lower band below the label, the bottom edge slightly wider with small grip notches on both sides, plain product, no text, no logo |
| `xbox.png` | 76 | 160 x 192, `view` side | an Xbox memory unit for the original Xbox controller, front view, a small dark grey plastic unit standing upright shaped like a rounded pill: a wide oval dome cap on top with three short grip ridges on each side, below it a slightly narrower rectangular body with a recessed square label, at the very bottom edge a small rectangular connector plug, matte, plain product, no text, no logo |
| `stick.png` | 81 | 128 x 192 | a Sony Memory Stick Duo: a slim dark stick shaped memory card, portrait, 20 millimetres wide and 31 millimetres tall, at the short top end a single row of ten small separated gold contact pins between thin black ribs, the top left corner cut off diagonally, a small notch in the left long side, matte black body with a slightly lighter label area below the pins, lying flat seen from directly above, plain product, no text, no logo |

All with `no_background`, "single color black outline", "medium shading", "medium detail", `text_guidance_scale` 8, `view`
"high top-down" unless said;
`xbox.png` and `stick.png` got the light rim (`rim.py`).
