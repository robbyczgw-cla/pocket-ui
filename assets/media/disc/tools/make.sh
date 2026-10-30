#!/bin/sh
# Rebuild a disc template (run from a scratch dir with dl/ = the four Commons originals at 640-1280 px, t/ p/ pl/ created).
# 1. straighten the photos to a perfect circle (disc diameter = 640 px = 160 px at 4x)
#    python3 discprep.py dl/cd.jpg  t/cd.png  7,0,952,960              # Audio Compact Disc.png
#    python3 discprep.py dl/rvt.jpg t/rvt.png 10,15,948,957            # RVT-R disc label side (white background).png
#    python3 polarflat.py dl/ps2.jpg t/ps2.png 457,398 dark 75         # Sony PlayStation 2 HDD Utility Disc 20071206.jpg
# 2. compose.py <kind> p/<kind>.png     (bands, hub, flat label colour; UMD: umd_prep.py)
# 3. pixellab_i2p.py p/<kind>.png pl/<kind>_x.png 160 160 [--strength 250 --fixer]   (UMD 156 160)
# 4. finish.py <kind> pl/<kind>_x.png <kind>.png   (UMD: umd_finish.py)  -> exact circle, hole, #FF00FF print area
# 5. cmp.py <kind> <photo> <kind>.png compare-<kind>.jpg
