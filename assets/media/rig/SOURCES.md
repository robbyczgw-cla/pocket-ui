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

## gba-sp

All four rig PNGs are **CC0 1.0**, **modified via PixelLab** (two photo conversions plus a masked empty-port edit):
`base.png`, `over.png` and `under.png` derive from Evan-Amos's public-domain
[Gameboy-Advance-SP-Mk2.jpg](https://commons.wikimedia.org/wiki/File:Gameboy-Advance-SP-Mk2.jpg)
(PD-self; photo retouch SerhioMagpie). `top.png` derives from Benoît Prieur's **CC0 1.0**
[Game Boy Advance SP Tribal Edition - 1.JPG](https://commons.wikimedia.org/wiki/File:Game_Boy_Advance_SP_Tribal_Edition_-_1.JPG).
The open photo supplies the actual lower shell, hinge, inner bezel and screen. The outer-lid photo is
straightened, its tribal decoration removed using adjacent photographed plastic, and recoloured Pearl Blue.
Lettering is removed; PixelLab output is cut out and registered with `assets/systems/tools/sp_finish.py`.
The clamshell's two sides share the photographed hinge, open before insertion and remain open.
The cartridge approaches the **bottom/front** slot contacts first, label facing the underside;
its in-plane turn rotates both axes together, never mirrors the picture. The same geometry is used in Quick.
The GBA rear cartridge retains its separate CC BY-SA 4.0 licence: [game sources](../game/SOURCES.md).

The SP source photograph contains a flush inserted cartridge. A masked PixelLab inpaint removes it from the system picture and `base.png`. `over.png` retains the photographed lower shell and blue rim, with the empty mouth transparent: the moving cartridge is drawn between the dark recess and the shell. All five SP assets are individually credited and dedicated CC0 1.0 in NOTICE.md.

Physical references (orientation only, no art copied): [Nintendo SP instruction booklet, section 5](https://csassets.nintendo.com/noaext/image/private/t_KA_PDF/agsmanual_english?_a=DATC1RAAZAA0),
[iFixit bottom-shell replacement, step 1](https://www.ifixit.com/Guide/Nintendo+Game+Boy+Advance+SP+Bottom+Shell+Replacement/12735)
(underside view with labelled GBA cartridge), and [Willem's real open SP](https://willem.com/en/2017-03-26_repairing-a-nintendo-gameboy-advance-sp/)
(front view with the GB cartridge's screw/rear face up).

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

## nes
Nintendo-Entertainment-System-NES-Console-FL.jpg, Evan Amos (Wikimedia Commons), Public domain (licence read with the Commons API on 2026-11-11).
https://commons.wikimedia.org/wiki/File:Nintendo-Entertainment-System-NES-Console-FL.jpg
(The console alone; the NES-Console-Set photo used for the shelf picture has a controller in front.)
Painted out with the console's own plastic: Nintendo / ENTERTAINMENT SYSTEM on the door, the POWER and RESET print on the buttons, the port numbers 1 and 2, the AUDIO/VIDEO print on the side.
The open console (`base.png` inside the door's place) is a montage: no public-domain photo of an open NES door was found, so the door's place on the photo was filled with the door's own plastic darkened (the slot in front darker, the inside seen from above lighter), as the PS1's well was made. `top.png` is the door cut from the shut console's conversion, `under.png` the same picture with the door's top part darkened (once the door stands up, that part shows its inside; the front panel still shows its outer face, upside down). `over.png` is the console around the opening.
PixelLab image-to-pixelart: 2 credits (shut and open, same seed, strength 300). Scripts: `tools/nes_prep.py`, `tools/nes_finish.py` (in the art workshop; not shipped).
How it goes in, from the NES manual: lift the door, slide the Game Pak in label up, connector first, all the way, press it down, close the door. The cartridge turns round on its way from the case (never mirrored), slides in lying flat, what of it is inside is in the console's shadow and shows only through the opening (`mouth` in `rig.json`), it is pressed down with the click and the door drops shut with the lid's clunk. The Famicom (a top-loader with another cartridge) keeps the plain animation.

## gbc
Nintendo-Game-Boy-Color-FL.jpg, Evan Amos (Wikimedia Commons), Public domain (licence read with the Commons API on 2026-11-11).
https://commons.wikimedia.org/wiki/File:Nintendo-Game-Boy-Color-FL.jpg
Painted out: GAME BOY COLOR on the screen's bezel (the letters filled with the bezel's own dark grey) and the Nintendo emboss under it (filled from the plastic around it; the grey patch the conversion left there evened out with the plastic round it afterwards). COMM, POWER, SELECT, START, A and B are too small to read at this size and were left.
PixelLab image-to-pixelart: 4 credits (four seeds, strength 300; seed 3 kept, the others in the art workshop). The slot is at the back of the top edge, as on the Game Boy: the cartridge turns its back to the player on the way and goes in behind the shell, a sixth of it showing (`behind`, `label_away`). Scripts: `tools/gbc_prep.py`, `tools/gbc_finish.py` (art workshop).

## psp
The PSP-1000 turns round: it comes up showing its front, turns to its back (`front.png`, `Pose::face`, as the game box turns on the card), its UMD cover springs open, the UMD slides down into it label to the player and the cover shuts (`Lid::medium_behind`). The way back turns it to its front again at the end.
- Front: [Sony-PSP-1000-Body.jpg](https://commons.wikimedia.org/wiki/File:Sony-PSP-1000-Body.jpg), Evan Amos, Public domain (Commons API, 2026-11-11). Rectified straight on (the screen's four corners to its real 95 x 54 mm), the PlayStation logo, SONY, PSP, POWER and HOLD painted out; the glossy black came out grey and was darkened round the screen afterwards (a recolour).
- Back: [PSP In Hand Back.JPG](https://commons.wikimedia.org/wiki/File:PSP_In_Hand_Back.JPG), Punkidow, Public domain (Commons API, 2026-11-11). Cut out along its outline (no hand), SONY, UMD, the PSP letters in the ring, PUSH and MagicGate painted out; skin colour bleeding into the bottom edge and the MagicGate patch evened out with the plastic next to them after the conversion.
- The bay behind the cover: [PSP med UMD luckan öppen.JPG](https://commons.wikimedia.org/wiki/File:PSP_med_UMD_luckan_%C3%B6ppen.JPG), Jalla (sv.wikipedia), Public domain (Commons API, 2026-11-11): its bay (the cover taken away, the mechanism continued down, darker) warped from its oblique view onto the cover's place on the back.
- The UMD's back is not shown (its label faces the player). A conversion of it from "Pursuit Force UMD (back).jpg" (PD, Znupi) was made for the first scene and is kept in the art workshop, not shipped.
"PSP2000 UMDPlacement.jpg" (CC BY-SA) and "UMD slot at PSP-1000 rear" (CC BY) were only looked at, not used.
PixelLab image-to-pixelart: 23 credits in all for the PSP: 6 for a first scene seen from behind and above (dropped after review: a flat strip, not recognisably a PSP), 4 + 4 for the front (the second set after rectifying it; seed 19 kept), 3 + 3 for the back shut and open (seed 9 kept), 3 for the UMD's back. How the UMD goes in, from the PSP-1000 manual: open the cover, slide the UMD in label to the rear of the system, shut the cover. Scripts: `tools/psp2_prep.py`, `tools/psp2_finish.py` (art workshop).

## mastersystem
Sega-Master-System-Console-FL.jpg, Evan Amos (Wikimedia Commons), Public domain (Commons API, 2026-11-11). https://commons.wikimedia.org/wiki/File:Sega-Master-System-Console-FL.jpg (the console alone; the set photo of the shelf picture has a pad in front).
Painted out with the plastic round them: SEGA MASTER SYSTEM/Power Base, the red panel's print and diagram (RESET, PAUSE, CARTRIDGE INPUT and its arrow, CARTRIDGE, POWER ON with its triangle, CONTROL 1/2, CARD, the TV SYSTEM box: the panel is its own plain red with its two buttons and the hatching at its foot), the POWER plate, CONTROL 1/2, CARD INPUT.
PixelLab image-to-pixelart: 4 credits (four seeds; seed 7 kept). The cartridge stands in the slot on top, label to the front, three fifths out; `over.png` is the console in front of the slot's line. Scripts: `tools/sms_prep.py`, `tools/sms_finish.py` (art workshop).

## gamegear
Game-Gear-Handheld.jpg, Evan Amos (Wikimedia Commons), Public domain (Commons API, 2026-11-11). https://commons.wikimedia.org/wiki/File:Game-Gear-Handheld.jpg
Painted out with the plastic round them (on the bezel: its own black, not its reflections): GAME GEAR and its three coloured dots, SEGA, PORTABLE VIDEO GAME SYSTEM, POWER, START.
PixelLab image-to-pixelart: 4 credits (four seeds; seed 7 kept). The cartridge goes in at the back of the top edge, as on the Game Boy: it turns its back to the player on the way (`label_away`, `assets/media/game/gg-back.png`) and goes in behind the shell, a third of it showing; the power light comes on. Scripts: `tools/gg_prep.py`, `tools/gg_finish.py` (art workshop).

## pcengine
NEC-PC-Engine-Console-FL.jpg, Evan Amos (Wikimedia Commons), Public domain (Commons API, 2026-11-11). https://commons.wikimedia.org/wiki/File:NEC-PC-Engine-Console-FL.jpg (the console alone; the shelf picture's set photo has its pad). It also stands for the TurboGrafx-16 (`tg16`), one picture for both.
Painted out: the red PC Engine logo, the embossed lettering on the two plates beside it, the print inside the card slot.
PixelLab image-to-pixelart: 4 credits (four seeds; seed 3 kept). The HuCard slides in lying on the slot's ramp, contacts first, label up, and stays half out in the finger recess; `over.png` is the console beyond the slot's back edge. Scripts: `tools/pce_prep.py`, `tools/pce_finish.py` (art workshop).

## 3ds
Nintendo3DSAqua.png, Xviila (Wikimedia Commons), CC0 (Commons API, 2026-11-11). https://commons.wikimedia.org/wiki/File:Nintendo3DSAqua.png
Replaces the first picture (Nintendo-3DS-AquaOpen.jpg, Evan Amos, public domain), poor in review: its strong slant
made it look off the middle, its screens were pale. Painted out: SELECT, HOME and its house, START, POWER, OFF and the 3D
slider's wedge, the two light symbols on the front edge. The lower screen, a pale reflection in the photo, set to the top
screen's grey; the photo turned 10 degrees to lean as the DS picture does; 262 px wide for its 134 mm against the DS
picture's 290 for 149 mm. After the conversion the screens and the black top half, warm grey in the photo, set to a
neutral dark grey with the DS picture's cool tint (brightness kept), and the backdrop's white specks on the outline cleared.
PixelLab image-to-pixelart: 6 credits (six seeds, strength 300; seed 7 kept). The card goes in at the rear of the lower
half under the hinge's middle, as on the DS: behind the console, its back to the player. The card is the DS card (front
and back) until there is a photo of a 3DS card of our own. Scripts: `tools/n3ds2_prep.py`, `tools/n3ds2_finish.py`
(scratch/smooth-anim).

## All rigs, 2026-11-02
The photo's light backdrop left on `base.png`'s outline was cleared: an edge pixel of low colour clearly lighter than the plastic within 5 pixels (30 to 670 pixels a console), and on the GBA three grey patches of the photo's shadow at its corners. No PixelLab. The GameCube's `under.png` kept the photo's white backdrop along the lid's front edge: filled with the plastic above it.

## Discs, 2026-11-08
No new pictures. The Saturn's `under.png` (the lid's underside) was darkened to the brightness of its top (RGB x 0.446; it was a pale grey slab above a black console, the underside of that lid is the same black moulding): a recolour of the existing PixelLab picture, no new conversion. The Saturn's and Dreamcast's `seat.u`/`seat.v` were enlarged by 1.25 and 1.1 so their discs are as wide beside the console as the real 12 cm disc (tests in `src/insert_rig.rs`).
