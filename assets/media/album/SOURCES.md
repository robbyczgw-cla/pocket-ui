# The photo album's art (`album.rs`, `album_paint.rs`)

Every piece is a photo converted to pixel art with PixelLab's image-to-pixelart (v2), then cut with `tools/finish.py`.
The prepared photos, PixelLab's raw answers and the requests (`tools/pixellab-log.jsonl`, never the key) are in `tools/`.

| File | Source photo | Author | Licence | What was done |
|---|---|---|---|---|
| `polaroid.png` (84 x 104, the picture window see-through) | [Polaroid fd01.png](https://commons.wikimedia.org/wiki/File:Polaroid_fd01.png) | R. Hoyos (see the file page) | CC0 | turned 3° straight, cropped to the frame, the picture painted over in grey (`tools/frame_prep.png`), PixelLab seed 2, window cut out |
| `case-plastic.png` (224 x 154) | [Amaray-case.jpg](https://commons.wikimedia.org/wiki/File:Amaray-case.jpg) | Asim18 | Public domain | both inner halves of the open case taken out of their perspective and laid flat side by side, lighter (`tools/case_open_prep.png`), PixelLab seed 4, the last rows (the white under the case) cleaned |
| `case-cardboard.png` (224 x 154) | the box: [Flipper Zero Video Game Module opened retail box and manual.jpg](https://commons.wikimedia.org/wiki/File:Flipper_Zero_Video_Game_Module_opened_retail_box_and_manual.jpg); the lid's cardboard: [Cardboard-546867.jpg](https://commons.wikimedia.org/wiki/File:Cardboard-546867.jpg) | Siarhei V; Humusak (Pixabay) | **CC BY-SA 4.0**; CC0 | the open box's tray (its foam insert with the game's slot) taken out of its perspective, the foam lighter; the lid's inside (its print left out) from the cardboard photo in the box's colour, with its rim and hinge (`tools/cardboard_flipper_prep.png`); PixelLab seed 9, strength 800, modified via PixelLab. **The picture is CC BY-SA 4.0** (a derivative of Siarhei V's photo) |

The frame and the plastic case are free of conditions (CC0, public domain). The cardboard case is a derivative of a
**CC BY-SA 4.0** photo: it keeps the credit above and is shared under CC BY-SA 4.0 (NOTICE.md, "Interface pictures").
The first cardboard case (`tools/cardboard_prep.png`, `tools/px-cardboard-5.png`, from the CC0 photo alone) is kept for reference.
