"""cmp.py <kind> <photo.jpg|png> <template.png> <out.jpg>: photo | template at 300 px (on dark grey) | 76 px, q75."""
import sys
from PIL import Image
k,ph,tp,out=sys.argv[1:5]
p=Image.open(ph).convert("RGB"); p.thumbnail((420,420))
t=Image.open(tp).convert("RGBA")
def on(bg,size):
    b=Image.new("RGBA",(size+20,size+20),bg); b.alpha_composite(t.resize((size,size),Image.NEAREST),(10,10)); return b.convert("RGB")
t3=on((70,70,78,255),300); t1=on((70,70,78,255),76)
W=p.width+t3.width+t1.width; H=max(p.height,t3.height)
S=Image.new("RGB",(W,H),(255,255,255)); S.paste(p,(0,0)); S.paste(t3,(p.width,0)); S.paste(t1,(p.width+t3.width,0))
S.save(out,quality=75)
