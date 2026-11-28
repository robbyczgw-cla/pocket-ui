#!/usr/bin/env python3
"""Cuts the two Chinese fonts of assets/fonts down to what pocket-ui paints (src/text_fallback.rs, `cjk`).

    python3 scripts/cjk-fonts.py fusion-pixel-12px-proportional-zh_hans.otf NotoSansSC-Regular.otf

Needs fonttools (`pip install fonttools`). In: fusion-pixel-12px-proportional-zh_hans.otf from
fusion-pixel-font-12px-proportional-otf-v2026.11.10.zip (https://github.com/TakWolf/fusion-pixel-font/releases/tag/2026.11.10)
and Sans/SubsetOTF/SC/NotoSansSC-Regular.otf of https://github.com/notofonts/noto-cjk. Out:
assets/fonts/FusionPixel12-zh_hans-CJK.otf and NotoSansSC-Regular-CJK.otf with the 6,763 hanzi of GB 2312 (the ones
EUC-CN can write: Simplified Chinese, the interface's and nearly every Chinese game name's), the 6,355 kanji of
JIS X 0208 (a Japanese game's name), the kana, CJK punctuation and the full-width forms; hinting and layout features
dropped, no glyph changed. Both files are read from the card (`text_fallback::shipped_dir`), not compiled in.

    python3 scripts/cjk-fonts.py --ja fusion-pixel-12px-proportional-ja.otf NotoSansJP-Regular.otf

The Japanese pair (the same release's ja file, noto-cjk Sans/SubsetOTF/JP/NotoSansJP-Regular.otf): Japanese letter forms
for the Japanese interface (`text_fallback::cjk`), cut to JIS X 0208's kanji, the kana, CJK punctuation and the
full-width forms: assets/fonts/FusionPixel12-ja-CJK.otf and NotoSansJP-Regular-CJK.otf.
"""
import os
import subprocess
import sys


def euc(codec, rows):
    s = set()
    for b1 in rows:
        for b2 in range(0xA1, 0xFF):
            try:
                c = bytes([b1, b2]).decode(codec)
            except UnicodeDecodeError:
                continue
            if len(c) == 1 and 0x4E00 <= ord(c) <= 0x9FFF:
                s.add(ord(c))
    return s


def unicodes():
    gb2312 = euc("gb2312", range(0xB0, 0xF8))
    jis = euc("euc_jp", range(0xB0, 0xF5))
    assert len(gb2312) == 6763 and len(jis) == 6355, (len(gb2312), len(jis))
    s = gb2312 | jis
    s |= set(range(0x3000, 0x3040))  # CJK punctuation: 、。「」『』【】〜
    s |= set(range(0x3041, 0x3097)) | set(range(0x3099, 0x3100))  # hiragana
    s |= set(range(0x30A0, 0x3100))  # katakana
    s |= set(range(0xFF01, 0xFF5F)) | set(range(0xFFE0, 0xFFE7))  # full-width forms: ，：！？（）￥
    return ",".join("U+%04X" % u for u in sorted(s))


def japanese():
    """JIS X 0208's kanji, the kana, CJK punctuation and the full-width forms: what a Japanese interface paints."""
    s = euc("euc_jp", range(0xB0, 0xF5))
    s |= set(range(0x3000, 0x3040)) | set(range(0x3041, 0x3097)) | set(range(0x3099, 0x3100)) | set(range(0x30A0, 0x3100))
    s |= set(range(0xFF01, 0xFF5F)) | set(range(0xFFE0, 0xFFE7))
    return ",".join("U+%04X" % u for u in sorted(s))


def main():
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "fonts")
    args = sys.argv[1:]
    ja = args[:1] == ["--ja"]
    srcs = args[1:3] if ja else args[0:2]
    outs = ["FusionPixel12-ja-CJK.otf", "NotoSansJP-Regular-CJK.otf"] if ja else ["FusionPixel12-zh_hans-CJK.otf", "NotoSansSC-Regular-CJK.otf"]
    for src, out in zip(srcs, outs):
        subprocess.run(["pyftsubset", src, "--unicodes=" + (japanese() if ja else unicodes()), "--no-hinting", "--layout-features=", "--name-IDs=*", "--notdef-outline", "--output-file=" + os.path.join(root, out)], check=True)


if __name__ == "__main__":
    main()
