"""finish.py: alpha clean, crop, palette<=48 (no dither), writes ../<name>.png. Compositions for hundred and goty."""
import json
from PIL import Image, ImageDraw
def clean(im):
    im=im.convert('RGBA'); px=im.load()
    for y in range(im.height):
        for x in range(im.width):
            r,g,b,a=px[x,y]; px[x,y]=(r,g,b,255) if a>=200 else (0,0,0,0)
    return im.crop(im.getbbox())
def pal(im,n=48):
    a=im.split()[3]; q=im.convert('RGB').quantize(n,method=Image.MEDIANCUT,dither=Image.NONE).convert('RGB').convert('RGBA')
    q.putalpha(a); return q
def save(im,name):
    im=pal(clean(im)); im.save('../'+name+'.png'); print(name,im.size,len(set(p for p in im.getdata() if p[3])))
meta={}
for n,s in [('completed','completed_a'),('favourite','favourite_b'),('playing','playing_a')]:
    save(Image.open(s+'.png'),n)
# hundred: drop ribbons, flatten centre disc
h=clean(Image.open('hundred_a.png')); 
h=h.crop((0,0,h.width,36)); W,H=h.size
cx,cy=(W-1)/2,16.5-0; 
d=ImageDraw.Draw(h); 
c=h.getpixel((int(cx),int(cy)))
# flat centre colour: median-ish of centre ring, use a calm gold
FL=(238,190,60,255)
for y in range(H):
    for x in range(W):
        if (x-cx)**2+(y-cy)**2<=10.5**2: h.putpixel((x,y),FL)
import math
cx0,cy0=(W-1)/2,16.5
for y in range(H):
    for x in range(W):
        if (x-cx0)**2+(y-cy0)**2>17.6**2: h.putpixel((x,y),(0,0,0,0))
h=clean(h); W,H=h.size
o2=Image.new('RGBA',(W+2,H+2),(0,0,0,0)); o2.paste(h,(1,1))
src2=o2.copy(); OL=(122,72,16,255)
for y in range(o2.height):
    for x in range(o2.width):
        if src2.getpixel((x,y))[3]==0 and any(0<=x+dx<o2.width and 0<=y+dy<o2.height and src2.getpixel((x+dx,y+dy))[3] for dx,dy in((1,0),(-1,0),(0,1),(0,-1))): o2.putpixel((x,y),OL)
h=o2; cx,cy=(h.width-1)/2,(h.height-1)/2
for y in range(h.height):
    for x in range(h.width):
        if (x-cx)**2+(y-cy)**2<=10.5**2: h.putpixel((x,y),FL)
save(h,'hundred'); hw,hh=Image.open('../hundred.png').size
meta['hundred']={'size':[hw,hh],'plain_rect':[round(cx-8+0.5),round(cy-5+0.5),16,10],'flat_colour':'#EEBE3C'}
# goty composition
src=Image.open('goty_a.png').convert('RGBA'); gp=lambda p:p[3]>0 and p[1]>p[0]+5
cells=[(x,y) for x in range(24) for y in range(2,30) if gp(src.getpixel((x,y)))]
x0=min(c[0] for c in cells);x1=max(c[0] for c in cells);y0=min(c[1] for c in cells);y1=max(c[1] for c in cells)
wr=src.crop((x0,y0,x1+1,y1+1))
W,H=56,28
o=Image.new('RGBA',(W,H),(0,0,0,0));d=ImageDraw.Draw(o)
OUT=(74,48,24,255);WH=(244,240,228,255);GOLD=(233,190,105,255);DK=(196,146,62,255);LT=(250,226,150,255)
d.rounded_rectangle((0,0,W-1,H-1),3,fill=OUT)
d.rounded_rectangle((1,1,W-2,H-2),2,fill=WH)
d.rounded_rectangle((2,2,W-3,H-3),1,fill=DK)
d.rectangle((3,3,W-4,H-4),fill=GOLD)
d.line((4,3,W-5,3),fill=LT)
ox=4;oy=(H-wr.height)//2
mw=wr.transpose(Image.FLIP_LEFT_RIGHT)
for y in range(wr.height):
    for x in range(wr.width):
        p=mw.getpixel((x,y))
        if gp(p): o.putpixel((ox+wr.width+x,oy+y),(p[0],p[1],p[2],255))
for y in range(wr.height):
    for x in range(wr.width):
        p=wr.getpixel((x,y))
        if gp(p): o.putpixel((ox+x,oy+y),(p[0],p[1],p[2],255))
o.save('goty_raw.png'); save(o,'goty')
meta['goty']={'size':[W,H],'plain_rect':[26,6,26,16],'flat_colour':'#E9BE69','wreath_rect':[ox,oy,wr.width,wr.height]}
json.dump(meta,open('meta.json','w'))
