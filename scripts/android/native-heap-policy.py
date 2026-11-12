#!/usr/bin/env python3
"""Check the compiled application policy from `aapt dump xmltree APK AndroidManifest.xml` on stdin."""
import re
import sys

def compatible(tree):
    application = False
    flags = []
    for line in tree.splitlines():
        value = line.strip()
        if re.match(r'E: application(?:\s|$)', value):
            application = True
            continue
        if application:
            if value.startswith('E:'):
                break
            match = re.match(r'A: android:allowNativeHeapPointerTagging(?:\(0x[0-9a-f]+\))?=\(type 0x12\)(0x[0-9a-f]+)$', value)
            if match:
                flags.append(int(match[1], 16))
    return flags == [0]

if __name__ == '__main__':
    if compatible(sys.stdin.read()):
        print('ok: application heap pointer tagging disabled for native fast-memory cores')
    else:
        print('FAIL: application must explicitly disable native heap pointer tagging for fast-memory cores')
        sys.exit(1)
