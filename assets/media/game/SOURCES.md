# Cartridge pictures from photos: sources and licences

The cartridges and cards in this folder that have a `<kind>.json` beside them were made from straight-on photos of
the real thing, converted to pixel art with PixelLab `image-to-pixelart` (v2), the way the console pictures in
`assets/systems/` were made (see `assets/systems/SOURCES.md`). They show the shell only. Every logo, brand name and
product name on the plastic was painted over before the conversion, and the label was painted one flat light grey: the
game's cover is put there at run time. `<kind>.json` holds the label rectangle (`label`: x, y, w, h in the PNG's
pixels), the real size in millimetres (`mm`), the source, licence, author, the PixelLab conversion that was kept and
what was removed.

## Photos

The licence was read from the Commons API (`extmetadata.LicenseShortName`) on 2026-10-29; all are public domain. The
original files were used.

| File | Commons file page | Author | Licence | Checked | Removed |
|---|---|---|---|---|---|
| `snes.png` (Super Famicom / PAL) | https://commons.wikimedia.org/wiki/File:SFC_Super_Mario_World_cassette_20090828.jpg | Y katsu 810 | Public domain | 2026-10-29 | the label (already pixelated on Commons) |
| `snes-us.png` (US SNES) | https://commons.wikimedia.org/wiki/File:Nintendo-Super-Game-Boy.jpg | Evan Amos | Public domain | 2026-10-29 | the Super Game Boy's Game Boy slot housing, label, Nintendo seal and upper screws (the shell's top rebuilt as a game cartridge's, see below) |
| `n64.png` | https://commons.wikimedia.org/wiki/File:N64-Game-Cartridge.jpg | Evan Amos | Public domain | 2026-10-29 | the upper part of the label recess (the label); the right outer panel replaced by the left one mirrored (2026-10-30, see below) |
| `gba.png` | https://commons.wikimedia.org/wiki/File:Nintendo-Game-Boy-Advance-Cartridge.jpg | Evan Amos | Public domain | 2026-10-29 | the embossed GAME BOY ADVANCE lettering; the blank label (redone 2026-10-30, see below) |
| `nes.png` | https://commons.wikimedia.org/wiki/File:NES-Cartridge.jpg | Evan Amos | Public domain | 2026-10-29 | the label recess (no lettering on the plastic) |
| `gb.png` | https://commons.wikimedia.org/wiki/File:Nintendo-Game-Boy-Cartridge.jpg | Evan Amos | Public domain | 2026-10-29 | the embossed Nintendo GAME BOY TM lettering; the blank label |
| `gbc.png` (the clear Game Boy Color-only cartridge) | https://commons.wikimedia.org/wiki/File:Nintendo-Game-Boy-Color-Cartridge.jpg | Evan Amos | Public domain | 2026-10-29 | the embossed GAME BOY COLOR lettering; the part number printed on the circuit board; the blank label |
| `ds.png` | https://commons.wikimedia.org/wiki/File:Acekard2guts.jpg | NESJohnny | Public domain | 2026-10-29 | the whole Acekard sticker (lettering, logo, web address); the cut top right corner of an official DS card painted in, the dark blue plastic made dark grey |

Commons has no public-domain front photo of a Super Famicom / PAL SNES cartridge by Evan Amos (his
`SNES-SFAM-Cartridges.jpg` shows both from the connector end), so `snes.png` comes from Y katsu 810's photo, the
label of which was pixelated by its author. Nor has it one of a plain US SNES game cartridge (the two front photos of US cartridges there, of the
MACS training cartridge, are tagged PD-text for their label only and were copied from a blog and a forum, so they were
used as a measuring reference, not as a source): `snes-us.png` comes from Evan Amos's Super Game Boy, which is built
in the US cartridge shell but is taller, with the Game Boy slot housing on top. `tools/usnes_compose.py` keeps the
photo's lower part (the plain band, the grip recess with its two ridges, the side ribs, the bottom screws) at a game
cartridge's proportions (about 1.55 times as wide as high) and rebuilds the centre column above it from the photo's
own plain plastic, with the label recess, the label and the small round window of a US game cartridge. There is no public-domain photo of an official DS game card on Commons (the ones there are
CC BY-SA or CC BY); `ds.png` comes from a photo of the Acekard 2.1, a third-party card made in the DS game card's
shell (the photo also shows its opened twin, which was not used). In a second pass (`tools/ds_notch.py`) the
45-degree cut at the top right corner of an official DS card's label side was painted into the straightened photo,
large enough to stay visible at 150 px, and the Acekard's dark blue plastic was made the dark grey of Nintendo's
cards; after the conversion `tools/ds_clean.py` replaced the photo's glare specks on the shell. No public-domain photo of a 3DS game card was found,
so there is no `3ds.png` from a photo.

## How each picture was made (tools/)

1. **Straightened.** The four corners of the cartridge's front face in the photo were mapped onto a rectangle in the
   real cartridge's proportions (`rectify.py`, a perspective transform; the photos are taken at an angle). For the N64
   the side that came out stretched was corrected with a horizontal remap that makes the two grooves symmetric
   (`hwarp.py`). The outline of faces that are not a plain rectangle (the N64's arch, the steps on the Game Boy family's
   sides) was taken from the photo against its white backdrop (`mask.py`).
2. **Lettering and label painted over** (`paint_nintendo.py`, `label.py`): lettering is filled with the plastic around it
   (blended from above and below where the plastic is shaded); the label becomes one flat light grey with its own
   outline.
3. **Converted.** `assets/systems/tools/prep.py` (crop, long side 1280 px), then PixelLab `image-to-pixelart`
   (`pixellab_i2p.py`, output long side about 160 px). Two to three conversions per cartridge (init image strength
   default, 150, 250 with the fixer); the one closest to the photo was kept (named in `<kind>.json`).
4. **Background removed** with the straightened outline scaled to the pixel art (`maskcut.py`), light leftovers of
   the backdrop dropped on the edge, the label made one flat colour, every pixel fully opaque or fully transparent
   (`finish_nintendo.py`).
5. **Checked** side by side with the photo and with a sample cover in the label (`compare.py`).

PixelLab made 29 conversions for these eight pictures (29 credits, the `cases-photo` line of the shared budget), and
6 more for the second versions of `snes-us.png` and `ds.png` (3 each).


The cartridges and cards in this folder that have a `<kind>.json` next to them were made from a straight-on front photo,
converted with PixelLab image-to-pixelart, the way the console pictures in `assets/systems/` were made (see
`assets/systems/SOURCES.md`). Every logo, brand name and lettering on the plastic was painted over on the photo before
the conversion, and the label was painted one flat light grey: the game's cover is put there at run time. The pictures
carry no trademarks; names such as "Mega Drive" appear in pocket-ui only as plain text.

The licence was read from the Commons API (`extmetadata.LicenseShortName`) and the file page on the date given.

| File | Commons file page | Author | Licence | Checked | What was removed |
|---|---|---|---|---|---|
| `md.png` | https://commons.wikimedia.org/wiki/File:Mega_drive_j-cart.jpg | 32X (Commons user) | Public domain (PD-self; the label PD-ineligible) | 2026-10-29 | J-CART embossing, both pad ports with their lettering, the Micro Machines 2 label |
| `sms.png` | https://commons.wikimedia.org/wiki/File:Sega-Master-System-Game-Cartridge-1.jpg | Evan Amos | Public domain | 2026-10-29 | the red grid label strip with the title and SEGA |
| `gg.png` | https://commons.wikimedia.org/wiki/File:Game_gear_cartridge.jpg | AOMAF2024 (Commons user) | CC0 1.0 | 2026-10-29 | the embossed SEGA in its oval, the (already blurred) label |
| `hucard.png` | https://commons.wikimedia.org/wiki/File:HU1.jpg | Kevin Ott | Public domain (PD-self) | 2026-10-29 | nothing printed; the card face below the contacts became the label |
| `cart.png` | https://commons.wikimedia.org/wiki/File:DonkeyKongAtari2600-(white_background).JPG | Joho345, cut out by Hic et nunc | CC0 1.0 | 2026-10-29 | the Donkey Kong / Nintendo label, the CBS / Atari text sticker |

## Steps

0. **Straighten** (only for photos taken at an angle, `tools/persp.py <photo> <out> <w> <h> TL TR BR BL`): the
   cartridge's front, given by its four corners in the photo, is mapped onto a straight rectangle in its real proportions.

1. **Prepare** (`tools/paint_sega.py <photo> <out> tools/ops.json <kind>`): the areas listed in `tools/ops.json` are
   filled — lettering and add-ons on the plastic row by row from the plastic either side (`hinterp`), the label as one
   flat grey polygon — and a dark cartridge photographed on a light backdrop gets a white background (`"bg": "dark"`).
   Then `assets/systems/tools/prep.py` crops it with a 3 % margin to 1280 px on the long side and writes the object mask.
2. **Convert**: `tools/pixellab_i2p.py prep.png raw-N.png <w> <h> [--strength N] [--fixer]`, PixelLab v2
   image-to-pixelart, output about 160 px on the long side in the photo's proportions. Several variants were made per
   cartridge; the one closest to the photo was kept (named in `<kind>.json`, key `pixellab`).
3. **Finish** (`tools/finish_sega.py raw.png prep.png <kind>.png`): background removed with `assets/systems/tools/cutout.py`
   (flood fill guarded by the photo's object mask), every pixel fully opaque or fully transparent, the label one flat
   grey rectangle, stored as a lossless palette PNG. It prints the label rectangle stored in `<kind>.json`.
4. **Check** (`tools/compare.py`): photo | picture scaled up | picture with a cover of that system in its label.

## Notes per file

- `md.png`: Commons has no public-domain photo of a standard Mega Drive / Genesis cartridge (Evan Amos's photos of
  the console carry no cartridge; the other cartridge photos are CC BY or CC BY-SA). The one public-domain front view
  is of a Codemasters J-Cart, a Mega Drive cartridge with two extra pad ports on top. The ports, their panel and the
  J-CART embossing were filled with the surrounding plastic, so it shows the black Mega Drive shell with its side
  grips and rounded top. Its shell is wider than a plain cartridge; the size in `md.json` is estimated from the photo.
  Four text-only PixelLab generations (`tools/pixflux.py`, no photo) were tried first and not used: they looked like
  no cartridge in particular.
- `sms.png`: Evan Amos photographed the cartridge from above at an angle; its front with the label strip was
  straightened (`tools/persp.py`, corners 460,55 1862,533 1515,1330 58,793 onto 1080 x 690, the real 108 x 69 mm).
  The label on a Master System cartridge is the strip along the top edge, so the cover goes into that strip.
- `gg.png`: Evan Amos has no photo of a Game Gear cartridge on Commons; this straight-on photo is CC0 (a public
  domain dedication). The photo has a lilac colour cast from its backdrop, so it was made grey (`"grey": true` in
  `tools/ops.json`) before the conversion; it was also slightly tilted and straightened. The label keeps its rounded
  top corners: `gg.json`'s rectangle is its bounding box, the label pixels are exactly (200, 200, 200).
- Compare images use real covers where one of that system was at hand (Mega Drive) and otherwise a plain stand-in
  label (`tools/titlelabel.py`), never another system's cover.
- `hucard.png`: the only public-domain HuCard photo on Commons; it shows a white card with its gold contacts on a
  black backdrop. The backdrop was made white and the white plastic a light grey with a one pixel grey edge, so the
  card stays apart from the backdrop in the conversion (`"bg": "light"`, `"outline"` in `tools/ops.json`).
- `cart.png` (every other cartridge system): an Atari 2600 cartridge, chosen because its shell is plain. Both
  stickers were removed (the lower one filled with the plastic colour) and the black plastic was lifted to a neutral
  mid grey (`"lift"`), so the picture can be tinted with a system's plastic colour at run time.
- No public-domain or CC0 photo of a Neo Geo Pocket, WonderSwan or Atari Lynx cartridge was found on Commons
  (2026-10-29: Evan Amos's uploads, the cartridge categories and searches in English, German and Japanese); those
  kinds have no photo picture yet.

## GBA and N64 redone (2026-10-30)

The owner did not recognise the first GBA and N64 cartridges (GBA too light, N64 label edge to edge and too light).
Both were made again from the same public-domain photos, checked against further photos used only as references
(straight-on GBA cartridges: Commons `Grand Theft Auto Advance Cartridge.jpg` and `Simcity2000 advance cartridge by
zeartul.jpg`, public domain; N64 cartridges with labels: `Used Nintendo 64 and Gamecube games.JPG`, CC BY-SA 4.0, not
used as a source). 28 PixelLab credits (budget line `carts-redo`), 5 to 8 variants each, compared on a sheet (photo |
variant | variant with Advance Wars, Pokémon Emerald and Golden Sun, or Super Mario 64, GoldenEye 007 and Mario Kart 64
on the label):

1. Straightened with `rectify.py` (GBA: corners 872,484 2723,1030 2290,2050 482,1451 onto 1140 x 700; N64: corners
   2424,108 3791,1216 2764,1698 1907,938 onto 1160 x 750, then the two grooves made parallel with a second
   `rectify.py` (750,430 1425,455 1585,1210 520,1200 onto 700 x 780) and symmetric with `hwarp.py 190 1405 400 1105`;
   the N64's left outer panel mirrored onto the right, whose shoulder the photo shows at an angle).
2. `prep_cart.py <kind>-prep.json`: the outline (GBA: the wider top part and the body; N64: the curved top with its
   rounded shoulders), the label painted flat (GBA: the whole recessed label, rounded corners; N64: the upper part of the
   recess with its rounded top, the rim above it and the recess below it kept as plastic), the embossed lettering
   painted out, the plastic set to the real colour's brightness (GBA dark grey, N64 light grey), scaled to the real
   proportions (57 x 35 and 116 x 75 mm).
3. `pixellab_redo.py`: GBA kept `create-image-pixflux` with the prepared photo as `init_image` (strength 500, seed 7,
   described as a GBA cartridge from the front); N64 kept `image-to-pixelart` (strength 250, fixer). Not kept: the other
   image-to-pixelart settings (GBA a light rim along the bottom), `image-to-pixelart-pro-flash` (GBA too narrow, N64 with
   square shoulders), a pixflux GBA that wrote lettering on the ridge.
4. `finish_cart.py raw.png <kind>.png <plastic> [--mask prep_mask.png]`: background off, the label grown from its middle
   and painted (214, 214, 214), the plastic set to #4b4c50 (GBA) or #9a9a9a (N64), every pixel solid or empty.
   `sheet_cart.py` makes the comparison sheet.
