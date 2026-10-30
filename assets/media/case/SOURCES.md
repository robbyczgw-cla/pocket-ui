# Plastic game cases from photos: sources and licences

The plastic cases (`<kind>-front.png`, `<kind>-spine.png` in `assets/media`, with `case/<kind>.json` beside them) were
made from straight-on photos, converted to pixel art with PixelLab `image-to-pixelart` (v2) the way the cartridges in
`assets/media/game/` and the consoles in `assets/systems/` were made: the photo straightened and fitted to the case's size
(one scale for every case, `docs/DESIGN.md` "Media"), three variants per face (default, strength 150, strength 250 with the
fixer), the closest to the photo kept (`tools/`), the label and band areas painted one flat grey (200, 200, 200) where
pocket-ui puts the cover, the title and the system's colour at run time. The pictures are twice the template size
(`res` 2). Commons has no public-domain or CC0 photo of a GameCube, original Xbox, PSP, DS or 3DS case: those are fitted
from the DVD case and the Wii case photos below, in their own sizes and plastics.

| file (out/) | system | size front / spine | photo (licence, author) | spine block |
|---|---|---|---|---|
| ps2-front/spine | PS2 | 170×240 / 38×240 | DVD_Jewel_Box.jpg (CC0, Gnnwikifikator) | black |
| xbox-front/spine | Xbox | 170×240 / 38×240 | DVD_Jewel_Box.jpg | Xbox green |
| gc-front/spine | GameCube | 170×232 / 36×232 | DVD_Jewel_Box.jpg | dark, silver mark |
| wii-front/spine | Wii | 170×240 / 38×240 | Wii-optical-case.png (PD, Evan Amos) | white, grey line |
| ds-front/spine | DS (plan size, tall) | 158×172 / 32×172 | DVD_Jewel_Box.jpg, lightened to dark grey | white |
| 3ds-front/spine | 3DS (plan size, tall) | 158×172 / 30×172 | Wii-optical-case.png | white, red mark |
|  | DS, real proportions | 172×158 / 32×158 | as ds | white |
| 3 | 3DS, real proportions | 172×158 / 30×158 | as 3ds | white, red mark |
| umd-front/spine | PSP | 132×220 / 30×220 | DVD_Jewel_Box.jpg, darker, radius 6 | black |
| segadisc-front/spine | Dreamcast / Saturn / Mega-CD | 180×160 / 32×160 | CD_Jewel_Case_(Front).jpg (CC0, DiscoA340), as PS1 | – |
| generic-disc-front/spine | generic disc | 170×240 / 38×240 | DVD_Jewel_Box.jpg, warm | band (system colour) |
| generic-cart-front/spine | generic cartridge box | 164×226 / 44×226 | DVD_Jewel_Box.jpg, warm | band |
| generic-digital-front/spine | generic small/digital | 132×172 / 28×172 | DVD_Jewel_Box.jpg, warm | band |

Sources: https://commons.wikimedia.org/wiki/File:DVD_Jewel_Box.jpg (CC0, checked via Commons API 2026-10-29),
https://commons.wikimedia.org/wiki/File:Wii-optical-case.png (PD), https://commons.wikimedia.org/wiki/File:CD_Jewel_Case_(Front).jpg (CC0).
No logos on the used plastic (the moulded Wii logo is on the unused disc half; the DVD logo of the tray photo lies under the label).
No PD/CC0 photo exists on Commons of a GameCube, Xbox, DS, 3DS or PSP case (searched; Keep-case-xbox.jpg is a green
Xbox 360 case): those are the PD DVD/Wii case photos fitted to the real size and plastic colour, the system read from
the spine block, as on the drawn templates.
A cover of another shape than its case (a DS or 3DS cover, a European Saturn cover) is fitted into the front whole,
never cut, its band kept (`cabinet_art::cover_fit`).

The PS1 jewel case (`ps1-*.png`, tools in `tools/jewel/`):

| file | photo | licence, author | kept |
|---|---|---|---|
| ps1-front.png, ps1-spine.png (180×158 / 26×158) | https://commons.wikimedia.org/wiki/File:CD_Jewel_Case_(Front).jpg | CC0, DiscoA340 | front-3, spine-3 (strength 250 + fixer) |

The Compact Disc logo moulded into the tray and the maker's mark lie under the insert (painted label grey); the spine is
the case's ribbed black hinge from the same photo, fitted to the template's width.
