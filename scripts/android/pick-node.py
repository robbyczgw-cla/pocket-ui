#!/usr/bin/env python3
"""Tap a system picker node derived from its current UIAutomator bounds, never screenshot coordinates."""
import re, subprocess, sys, xml.etree.ElementTree as ET
adb, serial, text = sys.argv[1:]
xml = subprocess.check_output([adb, '-s', serial, 'exec-out', 'uiautomator', 'dump', '/dev/tty'], text=True)
xml = xml[xml.index('<?xml'):]
xml = xml[:xml.index('</hierarchy>') + len('</hierarchy>')]
for node in reversed(list(ET.fromstring(xml).iter('node'))):
    if text in (node.get('text'), node.get('content-desc')) and node.get('enabled') == 'true':
        x1, y1, x2, y2 = map(int, re.findall(r'\d+', node.get('bounds')))
        subprocess.check_call([adb, '-s', serial, 'shell', 'input', 'tap', str((x1+x2)//2), str((y1+y2)//2)])
        break
else:
    sys.exit('No enabled picker node: ' + text)
