"""Prepare real SP photos for PixelLab; finish only masks, crops and registers the photographs.

Usage: sp_photo.py prepare SOURCE_DIR OUT_DIR; then sp_finish.py CONVERTED_DIR REPO
Evan-Amos PD open Pearl Blue SP, Benoit Prieur CC0 closed SP (sources in SOURCES.md).
"""
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter

def erase(im, poly, dark=None):
    mask = Image.new('L', im.size); ImageDraw.Draw(mask).polygon(poly, fill=255)
    px, m = im.load(), mask.load()
    todo = {(x,y) for y in range(im.height) for x in range(im.width)
            if m[x,y] and (dark is None or min(px[x,y][:3]) < dark)}
    while todo:
        done = {}
        for x,y in todo:
            ns = [px[a,b] for a,b in [(x-1,y),(x+1,y),(x,y-1),(x,y+1)]
                  if 0<=a<im.width and 0<=b<im.height and (a,b) not in todo]
            if ns: done[x,y] = tuple(sum(c[k] for c in ns)//len(ns) for k in range(3))
        if not done: break
        for pt,c in done.items(): px[pt] = c
        todo.difference_update(done)

def prepare(src, out):
    out.mkdir(parents=True, exist_ok=True)
    im = Image.open(src/'sp-pd.jpg').convert('RGB'); im.thumbnail((1000,1000))
    # Bezel wordmark, SELECT/START and the tiny power-switch legend: only photographed material replaces lettering.
    ImageDraw.Draw(im).polygon([(467,268),(700,392),(676,422),(438,295)], fill=im.getpixel((490,276)))
    erase(im, [(202,548),(245,568),(235,581),(192,560)])
    erase(im, [(275,577),(309,597),(300,611),(265,591)])
    erase(im, [(439,677),(480,653),(486,671),(445,695)])
    im.resize((512,416), Image.Resampling.LANCZOS).save(out/'open-photo.png')
    closed = Image.open(src/'Game Boy Advance SP Tribal Edition - 1.JPG').convert('RGB'); closed.thumbnail((1000,1000))
    erase(closed, [(150,125),(790,125),(840,541),(140,541)], dark=98)
    # Rectify the photographed outside panel, then match Pearl Blue using its photographed luminance.
    panel = closed.transform((512,512), Image.Transform.QUAD, (195,133,145,528,826,538,778,128), Image.Resampling.BICUBIC)
    panel = panel.filter(ImageFilter.GaussianBlur(22))
    gray = panel.convert('L').point(lambda v:min(255,int(v*1.25))); panel = Image.merge('RGB', (gray.point(lambda v: int(v*.64)),gray.point(lambda v: min(255,int(v*.91))),gray.point(lambda v: min(255,int(v*1.02)))))
    panel.save(out/'closed-photo.png')

if __name__ == '__main__':
    prepare(Path(sys.argv[2]), Path(sys.argv[3]))
