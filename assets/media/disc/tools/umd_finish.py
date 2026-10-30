"""umd_finish.py <pixellab.png> <out.png>: UMD shell: alpha 0/255 from the shell outline (p/umd_geom.json), the label circle one flat #FF00FF."""
import sys, json, numpy as np
from PIL import Image, ImageDraw
g=json.load(open("p/umd_geom.json"))
im=Image.open(sys.argv[1]).convert("RGBA").resize((156,160),Image.NEAREST); a=np.asarray(im).copy()
m=Image.new("L",(156*8,160*8),0); ImageDraw.Draw(m).polygon([(x*8,y*8) for x,y in g["poly"]],fill=255)
m=np.asarray(m.resize((156,160),Image.BOX))
a[...,3]=np.where(m>=128,255,0)
cx,cy,r=g["circle"]; r-=1.5
yy,xx=np.mgrid[0:160,0:156]; d=np.hypot(xx+0.5-cx,yy+0.5-cy)
a[(d<=r)&(a[...,3]==255)]=[255,0,255,255]
Image.fromarray(a).save(sys.argv[2]); print("label circle",cx,cy,r,"magenta",int(((a[...,0]==255)&(a[...,1]==0)&(a[...,2]==255)).sum()))
