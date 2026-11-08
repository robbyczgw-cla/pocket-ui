#!/usr/bin/env python3
"""Draws the on-screen messages of the TrimUI standalone card (docs/TRIMUI-BRICK-STANDALONE.md): what the player sees when
something is missing or pocket-ui will not start. English first, German second (every line in both), big and plain. Two sizes, because pocket-fb
shows a picture top left without scaling: <name>.png is 1024x768 (Brick), <name>-wide.png is 1280x720 (Smart Pro); start.sh picks
by the screen. No other frontend's name goes into a message (scripts/test-brick-card.sh scans this file).

    PYTHONPATH=<folder with Pillow> python3 scripts/brick-messages.py deploy/brick/msg

The updater shows them with `pocket-fb --picture`; the "missing files" one is also written as a raw 32-bit frame per size
(`missing.raw`, `missing-wide.raw`: pocket-fb itself may be what is missing). Run again only when a text changes; the PNGs are committed.
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FONT = ROOT / "assets/fonts/Barlow-Medium.ttf"
BG, FG, DIM, ACCENT = (16, 19, 26), (240, 242, 246), (150, 158, 172), (255, 190, 70)

# English first (big title, then its lines), German second (small title, then its lines): most players read English.
MESSAGES = {
    "starting": {
        "colour": ACCENT,
        "en": ("pocket-ui is starting", ["Please wait a moment."]),
        "de": ("pocket-ui startet", ["Einen Moment bitte."]),
    },
    "missing": {
        "colour": (255, 110, 100),
        "en": ("Files are missing", [
            "A part of pocket-ui is missing on this card.",
            "Please copy the whole folder to the card again.",
            "More in pocket-ui/logs/boot.log.",
            "To go back to the normal start: switch the handheld",
            "off and put the normal card in."]),
        "de": ("Dateien fehlen", [
            "Auf dieser Karte fehlt ein Teil von pocket-ui.",
            "Bitte den ganzen Ordner neu auf die Karte kopieren.",
            "Mehr steht in pocket-ui/logs/boot.log.",
            "Zurück zum normalen Start: Gerät ausschalten,",
            "die normale Karte einlegen."]),
    },
    "crashed": {
        "colour": (255, 110, 100),
        "en": ("pocket-ui does not start", [
            "pocket-ui stopped several times in a row.",
            "The reason is in pocket-ui/logs/boot.log on the card.",
            "To go back to the normal start: switch off (hold Power)",
            "and put the normal card in.",
            "The handheld switches itself off in 10 minutes."]),
        "de": ("pocket-ui startet nicht", [
            "pocket-ui hat sich mehrmals hintereinander beendet.",
            "Der Grund steht in pocket-ui/logs/boot.log auf der Karte.",
            "Zurück zum normalen Start: Gerät ausschalten (Power lange",
            "drücken), die normale Karte einlegen.",
            "Das Gerät schaltet sich in 10 Minuten selbst aus."]),
    },
}


def font(size):
    from PIL import ImageFont  # imported late: the card test reads MESSAGES without Pillow
    return ImageFont.truetype(str(FONT), size)


# (width, height, English title y, German title y, first line y, line step): the same look, spaced to the screen's height.
SIZES = {"": (1024, 768, 90, 180, 262, 41), "-wide": (1280, 720, 60, 144, 220, 40)}


def draw(message, size):
    from PIL import Image, ImageDraw
    w, h, ty, sy, y, step = size
    img = Image.new("RGB", (w, h), BG)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, w - 1, 10], fill=message["colour"])
    d.text((64, ty), message["en"][0], font=font(72), fill=FG)
    d.text((64, sy), message["de"][0], font=font(36), fill=DIM)
    for lang in ("en", "de"):
        for line in message[lang][1]:
            assert font(32).getlength(line) <= w - 128, f"line too wide: {line}"
            d.text((64, y), line, font=font(32), fill=FG)
            y += step
        y += step // 2 + 6  # a gap between the languages
    y -= step // 2 + 6 + step
    assert y + 40 <= h - 24, f"text runs off the {w}x{h} screen"
    return img


def raw_bgra(img):
    from PIL import Image
    r, g, b = img.split()
    return Image.merge("RGBA", (b, g, r, Image.new("L", img.size, 255))).tobytes()


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for name, message in MESSAGES.items():
        for suffix, size in SIZES.items():
            img = draw(message, size)
            img.save(out / f"{name}{suffix}.png", optimize=True)
            if name == "missing":
                (out / f"missing{suffix}.raw").write_bytes(raw_bgra(img))
    print(f"brick-messages: {len(MESSAGES) * len(SIZES)} pictures in {out}")


if __name__ == "__main__":
    main()
