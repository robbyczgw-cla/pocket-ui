# Sealed new games: the film's glare (`src/shrink_wrap.rs`)

`glare.png` (256 x 2, grey as RGB) is the glare of real shrink film measured in a public-domain photo: row 0 its long
glare band's profile across it (a slow rise, a jump to white at one edge, a soft fade at the other), row 1 how that
sharp edge strays from a straight line along the band. The code lays two slanted bands of this profile over a case, in
screen pixels, and keeps the rest of the film clear. No PixelLab: the glare is light on the case, not art of its own.

| File | Photo | How |
|---|---|---|
| `glare.png` | [Cellofan.jpg](https://commons.wikimedia.org/wiki/File:Cellofan.jpg) by Holger.Ellgaard (own work), public domain (Commons API, read 2026-11-14), 1350 x 930 | `tools/prep.py`: a pixel counts by its blue and its greyness (the biscuits under the film have little blue, their own shine is pink); in the part over the biscuits (x 300-1000, y 205-510) the band's middle is found in each column from 300 to 540; the mean of 48 px each side is the profile (the film's own glow, 30 %, taken off), the steepest rise above the middle the edge |

14.11.2026: the first film (`foil.png`, the photo's light as a tiled mask, one mask pixel a template pixel) read as smudges
and fog rather than film and was replaced by these measured bands.

The photo is not kept here; `prep.py` makes the file again from the Commons file:

    python3 tools/prep.py Cellofan.jpg glare.png
