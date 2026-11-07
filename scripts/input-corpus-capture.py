#!/usr/bin/env python3
"""Read evdev identity/capabilities without grabs, input injection or device changes."""
import os, pathlib, fcntl, struct, json
out=[]
for p in sorted(pathlib.Path('/sys/class/input').glob('event*')):
    d=p/'device'
    dev={'name':p.name}
    try:
        dev={'name':(d/'name').read_text().strip(), 'id':{k:int((d/'id'/k).read_text(),16) for k in ['bustype','vendor','product','version']},'bitmaps':{k:(d/'capabilities'/k).read_text().strip() for k in ['ev','key','abs','sw','rel','ff']},'abs':[]}
        node='/dev/input/'+p.name
        if not pathlib.Path(node).exists():
            major,minor=map(int,(p/'dev').read_text().strip().split(':'))
            for proc in pathlib.Path('/proc').glob('[0-9]*/comm'):
                try:
                    if proc.read_text().strip() != 'inputplumber': continue
                    for fd in (proc.parent/'fd').iterdir():
                        try:
                            if fd.stat().st_rdev == os.makedev(major,minor): node=str(fd); break
                        except OSError: pass
                except OSError: pass
        with open(node,'rb',buffering=0) as f:
            # EVIOCGBIT(EV_ABS,8), EVIOCGABS; no event reads and no EVIOCGRAB.
            bits=bytearray(8); fcntl.ioctl(f,0x80084523,bits,True)
            mask=int.from_bytes(bits,'little')
            for axis in range(64):
                if mask & 1<<axis:
                    buf=bytearray(24); fcntl.ioctl(f,0x80184540+axis,buf,True)
                    val,lo,hi,fuzz,flat,res=struct.unpack('iiiiii',buf)
                    dev['abs'].append([axis,lo,hi,fuzz,flat,res])
        out.append(dev)
    except Exception as e:
        dev['capture_error']=str(e); out.append(dev)
print(json.dumps(out,indent=2))
