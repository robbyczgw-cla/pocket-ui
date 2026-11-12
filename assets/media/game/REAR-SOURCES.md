# Rear cartridge shells used at launch

The screen side of a Game Boy, Game Boy Color, Game Boy Advance, Nintendo DS or 3DS or Game Gear faces the
player. Its cartridge label faces away while entering the rear slot. The front templates
remain the labelled faces shown as the game leaves its case; these separate rear photos
supply the reverse face after the turn. Neither face is mirrored or turned upside down.

Each derived PNG is **modified via PixelLab** and carries its author, source and licence
in its PNG text metadata and in the shipped `NOTICE.md`. The GBA image retains **CC BY-SA 4.0**;
the GB, GBC and DS images are released under **CC0 1.0**, matching the source photographs.
These artwork licences are separate from the program's GPL. Preserve the GBA attribution,
source, licence and modification notice and share its adaptations under CC BY-SA 4.0.
No endorsement by the photographers is implied.

| Image | Photographer / source | Source and derived-image licence |
| --- | --- | --- |
| `gba-back.png` | ITEagle Europe – Sebastiaan Broekhoven, [Game Boy Advance game cartridges - rear view.jpg](https://commons.wikimedia.org/wiki/File:Game_Boy_Advance_game_cartridges_-_rear_view.jpg) | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) |
| `gb-back.png` | Christopher Wells, [Baseball cartridge (JP)](https://archive.org/details/baseball-cartridge-jp), `02_cartridge_back.jpg` | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) |
| `gbc-back.png` | Christopher Wells, [Harvest Moon 3 GBC cartridge (JP)](https://archive.org/details/harvest-moon-3-gbc-cartridge-jp), `02_cartridge_back.jpg` | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) |
| `ds-back.png` | Christopher Wells, [Lux-Pain cartridge (NA)](https://archive.org/details/lux-pain-cartridge-na), `02_cartridge_back.jpg` | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) |
| `gg-back.png` | AOMAF2024, [Game gear cartridge.jpg](https://commons.wikimedia.org/wiki/File:Game_gear_cartridge.jpg) (the front: no free photo of a Game Gear cartridge's back was found on Commons or the Internet Archive, 2026-11-11) | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) |

Licences and authors were checked on 2026-11-08 through Commons `imageinfo.extmetadata`
and the Internet Archive items' `photographer` and `licenseurl` metadata. The existing
front-shell and console sources remain as listed in SOURCES.md.

Modifications: crop to one shell, remove embossed branding and serial lettering with
adjacent photographed plastic, normalise brightness, convert with PixelLab v2
image-to-pixelart, cut out using the photographed outline. The screw, moulding, rear
connector end and (GBC) translucent shell come from the photos. No shell geometry is drawn
as replacement art. `tools/rear_photos.py` records crops, retouch regions and cutout outlines;
`tools/rear-pixellab-log.jsonl` records the selected conversions. The PNGs have the front
templates' dimensions so the turn keeps each cartridge's physical size.

The source search covered the Commons cartridge categories (including PD-self/CC0 uploads),
Openverse with CC0/PDM filters, Flickr, stock-photo sites, Smithsonian and Internet Archive.
The latter supplied three usable CC0 rear photographs, replacing initial CC BY-SA choices.
For GBA the examined PD/CC0 candidates showed fronts, open PCBs, special cartridge forms,
partly occluded installed cartridges or a reproduction shell. None supplied the complete
standard rear shell used here, so the attributed CC BY-SA 4.0 photograph was retained.

`gg-back.png` (2026-11-11) is not a photo of the back: it is the Game Gear front picture (`gg.png`, PixelLab from the
CC0 photo above) with its label and its moulded SEGA oval filled with that shell's own plastic (median colour, a little
grain) and mirrored, as the back is seen from behind: a plain black shell with the front's outline and top ridges. The
3DS uses the DS card's back (`ds-back.png`): no free photo of a 3DS card was found (the Commons ones are CC BY-SA).
