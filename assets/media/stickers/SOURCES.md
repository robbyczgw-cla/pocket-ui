# Sources

All pictures generated with PixelLab v2 `create-image-pixflux` (text to pixel art, no input photo), prompts and seeds in requests.json. Credits: 14 (1 per call, 14 calls).

## Stickers
completed, hundred, favourite, playing: generated from text, 2 variants each, cleanest picked.
hundred: ribbons cropped off, outer shape masked to a circle, 1 px dark-gold outline added, centre disc painted flat #EEBE3C.
goty: the frame is drawn in code (outline, white die-cut border, gold foil); the laurel is the left wreath of a generated banner plus a mirrored copy; the text area is flat #E9BE69.
Post-processing: alpha < 200 cleared, rest 255, tight crop, median-cut palette <= 48 colours without dithering (tools/finish.py).

## CRT TV (tv.png)
Generated from text (seed 62), no photo. Wikimedia Commons was searched (CRT television, Trinitron, portable television 1990s, CRT television set): the only PD/CC0 photos are a 1950s Mitsubishi set (CC0, Asanagi, cluttered museum background) and a Sony Trinitron with a hand and remote in front (PD, Csendes Vivien); neither is a clean 90s portable, so not used.
Prompt: "a small 1990s portable CRT television set, front view slightly from above, warm beige plastic body, large curved glass screen on the left, a speaker grille and two round knobs on the right side, small feet, no antenna, no logo, cozy retro, single object, centered, pixel art" (112x96, highly detailed, flat shading, view high top-down).
Edits: a small stray glyph at the bottom right of the body painted over with body colour; glass area filled with #FF00FF (tools/tv.py).
