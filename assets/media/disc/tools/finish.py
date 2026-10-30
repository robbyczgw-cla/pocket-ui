"""finish.py <kind> <pixellab.png> <out.png>: exact circle (alpha 0/255), centre hole transparent, print annulus one flat #FF00FF.
Prints the json fields hole/print radii."""
import sys, json, numpy as np
from PIL import Image
SPEC={"cd":(10,29,77.5),"ps1":(10,28,77.5),"ps2":(10,32,77.8),"gamecube":(15,36,77.5),"wii":(10,25,77.5),
      "dreamcast":(10,35,77.5),"saturn":(10,36,77.5),"xbox":(10,30,77.8)}
BANDS={"ps2":[(24,32,"#101014")],"gamecube":[(34,36,"#8a8f98")],"dreamcast":[(29,33,"#e8eaec"),(33,35,"#5a5f66")],
       "saturn":[(29,34,"#3a4663"),(34,36,"#a3a8b0")],"xbox":[(17.5,25.5,"#37821f"),(25.5,30,"#0e0e10")]}
kind,src,out=sys.argv[1:4]
hole,pin,pout=SPEC[kind]
im=Image.open(src).convert("RGBA").resize((160,160),Image.NEAREST); a=np.asarray(im).copy()
yy,xx=np.mgrid[0:160,0:160]; r=np.hypot(xx+0.5-80,yy+0.5-80)
a[...,3]=np.where((r<=80)&(r>hole),255,0)
a[(r>=pin)&(r<=pout)&(a[...,3]==255)]=[255,0,255,255]
for r0,r1,h in BANDS.get(kind,[]):
    a[(r>=r0)&(r<r1)&(r>hole)]=[int(h[i:i+2],16) for i in (1,3,5)]+[255]
# drop the near-white backdrop colour that PixelLab leaves on the very edge of dark discs
Image.fromarray(a).save(out)
print(kind,"hole",hole,"print",pin,pout,"magenta px",int(((a[...,0]==255)&(a[...,1]==0)&(a[...,2]==255)).sum()))
