# Consoles at a game's start: sources and licences

Each `<system>/` holds a console for Cozy's launch (`src/insert_rig.rs`): `base.png`, the console without what moves; `over.png`, what is in front of the medium; `top.png` + `under.png`, a lid's two sides, or `tray.png`; `rig.json`, where things are. Real objects only: public-domain (Evan Amos, Wikimedia Commons) or CC0 photos, logos and lettering painted out with the plastic around them, montaged where no photo of the open state exists, then converted with PixelLab `image-to-pixelart` v2. The licence was read from the Commons API on 2026-10-31.

## gamecube
GameCube rig. Photos (Public domain, Evan Amos): https://commons.wikimedia.org/wiki/File:Nintendo-GameCube-Console-FL-Open.jpg and https://commons.wikimedia.org/wiki/File:Nintendo-GameCube-Console-FL.jpg (same pose, registered). Painted out: the logo on the lid, the emboss, the warning sticker under the lid.

## dreamcast
dreamcast rig. Photos (Public domain, Evan Amos, Wikimedia Commons, licence checked with the Commons API):
- Sega-Dreamcast-Console-FL.jpg https://commons.wikimedia.org/wiki/File:Sega-Dreamcast-Console-FL.jpg
- Nintendo-GameCube-Console-FL-Open.jpg https://commons.wikimedia.org/wiki/File:Nintendo-GameCube-Console-FL-Open.jpg (spindle/turntable/laser piece, greyed, in the lid opening; wall = Dreamcast plastic darkened)
Painted out with the plastic around: Dreamcast logo sticker, SEGA, A/B/C/D, Windows CE notice, POWER/OPEN lettering.
Underside: lid's own plastic darkened + the console's round (clean) button as clamp. PixelLab 3 credits.

## gb
Game-Boy-FL.jpg, Evan Amos, public domain, https://commons.wikimedia.org/wiki/File:Game-Boy-FL.jpg
Painted out (tools/gb_prep.py): Nintendo GAME BOY lettering, OFF-ON, DOT MATRIX text, BATTERY, SELECT/START/A/B, PHONES, EXT. CONNECTOR.
PixelLab i2p x1 (strength 300). behind:true, no over.png.

## gba
Nintendo-Game-Boy-Advance-Purple-FL.jpg, Evan Amos, public domain, https://commons.wikimedia.org/wiki/File:Nintendo-Game-Boy-Advance-Purple-FL.jpg
Painted out with the surrounding plastic (tools/gba_prep.py): Nintendo emboss, GAME BOY ADVANCE sticker, START/SELECT/POWER/PHONES/VOLUME lettering.
PixelLab i2p x2 (strength 300). Medium sticks out behind the top edge (behind:true), no over.png.

## megadrive
Sega-Genesis-Mk2-Console-01.jpg, Evan Amos (Wikimedia Commons), Public domain.
https://commons.wikimedia.org/wiki/File:Sega-Genesis-Mk2-Console-01.jpg
(The Mk1 set photo has a controller covering the console's front, so the Mk2 console-alone photo was used; the CC BY-SA Sonic+Knuckles lock-on photo was only looked at, not used.)
Painted out with the console's own plastic: GENESIS logo, SEGA plate lettering, CARTRIDGE INPUT text + triangle, POWER/RESET button labels, CONTROL 1/2 engraving. No LED: the photo shows none. PixelLab image-to-pixelart, 1 credit. Tools: tools/md_prep.py, md_finish.py, inpaint_lib.py.
Round 2: cart picture is 112x84 mm (assets/media/game/md.png), so v = 37 art px (not 50); seat.at raised 16 px so about 70 % stays out and the label reads.

## n64
Nintendo-64-Console-FL.jpg, Evan Amos (Wikimedia Commons), Public domain.
https://commons.wikimedia.org/wiki/File:Nintendo-64-Console-FL.jpg
Painted out with the console's own plastic: embossed Nintendo logo on top, MEMORY EXPANSION lettering, RESET lettering, NINTENDO 64 lettering and logo on the front screen plate (plate left plain black). Slot floor (light grey in photo) darkened to the slot's own black. PixelLab image-to-pixelart, 1 credit. Tools: tools/n64_prep.py, n64_finish.py, inpaint_lib.py.

## nds
Nintendo-DS-Fat-Blue.jpg, Evan Amos, public domain, https://commons.wikimedia.org/wiki/File:Nintendo-DS-Fat-Blue.jpg
Painted out (tools/ds_prep.py): NINTENDO DS logo on the lower shell. PixelLab i2p x1 (strength 300). behind:true, slot = rear middle of the lower half (hinge middle).

## ps2
Console: File:Sony-PlayStation-2-30001-Console-FL.jpg, Evan Amos, public domain (Wikimedia Commons).
Tray: File:Benytone CD33 - transport - tray open.jpg, User:Retired electrician, CC0 (Commons). Cropped to the tray, recoloured black, floor seen through its openings painted with its own plastic, warped into the console's perspective.
Painted out with the console's own plastic: PS2 logo and "playstation2" on top, DISC/DVD marks, PS mark on the front, MEMORY CARD / MagicGate lettering and numbers, RESET lettering, SONY.
PixelLab image-to-pixelart: 2 credits (base, tray), strength 300. "PS2 Tray.jpg" (CC BY-SA) was looked at only, not used.
Scripts: tools/ps2_prep.py ps2_base.py ps2_tray.py ps2_finish.py
Round 2: slot moved two ridges higher on the front (base re-converted, +1 credit), tray cut to 130 mm out, 16 art px transparent padding at the bottom of base/over, disc hover in front of the tray (flat).

## psx
psx rig. Photos (all Public domain, Evan Amos, via Wikimedia Commons, checked with the Commons API):
- Sony-PlayStation-5501-Console-FL.jpg  https://commons.wikimedia.org/wiki/File:Sony-PlayStation-5501-Console-FL.jpg
- Nintendo-GameCube-Console-FL-Open.jpg https://commons.wikimedia.org/wiki/File:Nintendo-GameCube-Console-FL-Open.jpg (spindle/turntable/laser piece, greyed, montaged into the PS1 well; PS1 well wall = PS1's own plastic darkened)
Painted out with the plastic around: SONY, PlayStation logo, CD logo, RESET/POWER/OPEN lettering, memory-card labels and numbers.
Underside: the lid's own plastic darkened, the console's round (painted-clean) power button as the clamp. Converted with PixelLab (6 credits).

## saturn
saturn rig. Photos (Public domain, Evan Amos, Wikimedia Commons, licence checked with the Commons API):
- Sega-Saturn-Console-NA-Mk-I-FL.jpg https://commons.wikimedia.org/wiki/File:Sega-Saturn-Console-NA-Mk-I-FL.jpg
- Nintendo-GameCube-Console-FL-Open.jpg https://commons.wikimedia.org/wiki/File:Nintendo-GameCube-Console-FL-Open.jpg (spindle/turntable/laser piece, greyed, in the lid opening)
Painted out with the plastic around: Saturn logo, SEGA SATURN, SEGA, POWER/ACCESS/OPEN/RESET lettering. Underside: lid plastic blurred flat + darkened, console's own round button as clamp. PixelLab 4 credits.
Round 2: Photo highlights lifted (gamma 0.5) before PixelLab, lid outline traced from the photo's groove, underside = blurred real lid plastic at mid grey + real clamp (console's own round button). PixelLab +3 credits.

## snes-us
Nintendo-Super-NES-Console-FR.jpg, Evan Amos (Wikimedia Commons), Public domain.
https://commons.wikimedia.org/wiki/File:Nintendo-Super-NES-Console-FR.jpg
Painted out with the console's own plastic: SUPER NINTENDO logo, Nintendo logo on top, POWER/RESET/EJECT/ON-OFF lettering. Flap opening darkened with dark grey plastic tone (controller port interior). PixelLab image-to-pixelart, 1 credit. Tools: tools/snesus_prep.py, snesus_finish.py.

## snes
Nintendo-Super-Famicom-Console-FL.jpg, Evan Amos (Wikimedia Commons), Public domain.
https://commons.wikimedia.org/wiki/File:Nintendo-Super-Famicom-Console-FL.jpg
Painted out with the console's own plastic: Nintendo/Super Famicom lettering, POWER/EJECT/RESET/ON-OFF labels, colour logo, slot text. Slot opening darkened with the RESET button's dark plastic. PixelLab image-to-pixelart, 1 credit. Tool: tools/snes_prep.py, snes_finish.py.

## wii
Wii-console.jpg, Evan Amos, public domain, https://commons.wikimedia.org/wiki/File:Wii-console.jpg
No retouching of the console or the Wii Remote (as in assets/systems/wii.png); only the separate wrist strap on the white background was cut away. PixelLab i2p x1 (strength 300).
over.png = base right of the slot's left edge (hides the disc once past the slot); seat.flat:true.
Painted out after the conversion, with the plastic around them: the Wii lettering on the console and on the Wiimote.

## xbox
Console: File:Xbox-Console-FL.jpg, Evan Amos, public domain (Wikimedia Commons).
Tray: File:Benytone CD33 - transport - tray open.jpg, User:Retired electrician, CC0 (Commons), warped in as above; the tray's front band is the console's own photographed band (the door with the CD/Dolby marks, painted out), moved out with the tray.
Painted out with the console's own plastic: XBOX lettering on the front plate and on the green jewel, CD/Dolby marks, player numbers 1-4 and the maker's name (blurred away).
PixelLab image-to-pixelart: 2 credits (base, tray), strength 300.
Scripts: tools/xbox_prep.py xbox_base.py xbox_tray.py xbox_finish.py
Round 2: tray rebuilt at 130 mm out (+1 credit), 5 art px transparent padding at the bottom of base/over, disc hover in front of the tray (flat).

