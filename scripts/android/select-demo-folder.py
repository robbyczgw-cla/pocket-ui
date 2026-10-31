#!/usr/bin/env python3
"""Drive API-35 DocumentsUI into our demo subtree using live UI nodes and bounded retries."""
import re, subprocess, sys, time, xml.etree.ElementTree as ET
from pathlib import Path
adb, serial, output = sys.argv[1:]
model = subprocess.check_output([adb, '-s', serial, 'shell', 'getprop', 'ro.product.model'], text=True).strip()
for step in range(16):
    raw = subprocess.check_output([adb, '-s', serial, 'exec-out', 'uiautomator', 'dump', '/dev/tty'], text=True)
    (Path(output) / f'picker-{step:02}.xml').write_text(raw)
    xml = raw[raw.index('<?xml'):raw.index('</hierarchy>') + 12]
    nodes = list(ET.fromstring(xml).iter('node'))
    def find(text, resource=None):
        return next((n for n in reversed(nodes) if text in (n.get('text'), n.get('content-desc')) and n.get('enabled') == 'true'
                     and (resource is None or n.get('resource-id', '').endswith(resource))), None)
    got_it = find('Got it')
    choose = find('Choose your ROM folder')
    allow = find('ALLOW')
    use = find('USE THIS FOLDER')
    if got_it is not None:
        target = got_it
    elif choose is not None:
        target = choose
    elif allow is not None:
        target = allow
    elif use is not None and any(n.get('text') == 'PocketMvp' for n in nodes):
        target = use
    else:
        target = find('PocketMvp', 'android:id/title')
        if target is None: target = find('Download', 'android:id/title')
        if target is None: target = find(model, 'android:id/title')
        if target is None: target = find('Show roots')
    if target is None and any(n.get('package') == 'dev.pocket.spike' for n in nodes):
        subprocess.check_call([adb, '-s', serial, 'shell', 'input', 'keyevent', '34'])
    if target is not None:
        x1, y1, x2, y2 = map(int, re.findall(r'\d+', target.get('bounds')))
        subprocess.check_call([adb, '-s', serial, 'shell', 'input', 'tap', str((x1+x2)//2), str((y1+y2)//2)])
        if allow is not None: break
    time.sleep(1)
else:
    sys.exit('Could not select PocketMvp through the real SAF picker; see picker XML evidence')
