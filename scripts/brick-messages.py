#!/usr/bin/env python3
"""Draws the on-screen messages of the Brick's standalone card (docs/TRIMUI-BRICK-STANDALONE.md): what the player sees when
something is missing or pocket-ui will not start. 1024x768, German first, English second, big and plain.

    PYTHONPATH=<folder with Pillow> python3 scripts/brick-messages.py deploy/brick/msg

The updater shows them with `pocket-fb --picture`; the "missing files" one is also written as a raw 32-bit frame
(`missing.raw`: pocket-fb itself may be what is missing). Run again only when a text changes; the PNGs are committed.
"""
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
FONT = ROOT / "assets/fonts/Barlow-Medium.ttf"
BG, FG, DIM, ACCENT = (16, 19, 26), (240, 242, 246), (150, 158, 172), (255, 190, 70)

MESSAGES = {
    "starting": (
        "pocket-ui startet", "pocket-ui is starting", ACCENT,
        ["Einen Moment bitte.", "", "Please wait a moment."],
    ),
    "missing": (
        "Dateien fehlen", "Files are missing", (255, 110, 100),
        ["Auf dieser Karte fehlt ein Teil von pocket-ui.",
         "Bitte den ganzen Ordner neu auf die Karte kopieren.",
         "Mehr steht in pocket-ui/logs/boot.log.",
         "",
         "A part of pocket-ui is missing on this card.",
         "Please copy the whole folder to the card again.",
         "",
         "Zurück zu NextUI: Brick ausschalten, normale Karte einlegen."],
    ),
    "crashed": (
        "pocket-ui startet nicht", "pocket-ui does not start", (255, 110, 100),
        ["pocket-ui hat sich mehrmals hintereinander beendet.",
         "Der Grund steht in pocket-ui/logs/boot.log auf der Karte.",
         "",
         "pocket-ui keeps stopping. The reason is in",
         "pocket-ui/logs/boot.log on the card.",
         "",
         "Zurück zu NextUI: Brick ausschalten (Power lange drücken),",
         "normale Karte einlegen.",
         "Der Brick schaltet sich in 10 Minuten selbst aus."],
    ),
}


def font(size):
    return ImageFont.truetype(str(FONT), size)


def draw(title, subtitle, colour, lines):
    img = Image.new("RGB", (1024, 768), BG)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, 1023, 10], fill=colour)
    d.text((64, 96), title, font=font(72), fill=FG)
    d.text((64, 186), subtitle, font=font(36), fill=DIM)
    y = 280
    for line in lines:
        assert font(32).getlength(line) <= 1024 - 128, f"line too wide: {line}"
        d.text((64, y), line, font=font(32), fill=FG)
        y += 42
    return img


def raw_bgra(img):
    r, g, b = img.split()
    return Image.merge("RGBA", (b, g, r, Image.new("L", img.size, 255))).tobytes()


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for name, (title, sub, colour, lines) in MESSAGES.items():
        img = draw(title, sub, colour, lines)
        img.save(out / f"{name}.png", optimize=True)
        if name == "missing":
            (out / "missing.raw").write_bytes(raw_bgra(img))
    print(f"brick-messages: {len(MESSAGES)} pictures in {out}")


if __name__ == "__main__":
    main()
