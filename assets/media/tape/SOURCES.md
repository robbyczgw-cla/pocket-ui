# Masking tape: source and licence

`tape.png` (580 x 96, RGB) is the masking tape on the home-made label of a game without box art (`src/tape_label.rs`):
a strip of tape cut out of one photo, laid horizontal, its length along the picture's width and its width the
picture's height. `src/tape_label.rs` scales it to the tape's width, tears its two ends per game and writes the name on
it.

## Photo

| Photo | Author | Licence | Checked |
|---|---|---|---|
| "A roll of beige masking tape partially unwound, positioned against a light gray background." https://wordpress.org/photos/photo/38768c59e6/ (file https://pd.w.org/2025/09/38768c59e635f96c4.39660493-1536x2048.jpg, 1536 x 2048) | Manjil Aryal | CC0 1.0 (WordPress Photo Directory; read through the Openverse API on 2026-11-14) | 2026-11-14 |

No public-domain or CC0 photo on Wikimedia Commons shows a flat strip of masking tape (the CC0 ones show rolls, tape
under paint or duct tape); the WordPress Photo Directory publishes every photo under CC0.

## How it was made

`tools/tape_tex.py <photo> tape.png` (Python 3 with Pillow and NumPy):

1. The unwound strip above the roll, x 480..880 and y 250..850 of the photo, turned 13 degrees so it stands straight,
   cut inside its edges (x 152..248, y 180..470 of that part: no background, no curl at the top, no roll at the bottom),
   then turned a quarter so its length runs across.
2. Recoloured to masking tape's cream (`#E7D8B0`): the photo's grey daylight made it green-grey. The paper's own grain
   (the photo's brightness minus an 18 px blur of it, 1.6 times) and a third of its uneven light are kept.
3. Mirrored once along its length, so it repeats without a seam.

No code draws the tape: only its torn ends (a ragged line per game), a slightly darker slit edge and a soft shadow
under it are added at run time.
