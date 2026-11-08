"""Register the PixelLab SP photographs as a cartridge rig with a separately hinged lid.
sp_finish.py CONVERTED_DIR REPO; geometry traces the photographed outlines, not replacement drawings.
"""
from pathlib import Path
import sys, json
from collections import deque
import numpy as np
from PIL import Image, ImageDraw, PngImagePlugin

src, repo = map(Path, sys.argv[1:])
out = repo/'assets/media/rig/gba-sp'; out.mkdir(parents=True, exist_ok=True)
(repo/'assets/systems').mkdir(parents=True, exist_ok=True)
im = Image.open(src/'open.png').convert('RGBA')
px=im.load(); q=deque([(x,0) for x in range(im.width)]+[(x,im.height-1) for x in range(im.width)]+[(0,y) for y in range(im.height)]+[(im.width-1,y) for y in range(im.height)])
seen=set()
while q:
    x,y=q.popleft()
    if (x,y) in seen or not (0<=x<im.width and 0<=y<im.height):continue
    seen.add((x,y));r,g,b,a=px[x,y]
    if not a or (min(r,g,b)>=235 and max(r,g,b)-min(r,g,b)<24):
        px[x,y]=(0,0,0,0);q.extend([(x-1,y),(x+1,y),(x,y-1),(x,y+1)])
# Wordmark retouch with the photographed dark bezel; all shell pixels come from PixelLab's photograph.
ImageDraw.Draw(im).polygon([(160,88),(222,121),(215,128),(152,95)],fill=im.getpixel((154,94)))
# The source photograph has a flush cartridge already inserted. Replace only
# that photographed face with PixelLab's masked empty-port edit, preserving the rim.
port=Image.new('L',im.size)
ImageDraw.Draw(port).polygon([(17,185),(105,232),(106,247),(18,204)],fill=255)
edited=Image.new('RGBA',im.size)
edited.paste(Image.open(src/'empty-slot-tight.png').convert('RGBA'),(16,180))
im.paste(edited,(0,0),port)
meta=PngImagePlugin.PngInfo()
meta.add_text('Author','Evan-Amos (open SP); Benoît Prieur (outside lid)')
meta.add_text('Source','https://commons.wikimedia.org/wiki/File:Gameboy-Advance-SP-Mk2.jpg ; https://commons.wikimedia.org/wiki/File:Game_Boy_Advance_SP_Tribal_Edition_-_1.JPG')
meta.add_text('License','CC0 1.0; original open photograph Public domain (PD-self), outside lid CC0 1.0')
meta.add_text('LicenseURL','https://creativecommons.org/publicdomain/zero/1.0/')
meta.add_text('Modification','modified via PixelLab; inserted cartridge removed by masked inpainting, lettering retouched, cut out, lid registered, outside lid recoloured')
im.save(repo/'assets/systems/gba-sp.png',pnginfo=meta)
size=(320,300)
full=Image.new('RGBA',size);full.paste(im,(0,0))
mask=Image.new('L',size);ImageDraw.Draw(mask).polygon([(0,0),(320,0),(320,201),(0,46)],fill=255)
lid=Image.new('RGBA',size);lid.paste(full,(0,0),mask)
base=full.copy();base.paste((0,0,0,0),(0,0,320,300),mask);base.save(out/'base.png',pnginfo=meta)
# Register the photographed mouth as an aperture: the dark recess is behind the
# moving cartridge; the photographed shell and rim occlude it in front.
over=base.copy();op=over.load();bp=base.load();pp=port.load()
for y in range(im.height):
    for x in range(im.width):
        if pp[x,y] and max(bp[x,y][:3])<=38:
            op[x,y]=(0,0,0,0)
over.save(out/'over.png',pnginfo=meta)
h=np.array([117.,106.]);a=np.array([105.,51.]);f=np.array([-108.,77.]);opened=np.array([86.,-98.])
theta=np.deg2rad(135);up=(opened-f*np.cos(theta))/np.sin(theta)
mat=np.column_stack((a,opened))@np.linalg.inv(np.column_stack((a,f)))
offset=h-mat@h
under=lid.transform(size,Image.Transform.AFFINE,tuple([*mat[0],offset[0],*mat[1],offset[1]]),Image.Resampling.NEAREST)
under.save(out/'under.png',pnginfo=meta)
outside=Image.open(src/'closed.png').convert('RGBA');inv=np.linalg.inv(np.column_stack((a,f)))
# The panel photo's x goes across the hinge; y goes from hinge to the front.
mapping=np.diag([outside.width,outside.height])@inv;off=-mapping@h
top=outside.transform(size,Image.Transform.AFFINE,tuple([*mapping[0],off[0],*mapping[1],off[1]]),Image.Resampling.NEAREST)
top.save(out/'top.png',pnginfo=meta)
rig=dict(seat=dict(at=[78,204],u=[-38,-19],v=[23,-16.5],hover=[12,252],slot=[[10,187],[112,247]]),behind=False,label_away=True,bottom_slot=True,
         clamshell=dict(at=[0,0],hinge=[[117,106],[222,157]],front=[-108,77],up=up.tolist(),open_deg=135),led=dict(at=[211,162],color=0x78dd71))
(out/'rig.json').write_text(json.dumps(rig,indent=2)+'\n')
