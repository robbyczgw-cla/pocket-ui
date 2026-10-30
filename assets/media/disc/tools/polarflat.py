"""polarflat.py <photo> <out.png> <cx,cy> <mode:dark|nonwhite|alpha|light> <thr> [--size 640] [--seedr R] [--edge-scale 1.0]
Rectifies an angled photo of a disc to a perfect circle (disc radius = size/2): the hub centre (cx,cy) and, for every angle,
the disc's edge radius found on the photo with a simple colour mask are used to resample radially (a star-shaped warp, so
the perspective of an angled photo and a hub that sits off the outline's centre are both undone)."""
import sys, numpy as np
from PIL import Image
from scipy.ndimage import map_coordinates, median_filter, uniform_filter1d
a=sys.argv[1:]
src,out=a[0],a[1]; cx,cy=[float(v) for v in a[2].split(",")]; mode,thr=a[3],int(a[4])
S=int(a[a.index("--size")+1]) if "--size" in a else 640
im=Image.open(src)
alpha=np.asarray(im)[:,:,3] if im.mode=="RGBA" else None
if im.mode=="RGBA":
    bg=Image.new("RGBA",im.size,(255,255,255,255)); bg.alpha_composite(im); im=bg
rgb=np.asarray(im.convert("RGB")).astype(float)
if mode=="dark": m=rgb.max(2)<thr
elif mode=="nonwhite": m=rgb.min(2)<255-thr
elif mode=="light": m=rgb.min(2)>thr
else: m=alpha>thr
H,W=m.shape; N=720
th=np.linspace(0,2*np.pi,N,endpoint=False)
Rs=np.zeros(N)
for i,t in enumerate(th):
    rr=np.arange(0,int(np.hypot(W,H)))
    xs=cx+rr*np.cos(t); ys=cy+rr*np.sin(t)
    ok=(xs>=0)&(xs<W-1)&(ys>=0)&(ys<H-1)
    inside=np.zeros(len(rr),bool); inside[ok]=m[ys[ok].astype(int),xs[ok].astype(int)]
    # edge: last r such that a solid run of >= 85% of rr[:r] is inside
    cs=np.cumsum(inside&(rr>=130))/np.maximum(rr-129,1)
    good=np.where((cs>0.88)&(rr>20))[0]
    Rs[i]=rr[good.max()] if len(good) else np.nan
if "--ellipse" in a:
    x0,y0,x1,y1=[float(v) for v in a[a.index("--ellipse")+1].split(",")]
    ex,ey,ea,eb=(x0+x1)/2,(y0+y1)/2,(x1-x0)/2,(y1-y0)/2
    ux,uy=cx-ex,cy-ey
    for i,t in enumerate(th):
        dx_,dy_=np.cos(t)/ea,np.sin(t)/eb
        A=dx_**2+dy_**2; B=2*(ux/ea*dx_+uy/eb*dy_); C=(ux/ea)**2+(uy/eb)**2-1
        Rs[i]=(-B+np.sqrt(B*B-4*A*C))/(2*A)
med=np.nanmedian(Rs)
Rs=np.where(np.isnan(Rs)|(abs(Rs-med)>0.25*med),med,Rs)
Rs=median_filter(np.concatenate([Rs,Rs,Rs]),size=15)[N:2*N]
Rs=uniform_filter1d(np.concatenate([Rs,Rs,Rs]),9)[N:2*N]
es=float(a[a.index("--edge-scale")+1]) if "--edge-scale" in a else 1.0
Rs*=es
yy,xx=np.mgrid[0:S,0:S]
dx,dy=xx-(S-1)/2,yy-(S-1)/2
r=np.hypot(dx,dy)/(S/2); t=np.arctan2(dy,dx)%(2*np.pi)
Ri=np.interp(t,np.append(th,2*np.pi),np.append(Rs,Rs[0]))
sx=cx+np.cos(t)*r*Ri; sy=cy+np.sin(t)*r*Ri
outc=[map_coordinates(rgb[:,:,c],[sy,sx],order=3,mode="nearest") for c in range(3)]
o=np.stack(outc,2); o[r>1.0]=255
Image.fromarray(np.clip(o,0,255).astype(np.uint8)).save(out)
print(out,"edge radius median",med,"min/max",Rs.min(),Rs.max())
