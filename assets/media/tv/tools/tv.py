from PIL import Image
import json
from collections import deque
im=Image.open('work/tv_c.png').convert('RGBA');W,H=im.size
px=im.load()
for y in range(W*0+H):
    for x in range(W):
        r,g,b,a=px[x,y]; px[x,y]=(r,g,b,255) if a>=200 else (0,0,0,0)
for y in range(57,61):
    for x in range(72,81): px[x,y]=(165,87,63,255)
isg=lambda p:p[3] and p[1]>p[0]+15
seen={(40,30)};q=deque(seen)
assert isg(px[40,30])
while q:
    x,y=q.popleft()
    for dx,dy in((1,0),(-1,0),(0,1),(0,-1)):
        n=(x+dx,y+dy)
        if 0<=n[0]<W and 0<=n[1]<H and n not in seen and isg(px[n]): seen.add(n);q.append(n)
for x,y in seen: px[x,y]=(255,0,255,255)
xs=[p[0] for p in seen];ys=[p[1] for p in seen]
rect=[min(xs),min(ys),max(xs)-min(xs)+1,max(ys)-min(ys)+1]
# reduce non-magenta palette to <=47 colours
a=im.split()[3]
mg=[(x,y) for x,y in seen]
base=im.convert('RGB').quantize(47,method=Image.MEDIANCUT,dither=Image.NONE).convert('RGB')
bp=base.load()
for x,y in mg: bp[x,y]=(255,0,255)
base=base.convert('RGBA');base.putalpha(a);base.save('tv.png')
cnt=sum(1 for p in base.getdata() if p[:3]==(255,0,255) and p[3]);print(base.size,rect,len(seen),cnt,len(set(p for p in base.getdata() if p[3])))
json.dump({"screen":rect,"size":[W,H],"notes":"screen = bounding rect of the #FF00FF pixels; the magenta shape is the curved glass (rounded corners), so the corners of the rect are not magenta. Front view slightly from above, beige body, knobs + grille on the right, speaker grille bottom-left; no antenna, no logo."},open('tv.json','w'),indent=1)
bg=Image.new('RGBA',(W*4,H*4),(0x3a,0x24,0x18,255))
bg.alpha_composite(base.resize((W*4,H*4),Image.NEAREST));bg.convert('RGB').save('tv-preview.jpg',quality=80)
