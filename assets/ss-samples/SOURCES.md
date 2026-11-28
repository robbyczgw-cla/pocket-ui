# Sample box pictures: sources and licence

All four pictures come from one game: **Tobu Tobu Girl** (Game Boy, 2017, Tangram Games).
It had a real physical retail release: a limited edition Game Boy cartridge with a full colour
Japanese style cardboard box and a 12 page full colour manual, released 3 December 2017
(sources: http://tangramgames.dk/tobutobugirl/ and https://tangramgames.itch.io/tobutobugirl).

The pictures are made from the print files Tangram Games ships in its own free download on itch.io
(`tobutobugirl.zip`, "Game Boy rom (includes PDF manual, box art and sticker)"):
`boxart.pdf` (full box wrap, die-cut layout), `sticker.pdf` (cartridge label), `manual.pdf` (16 PDF pages).
These are the original 2017 Tangram release files. They are NOT the later "Deluxe" edition by
First Press Games (Tangram says on its itch.io page that it does not own the rights to that manual).

## Shared facts (all four images)

- **Game:** Tobu Tobu Girl (Game Boy), Tangram Games, 2017
- **Source URL:** https://tangramgames.itch.io/tobutobugirl (download "Game Boy rom (includes PDF manual, box art and sticker)", file `tobutobugirl.zip`, upload id 678889, downloaded 2026-11-27, sha256 `c5ec138071079b9cad227d6fb875ad4e58a77f173ed3cd8758c04c0ab166d6e7`)
- **Copyright holder:** Tangram Games (the box back prints "© 2017 Tangram Games")
- **Artist:** Lukas Erritsø Hansen (manual credits page: "Box and manual artwork: Lukas Erritsø Hansen")
- **Licence:** Creative Commons Attribution 4.0 International (CC BY 4.0), https://creativecommons.org/licenses/by/4.0/ ; full text in `CC-BY-4.0.txt` next to this file
- **Licence statements, quoted exactly:**
  - itch.io project page, "More information" table: "Asset license: Creative Commons Attribution v4.0 International" (and "Code license: MIT License"). This is the page whose only download contains the box, label and manual PDFs.
  - GitHub README, https://github.com/SimonLarsen/tobutobugirl : "All assets (images, text, sound and music) are licensed under the [Creative Commons Attribution 4.0 International License](http://creativecommons.org/licenses/by/4.0/)."
  - Game website, http://tangramgames.dk/tobutobugirl/ : "The full source code and assets for the game are available on GitHub under a MIT/CC-BY license"
- **Suggested attribution line (in app credits/licences screen):**
  "Tobu Tobu Girl box, label and manual art © 2017 Tangram Games (artwork by Lukas Erritsø Hansen), CC BY 4.0, https://creativecommons.org/licenses/by/4.0/ . Source: https://tangramgames.itch.io/tobutobugirl . Cropped and resized by pocket-ui."

## Per image

| File | What it is | Source file | Changes |
|---|---|---|---|
| `spine.jpg` (84x540, 7.0 KB) | Box spine (side panel between front and back; title runs top to bottom, Tangram logo at the foot) | `boxart.pdf` (sha256 `e5b20ad8...865939`) | Rendered at 300 dpi; cropped to the right-hand spine panel (x 1721..1939, y 536..1948 of 3508x2481), without fold flaps; resized to 540 px high; JPEG q80. Already vertical, no rotation needed. |
| `back.jpg` (428x540, 47 KB) | Whole box back (three screenshots, blurb, bullet list, Tangram logo, © line) | `boxart.pdf` | Rendered at 300 dpi; cropped to back panel (x 1945..3065, y 536..1948), without flaps; resized to 540 px high; JPEG q80. |
| `label.jpg` (540x475, 49 KB) | Cartridge label (sticker) artwork | `sticker.pdf` (sha256 `2720baa3...806722`) | Rendered at 600 dpi (993x874, whole sticker, no crop); resized to 540 px wide; JPEG q80. |
| `manual.jpg` (415x540, 39 KB) | Manual page 5 "Playing the game" (Bouncing / Dashing / Ouch!, with character art) | `manual.pdf` PDF page 7 (sha256 `c3036b8c...ef36f47`) | Rendered at 300 dpi; whole page, no crop; resized to 540 px high; JPEG q80. |

Rendered from the PDFs with pdftoppm and cut with ImageMagick at the coordinates above.
