"""Prepare rear cartridge photos for PixelLab, or cut out its result with the photo's outline.

Usage: rear_photos.py prepare SOURCE_DIR WORK_DIR
       rear_photos.py finish WORK_DIR ASSET_DIR
Coordinates trace the photographed shells, not replacement artwork. See ../REAR-SOURCES.md.
"""
from pathlib import Path
import sys
from PIL import Image, ImageDraw, ImageOps, ImageEnhance, PngImagePlugin

# Photo crop, target pixels, lettering to remove (fractions of the cropped photo), shell brightness.
SPECS = {
    'gba': ('Game Boy Advance game cartridges - rear view.jpg', (395, 706, 1890, 1590), (152, 92), [(0.30, .21, .70, .48)], .90),
    'gb': ('baseball-cartridge-jp-back.jpg', (0, 0, 2327, 2631), (132, 150), [(.22, .025, .80, .17)], 1.0),
    'gbc': ('harvest-moon-3-gbc-cartridge-jp-back.jpg', (0, 0, 3004, 3418), (132, 150), [(.17, .05, .85, .35)], .85),
    'ds': ('lux-pain-cartridge-na-back.jpg', (0, 0, 1651, 1766), (151, 142), [(.18, .20, .89, .54), (.10, .69, .90, .79)], 1.0),
}
CREDITS = {
    'gba': ('ITEagle Europe – Sebastiaan Broekhoven', 'Game_Boy_Advance_game_cartridges_-_rear_view.jpg', '4.0'),
    'gb': ('Christopher Wells', 'https://archive.org/details/baseball-cartridge-jp', 'CC0 1.0'),
    'gbc': ('Christopher Wells', 'https://archive.org/details/harvest-moon-3-gbc-cartridge-jp', 'CC0 1.0'),
    'ds': ('Christopher Wells', 'https://archive.org/details/lux-pain-cartridge-na', 'CC0 1.0'),
}
OUTLINES = {
    'gba': [(0.04,0),(.96,0),(1,.04),(1,.20),(.98,.23),(.98,.96),(.96,1),(.03,1),(.01,.96),(.01,.23),(0,.20),(0,.04)],
    'gb': [(.11,0),(.96,0),(.99,.03),(1,.96),(.97,1),(0,1),(0,.05),(.10,.05)],
    'gbc': [(.02,0),(.98,0),(1,.02),(1,.99),(.99,1),(.01,1),(0,.99),(0,.02)],
    'ds': [(.02,0),(.97,.005),(1,.035),(1,.49),(.96,.49),(.96,.63),(1,.63),(1,.81),(.94,.95),(.87,1),(.02,1),(0,.97),(0,.03)],
}

def prepare(src, work):
    work.mkdir(parents=True, exist_ok=True)
    for kind, (filename, crop, size, lettering, brightness) in SPECS.items():
        im = Image.open(src / filename).convert('RGB').crop(crop)
        w,h = im.size
        # Retouch only lettering with plastic from just above and below, preserving the photo's shade.
        for x0,y0,x1,y1 in lettering:
            x0,y0,x1,y1 = int(x0*w),int(y0*h),int(x1*w),int(y1*h)
            px=im.load()
            for x in range(x0,x1):
                a,b=px[x,max(0,y0-3)],px[x,min(h-1,y1+3)]
                for y in range(y0,y1):
                    t=(y-y0)/(y1-y0);px[x,y]=tuple(round(a[c]*(1-t)+b[c]*t) for c in range(3))
        im = ImageEnhance.Brightness(ImageOps.grayscale(im)).enhance(brightness).convert('RGB') if kind in ('gb','gba') else ImageEnhance.Brightness(im).enhance(brightness)
        mask=Image.new('L',(w,h));ImageDraw.Draw(mask).polygon([(int(x*w),int(y*h)) for x,y in OUTLINES[kind]],fill=255)
        # White outside the actual shell, with a small margin for PixelLab.
        im=Image.composite(im,Image.new('RGB',(w,h),'white'),mask)
        im=im.resize((size[0]*6,size[1]*6),Image.Resampling.LANCZOS)
        mask=mask.resize(im.size,Image.Resampling.NEAREST)
        im=ImageOps.expand(im,border=24,fill='white');mask=ImageOps.expand(mask,border=24,fill=0)
        im.save(work/f'{kind}-prep.png');mask.save(work/f'{kind}-mask.png')
        print(kind,'PixelLab output',size[0]+8,size[1]+8)

def finish(work, out):
    for kind, (_, _, size, _, _) in SPECS.items():
        raw=Image.open(work/f'{kind}-raw.png').convert('RGBA')
        mask=Image.open(work/f'{kind}-mask.png').resize(raw.size,Image.Resampling.NEAREST)
        # Remove only bright backdrop remnants touching the photographed outline.
        from collections import deque
        alpha=mask.load();px=raw.load();w,h=raw.size
        q=deque((x,y) for y in range(h) for x in range(w) if alpha[x,y]==0)
        seen=set(q)
        while q:
            x,y=q.popleft()
            for xx,yy in [(x-1,y),(x+1,y),(x,y-1),(x,y+1)]:
                if 0<=xx<w and 0<=yy<h and (xx,yy) not in seen and min(px[xx,yy][:3])>220:
                    alpha[xx,yy]=0;seen.add((xx,yy));q.append((xx,yy))
        if kind == 'ds':
            # The photo's pale tabletop also shows through the gaps at the terminal edge.
            for y in range(int(h * .90), h):
                for x in range(w):
                    if min(px[x,y][:3]) > 190:
                        alpha[x,y] = 0
        raw.putalpha(mask)
        # PixelLab's eight-pixel margin becomes four on each side.
        raw=raw.crop((4,4,4+size[0],4+size[1]))
        author, source, version = CREDITS[kind]
        info = PngImagePlugin.PngInfo()
        for key, value in {
            'Author': author,
            'Source': source if source.startswith('https:') else 'https://commons.wikimedia.org/wiki/File:' + source,
            'License': version if version.startswith('CC0') else 'CC BY-SA ' + version,
            'LicenseURL': 'https://creativecommons.org/publicdomain/zero/1.0/' if version.startswith('CC0') else 'https://creativecommons.org/licenses/by-sa/' + version + '/',
            'Modification': 'modified via PixelLab; cropped, retouched, colour adjusted, pixel-art conversion, cut out',
        }.items():
            info.add_text(key, value)
        raw.save(out/f'{kind}-back.png', pnginfo=info)

if __name__ == '__main__':
    {'prepare':prepare,'finish':finish}[sys.argv[1]](Path(sys.argv[2]),Path(sys.argv[3]))
