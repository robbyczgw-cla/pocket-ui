# Disc templates from photos: sources and licences

Each disc here (`<kind>.png` with `<kind>.json`) is the printed label side of a disc, made from a public-domain Commons
photo the way the cartridges in `assets/media/game/` were (see `../game/SOURCES.md`): straightened to a perfect circle,
every logo and print painted over, converted to pixel art with PixelLab `image-to-pixelart` (v2), cut with an exact circle.
The pictures carry no trademarks and no vendor logos.

The **print area** (where the game's cover art and title are pasted at run time) is one flat colour, exactly `#FF00FF`,
every pixel of it. `<kind>.json`: `mm` (real size), `hole` (radius of the centre hole in px; transparent), `print` (the
annulus `r_in`..`r_out` in px measured from the centre, or for `umd` the label circle), `source`, `licence`, `author`,
`pixellab` (the conversion kept), `logos_removed`. Discs are 160x160 px for their diameter (GameCube: 160 px = 80 mm),
`umd.png` is 156x160 (64x65 mm). Alpha is 0 or 255 everywhere. Outside the print area stays the disc's own look: hub rings,
clear plastic, rim, system bands.

## Photos

Licences read from the Commons API (`extmetadata.LicenseShortName`) on 2026-10-30; all four are public domain.

| Photo (Commons file page) | Author | Licence | Used for |
|---|---|---|---|
| https://commons.wikimedia.org/wiki/File:Audio_Compact_Disc.png | Liamz2r | Public domain | `cd.png`; base of `gamecube.png`, `dreamcast.png`, `saturn.png` (rim, clear hub) |
| https://commons.wikimedia.org/wiki/File:Sony_PlayStation_2_HDD_Utility_Disc_20071206.jpg | Taurolyon at English Wikipedia | Public domain | base of `ps1.png`, `ps2.png`, `xbox.png` (black label, rim) |
| https://commons.wikimedia.org/wiki/File:RVT-R_disc_label_side_(white_background).png | Nintendo | Public domain | `wii.png` (white label, recess ring, rim) |
| https://commons.wikimedia.org/wiki/File:Pursuit_Force_UMD_(front).jpg | Znupi | Public domain | `umd.png` (shell front, clear window ring) |

## Per kind

| File | Photo | What was changed |
|---|---|---|
| `wii.png` | RVT-R label side (a Wii-size disc) | Wii logo, Nintendo logo, RVT-R / DATE / TITLE print and rules painted white; label annulus (r 25..77.5) = print area. Conversion b. |
| `ps2.png` | PS2 HDD Utility Disc | All print painted black; hub replaced by the clear hub of the CD photo; the black band round the hub (r 24..32) kept as a flat shape; silver rim added; print r 32..77.8. Conversion c. |
| `ps1.png` | same photo, recoloured | Black label, the CD photo's clear hub darkened (the PS1's black data side shows through it), black rim; print r 28..77.5. No PD photo of a PS1 disc exists on Commons. Conversion c. |
| `xbox.png` | same photo, recoloured | Black label, flat green band round a clear hub, black ring, grey rim; print r 30..77.8. No PD photo of an original Xbox disc exists on Commons (the Xbox 360 disc scan there shows a data side). Conversion c. |
| `gamecube.png` | CD photo, hub stretched radially | 8 cm disc (160 px = 80 mm): large clear hub (hole r 15, clear to r 30, ring to r 36), silver label; print r 36..77.5. The CC0 Game Boy Player disc photo was too tilted/distorted at the hub to use. Conversion c. |
| `dreamcast.png` | CD photo, recoloured | White-grey hub band with a dark grey ring (the GD-ROM's visible ring) at r 29..35; print r 35..77.5. No PD photo of a GD-ROM disc (the PD `Gdrom.jpg` is a hub close-up). Conversion c. |
| `saturn.png` | CD photo, recoloured | Navy band with a light line round the clear hub, navy rim line; print r 36..77.5. No PD photo of a Saturn disc on Commons. Conversion c. |
| `cd.png` | CD photo | Plain silver CD: clear hub with its black ring, bright rim; print r 29..77.5. Conversion b. |
| `umd.png` | Pursuit Force UMD front | PSP lettering, controller symbol and the whole label (UMD wordmark, Sony/PlayStation/publisher logos, title, legal text) painted over; shell redrawn as light plastic with a darker outline (the photo's strong vignette made its shell grey); the clear window ring kept; print = the whole label circle (no centre hole), r 66.6 px at (78.4, 78.1). Conversion b. |

Photo-less systems (PS1, Xbox, Dreamcast, Saturn) use the closest public-domain disc photo with the system's rings, hub and
base recoloured, as agreed. The bands are painted flat again after the conversion (`finish.py`) so they are exact.

## How they were made (tools/)

1. `discprep.py` / `polarflat.py`: the angled photo is resampled to a perfect circle (disc diameter = 640 px).
2. `compose.py`: flat label colour on the print annulus, the system's bands, hub patch, rim; `umd_prep.py` for the UMD.
3. `pixellab_i2p.py` (budget line `discs`): 2 to 3 conversions per kind (default, strength 250 with fixer, a re-run after
   the hub was desaturated); the one closest to the photo and cleanest was kept. 25 credits in all (8 + 8 + 7 + 2).
4. `finish.py` / `umd_finish.py`: exact circle (alpha 0/255), centre hole transparent, bands re-applied, print area `#FF00FF`.
5. `cmp.py`: photo | template at 300 px | template at 76 px.
6. `recolour.py` (after the first review against photos of the real discs): the UMD's window ring from the photo's blue
   label edge to the dark edge a real UMD shows through its clear window; the PS2's cream hub to clear grey.

`tools/make.sh` lists the commands.
