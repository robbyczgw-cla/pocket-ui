"""umd_prep.py: UMD shell front from dl/umdf.jpg (Pursuit Force UMD, public domain) -> p/umd.png (624x640 = 156x160 px at 4x)
and p/umd_geom.json (shell outline polygon and the label circle in final px). The PSP lettering and the controller symbol on the
shell and the UMD wordmark / print on the label are painted over; the label (circle inside the clear window) is one flat blue."""
import json, numpy as np
from PIL import Image, ImageDraw
from umdpoly import outline
X0,Y0,X1,Y1=212,62,1062,898
W,H,K=156,160,4
sx,sy=(X1-X0)/(W*K),(Y1-Y0)/(H*K)
im=Image.open("dl/umdf.jpg").convert("RGB")
d=ImageDraw.Draw(im)
shell=(236,236,240)
# PSP lettering (lower left) and controller symbol (lower right) -> shell colour
d.rounded_rectangle([225,765,380,815],radius=10,fill=shell)
d.rounded_rectangle([935,770,1030,830],radius=10,fill=shell)
# label: circle centre (639,470) r 371 -> flat blue, with a thin darker rim
cx,cy,r=639,470,371
d.ellipse([cx-r,cy-r,cx+r,cy+r],fill=(44,82,176))
out=im.transform((W*K,H*K),Image.AFFINE,(sx,0,X0,0,sy,Y0),Image.BICUBIC)
poly=[((x-X0)/sx/K,(y-Y0)/sy/K) for x,y in outline()]
json.dump({"poly":poly,"circle":[(cx-X0)/sx/K,(cy-Y0)/sy/K,r/sx/K]},open("p/umd_geom.json","w"))
# the photo's shell is shaded by a strong vignette: keep the photo only inside the window ring (r <= 408 photo px) and paint
# the shell as light plastic with a soft light-to-dark gradient and a darker outline
cxf,cyf,rf=(cx-X0)/sx,(cy-Y0)/sy,408/sx
yy,xx=np.mgrid[0:H*K,0:W*K]
g=np.clip((xx/(W*K)*0.45+yy/(H*K)*0.55),0,1)
shellimg=np.stack([238-30*g,239-30*g,243-28*g],2)
a=np.asarray(out).astype(float)
ring=(((xx-cxf)/(rf)) ** 2 + ((yy-cyf)/(408/sy)) ** 2)<=1
a=np.where(ring[:,:,None],a,shellimg)
out=Image.fromarray(a.astype(np.uint8))
m=Image.new("L",(W*K,H*K),0); ImageDraw.Draw(m).polygon([(x*K,y*K) for x,y in poly],fill=255)
dd=ImageDraw.Draw(out)
bg=Image.new("RGB",out.size,(236,236,240)); out=Image.composite(out,bg,m)
dd=ImageDraw.Draw(out); dd.line([(x*K,y*K) for x,y in poly]+[(poly[0][0]*K,poly[0][1]*K)],fill=(150,153,162),width=5)
out.save("p/umd.png")
print(poly[0],poly[len(poly)//2],json.load(open("p/umd_geom.json"))["circle"])
