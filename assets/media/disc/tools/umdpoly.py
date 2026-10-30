import sys, math
from PIL import Image, ImageDraw
# outline of the UMD shell on dl/umdf.jpg (1280x960): arch on top, straight sides, rounded bottom corners
def outline():
    pts=[(213,430),(222,360),(245,290),(285,225),(340,165),(420,115),(520,80),(640,65),(760,75),(860,105),(940,150),(1000,215),(1040,290),(1058,360),(1062,430)]
    # right side down to the bottom corner
    pts+= [(1062,700),(1056,770)]
    for i in range(0,91,6):
        t=math.radians(i); pts.append((990+66*math.cos(t)*0.9+0,   830+66*math.sin(t)*0.95))
    pts+= [(930,898),(340,898)]
    for i in range(90,181,6):
        t=math.radians(i); pts.append((286+66*math.cos(t)*0.9, 830+66*math.sin(t)*0.95))
    pts+= [(214,770),(212,700)]
    return pts
if __name__=="__main__":
    im=Image.open("dl/umdf.jpg").convert("RGB"); d=ImageDraw.Draw(im)
    d.line(outline()+[outline()[0]],fill=(255,0,0),width=3)
    im.resize((800,600)).save("t/umdpoly.jpg",quality=75)
