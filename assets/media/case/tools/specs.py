"""specs.py > specs.json: every plastic case's faces (photo quad, size at res 2, rim, fixed colours, label/band)."""
import json
DVD = "photos/DVD_Jewel_Box.jpg"; WII = "photos/Wii-optical-case.png"; JEWEL = "photos/CD_Jewel_Case__Front_.jpg"
DF = [0, 0, 1622, 0, 1622, 2248, 0, 2248]          # the DVD case's tray half, straight on
DS_ = [1560, 0, 1622, 0, 1622, 2248, 1560, 2248]   # its hinge wall
WF = [24, 22, 576, 22, 576, 778, 24, 778]          # the Wii case's cover half
WS = [578, 22, 625, 22, 625, 778, 578, 778]        # the Wii case's hinge
DARK = [[85, 85, 90], [10, 10, 12]]; WHITE = [[255, 255, 255], [168, 168, 174]]; GREY = [[140, 140, 146], [38, 38, 44]]
U = {"source": "https://commons.wikimedia.org/wiki/File:DVD_Jewel_Box.jpg", "licence": "CC0", "author": "Gnnwikifikator"}
UW = {"source": "https://commons.wikimedia.org/wiki/File:Wii-optical-case.png", "licence": "Public domain", "author": "Evan Amos"}
UJ = {"source": "https://commons.wikimedia.org/wiki/File:CD_Jewel_Case_(Front).jpg", "licence": "CC0", "author": "DiscoA340"}


def face(photo, quad, size, label, flip=True, radius=4, bevel=3, cols=DARK, tone=None, tint=None, fixed=(), band=None):
    f = {"photo": photo, "quad": quad, "size": size, "label": label, "flip": flip, "radius": radius}
    if bevel: f["bevel"], f["bevel_colours"] = bevel, cols
    if tone: f["tone"] = tone
    if tint: f["tint"] = tint
    if fixed: f["fixed"] = [{"rect": r, "colour": c} for r, c in fixed]
    if band: f["band"] = band
    return f


def keep(w, h, sw, label_top, top, dash, cols=DARK, photo=(DVD, DF, DS_), tone=None, tint=None, radius=4, meta=U, note=""):
    """A keep case: the sleeve over the whole front, on the spine a block at the top in the case's own colours."""
    p, fq, sq = photo
    fixed = [([5, 5, sw - 10, label_top - 8], top)]
    if dash:
        fixed.append(([sw // 2 - 4, (label_top - 8) // 2 + 3, 8, 3], dash))
    fixed.append(([5, label_top - 4, sw - 10, 2], [120, 120, 126]))
    return {"front": face(p, fq, [w, h], [6, 6, w - 12, h - 12], radius=radius, cols=cols, tone=tone, tint=tint),
            "spine": face(p, sq, [sw, h], [5, label_top, sw - 10, h - label_top - 5], flip=False, radius=3, bevel=2,
                          cols=cols, tone=tone, tint=tint, fixed=fixed),
            "meta": dict(meta, note=note)}


def generic(w, h, sw, fb, sb, note):
    t, ti = [1.05, 8], [12, 4, -6]
    return {"front": face(DVD, DF, [w, h], [6, 6 + fb + 2, w - 12, h - 12 - fb - 2], tone=t, tint=ti, band=[6, 6, w - 12, fb]),
            "spine": face(DVD, DS_, [sw, h], [5, 5 + sb + 3, sw - 10, h - 10 - sb - 3], flip=False, radius=3, bevel=2,
                          tone=t, tint=ti, band=[5, 5, sw - 10, sb]),
            "meta": dict(U, note=note)}


S = {
 "ps2": keep(170, 240, 38, 40, [14, 14, 16], [110, 110, 116], note="PS2: black DVD keep case, black block at the top of the spine"),
 "xbox": keep(170, 240, 38, 40, [62, 150, 38], [168, 220, 140], note="Xbox: black keep case, the green block at the top of the spine"),
 "gc": keep(170, 232, 36, 40, [34, 34, 40], [150, 150, 162], note="GameCube: the smaller black case, dark block with a silver mark on the spine"),
 "wii": keep(170, 240, 38, 40, [247, 247, 247], [170, 170, 176], cols=WHITE, photo=(WII, WF, WS), meta=UW, tone=[0.3, 180],
             note="Wii: white keep case (Evan Amos's photo: the cover half mirrored as the front, the hinge as the spine)"),
 "ds": keep(158, 172, 32, 32, [240, 240, 240], [90, 90, 96], cols=GREY, tone=[1.0, 42], tint=[0, 0, 4],
            note="DS: the small dark-grey see-through case, white block with a grey mark on the spine"),
 "3ds": keep(158, 172, 30, 32, [244, 244, 244], [206, 40, 40], cols=WHITE, photo=(WII, WF, WS), meta=UW, tone=[0.3, 180],
             note="3DS: the small white case (white plastic from the Wii case photo), white block with a red mark on the spine"),
 "umd": keep(132, 220, 30, 36, [12, 12, 14], [120, 120, 126], tone=[0.75, 0], radius=6,
             note="PSP: the tall narrow black UMD case, black block on the spine"),
 "segadisc": {"front": face(JEWEL, [320, 125, 4220, 93, 4390, 3545, 139, 3572], [180, 160], [19, 5, 156, 150], flip=False, radius=1, bevel=0),
              "spine": face(JEWEL, [320, 125, 660, 122, 513, 3572, 139, 3572], [32, 160], [5, 8, 22, 144], flip=False, radius=1, bevel=0),
              "meta": dict(UJ, note="Dreamcast / Saturn / Mega-CD: the CD jewel case (clear front, black tray), ribbed hinge as the spine")},
 "generic-disc": generic(170, 240, 38, 32, 30, "generic disc case: warm dark plastic, band at the top in the system's colour"),
 "generic-cart": generic(164, 226, 44, 30, 30, "generic cartridge box: warm dark plastic, band at the top in the system's colour"),
 "generic-digital": generic(132, 172, 28, 24, 24, "generic small digital case: warm dark plastic, band at the top in the system's colour"),
}
for k in ("ds", "3ds"):
    import copy
    L = copy.deepcopy(S[k]); L["front"]["size"] = [172, 158]; L["front"]["label"] = [6, 6, 160, 146]
    L["spine"]["size"][1] = 158; sl = L["spine"]["label"]; sl[3] = 158 - sl[1] - 5
    L["meta"]["note"] += "; landscape: the real case is 137 mm wide, 125 mm high"
    S[k + "-wide"] = L
print(json.dumps(S, indent=1))
