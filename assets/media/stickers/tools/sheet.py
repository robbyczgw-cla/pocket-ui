from PIL import Image
import json
m=json.load(open('work/meta.json')); m['hundred']['plain_rect']=[10,13,16,10]
for n in m:
    x,y,w,h=m[n]['plain_rect'];im=Image.open(n+'.png')
    print(n,set(im.getpixel((i,j)) for i in range(x,x+w) for j in range(y,y+h)))
json.dump(m,open('work/meta.json','w'))
names=['completed','hundred','favourite','playing','goty']
ims=[Image.open(n+'.png').convert('RGBA') for n in names]
s=6;W=sum(i.width*s+16 for i in ims)+16;H=max(i.height for i in ims)*s+32
out=Image.new('RGB',(W,H*2))
for bi,bg in enumerate([(0x1f,0x1f,0x22),(0xec,0xe9,0xe1)]):
    o=Image.new('RGBA',(W,H),bg+(255,));x=16
    for i in ims:
        j=i.resize((i.width*s,i.height*s),Image.NEAREST);o.alpha_composite(j,(x,16));x+=j.width+16
    out.paste(o.convert('RGB'),(0,bi*H))
out.save('sheet-preview.jpg',quality=75)
