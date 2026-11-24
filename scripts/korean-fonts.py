#!/usr/bin/env python3
"""Cuts the two Korean fonts of assets/fonts down to what pocket-ui paints (src/text_fallback.rs).

    python3 scripts/korean-fonts.py Galmuri11.ttf NotoSansKR-Regular.otf

Needs fonttools (`pip install fonttools`). In: Galmuri11.ttf from Galmuri-v2.40.4.zip
(https://github.com/quiple/galmuri/releases/tag/v2.40.4) and Sans/SubsetOTF/KR/NotoSansKR-Regular.otf of
https://github.com/notofonts/noto-cjk. Out: assets/fonts/Galmuri11-KSX1001.ttf and NotoSansKR-Regular-KSX1001.otf with
the 2,350 hangul syllables of KS X 1001 (the ones EUC-KR can write), the compatibility jamo, a few CJK punctuation marks
and the two ideographs 国 國 (the fit's measure); hinting and layout features dropped, no glyph changed.
"""
import os
import subprocess
import sys


def unicodes():
    s = set()
    for b1 in range(0xB0, 0xC9):
        for b2 in range(0xA1, 0xFF):
            try:
                c = bytes([b1, b2]).decode("euc-kr")
            except UnicodeDecodeError:
                continue
            if "가" <= c <= "힣":
                s.add(ord(c))
    assert len(s) == 2350, len(s)
    s |= set(range(0x3131, 0x318F))  # compatibility jamo
    s |= set(range(0x3000, 0x3004)) | set(range(0x3008, 0x3012)) | {0x301C}  # CJK punctuation, brackets
    s |= {0x56FD, 0x570B}  # 国 國
    return ",".join("U+%04X" % u for u in sorted(s))


def main():
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "fonts")
    for src, out in zip(sys.argv[1:3], ["Galmuri11-KSX1001.ttf", "NotoSansKR-Regular-KSX1001.otf"]):
        subprocess.run(["pyftsubset", src, "--unicodes=" + unicodes(), "--no-hinting", "--layout-features=", "--name-IDs=*", "--notdef-outline", "--output-file=" + os.path.join(root, out)], check=True)


if __name__ == "__main__":
    main()
