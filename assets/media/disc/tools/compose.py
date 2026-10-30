"""compose.py <kind> <out.png>: builds the prepared photo of a disc's label side (640 px = the disc's diameter) for PixelLab:
a rectified photo (polarflat.py / discprep.py output in t/) with its hub radially remapped, a hub patch from another photo
(clear plastic ring), the system's rings painted as flat bands and the print annulus painted one flat base colour."""
import sys, numpy as np
from PIL import Image
S=640; K=S/160
yy,xx=np.mgrid[0:S,0:S]
c=(S-1)/2
R=np.hypot(xx-c,yy-c)/K            # radius in final px (disc edge = 80)
T=np.arctan2(yy-c,xx-c)
def load(n): return np.asarray(Image.open(f"t/{n}.png").convert("RGB")).astype(float)
def remap(img,src,tgt):
    rs=np.interp(R,tgt,src)/np.maximum(R,1e-6)
    sx=c+(xx-c)*rs; sy=c+(yy-c)*rs
    from scipy.ndimage import map_coordinates
    return np.stack([map_coordinates(img[:,:,i],[sy,sx],order=3,mode="nearest") for i in range(3)],2)
def band(img,r0,r1,col):
    img[(R>=r0)&(R<r1)]=col
def hub(img,srcimg,rh,tint=1.0):
    m=R<rh
    img[m]=srcimg[m]*tint
def hexc(h): return [int(h[i:i+2],16) for i in (1,3,5)]
kind,out=sys.argv[1],sys.argv[2]
cd,ps2,rvt=load("cd"),load("ps2"),load("rvt")
# the clear hub of the CD photo reflects a cyan backdrop: take most of the colour out of it
g=cd.mean(2,keepdims=True); hubz=(R<30)[:,:,None]
cd=np.where(hubz,0.7*g+0.3*cd,cd)
if kind=="cd":
    im=cd.copy(); band(im,29,77.5,hexc("#c9ced4"))
elif kind=="ps1":
    im=ps2.copy(); hub(im,cd,24,0.42); band(im,24,28,hexc("#2c2d32")); band(im,28,80.5,hexc("#151519")); band(im,79.3,80.5,hexc("#55565e"))
elif kind=="ps2":
    im=ps2.copy(); hub(im,cd,24,0.85); band(im,24,32,hexc("#101014")); band(im,32,77.8,hexc("#18181c")); band(im,77.8,78.4,hexc("#2a2a30")); band(im,78.4,80.5,hexc("#b9bec6"))
elif kind=="gamecube":
    im=remap(cd,[0,10,22,28,80],[0,15,30,34,80]); band(im,34,36,hexc("#8a8f98")); band(im,36,77.5,hexc("#d3d7dc")); band(im,78.5,80.5,hexc("#e4e7ea"))
elif kind=="wii":
    im=rvt.copy(); blue=((im[:,:,2]-im[:,:,0])>14)&(R<26); im[blue]=[236,237,238]; band(im,20,77.5,hexc("#f1f2f2"))
elif kind=="dreamcast":
    im=cd.copy(); band(im,29,33,hexc("#e8eaec")); band(im,33,35,hexc("#5a5f66")); band(im,35,77.5,hexc("#dfe2e5"))
elif kind=="saturn":
    im=cd.copy(); band(im,29,34,hexc("#3a4663")); band(im,34,36,hexc("#a3a8b0")); band(im,36,77.5,hexc("#cfd3d8")); band(im,78.5,80.5,hexc("#2b3350"))
elif kind=="xbox":
    im=ps2.copy(); hub(im,cd,30,0.9); band(im,17.5,25.5,hexc("#37821f")); band(im,25.5,30,hexc("#0e0e10")); band(im,30,77.8,hexc("#141416")); band(im,77.8,78.4,hexc("#2a2a30")); band(im,78.4,80.5,hexc("#8c9298"))
outcol={"ps1":"#151519","ps2":"#b9bec6","xbox":"#8c9298"}.get(kind,"#ffffff")
im[R>80.2]=hexc(outcol)
Image.fromarray(np.clip(im,0,255).astype(np.uint8)).save(out)
