"""discprep.py <photo> <out.png> <x0,y0,x1,y1> [--size 640] [--guide]: the disc's outline (bounding box of the ellipse the
angled photo shows it as) is resampled to a perfect circle filling a size x size square (white outside).  --guide draws
circles every 10 final px (of 160) so radii can be read."""
import sys
from PIL import Image, ImageDraw
a=sys.argv[1:]
src,out=a[0],a[1]; x0,y0,x1,y1=[float(v) for v in a[2].split(",")]
S=int(a[a.index("--size")+1]) if "--size" in a else 640
im=Image.open(src)
if im.mode=="RGBA":
    bg=Image.new("RGBA",im.size,(255,255,255,255)); bg.alpha_composite(im); im=bg
im=im.convert("RGB")
r=im.transform((S,S),Image.AFFINE,((x1-x0)/S,0,x0,0,(y1-y0)/S,y0),Image.BICUBIC,fillcolor=(255,255,255))
if "--guide" in a:
    d=ImageDraw.Draw(r); k=S/160
    for rad in range(10,81,10):
        d.ellipse([S/2-rad*k,S/2-rad*k,S/2+rad*k-1,S/2+rad*k-1],outline=(255,0,0) if rad%20==0 else (0,160,255))
r.save(out)
