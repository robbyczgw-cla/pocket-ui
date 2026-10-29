"""finalize.py <raw_dir> <out_dir>: the PixelLab pictures as pocket-ui ships them.

Each is cut to what it shows (a 1 px clear margin), its transparency made on/off (solid from half on) and saved as a
plain RGBA PNG. Stickers get the same die-cut edge each: 2 px of paper white round the picture, then 1 px of dark
outline, so they read as one set on dark and on light. Which raw picture became which file: PICKS below; the prompts
are in ../SOURCES.md and ../pixellab-assets.json."""
import os, sys
from PIL import Image

PICKS = {
    # empty states
    "empty-no-games": "e_noGames_a",
    "empty-no-favorites": "e_noFav_a",
    "empty-history": "e_history_a",
    "empty-offline": "e_offline_d",
    "empty-hotel": "e_hotel_c",
    "empty-screenshots": "e_shots_c",
    "empty-search": "e_search_b",
    "empty-store": "e_store_a",
    "empty-nothing-played": "e_played_a",
    # Year in review stickers
    "sticker-completed": "s_done_a",
    "sticker-hundred-hours": "s_hours_a",
    "sticker-ten-games": "s_ten_b",
    "sticker-streak": "s_streak_a",
    "sticker-came-back": "s_back_b",
    # the fronts of boxes that are not a game (2026-10-29, cohesion)
    "emblem-curator": "curator_d",
    "emblem-surprise": "surprise_a",
    "emblem-streaming": "streaming_a",
    "emblem-sticker": "sticker_a",
}
PAPER = (0xf4, 0xf1, 0xea, 255)
EDGE = (0x2a, 0x27, 0x24, 255)


def binary(im):
    im = im.convert("RGBA")
    px = im.load()
    for y in range(im.height):
        for x in range(im.width):
            r, g, b, a = px[x, y]
            px[x, y] = (r, g, b, 255) if a >= 128 else (0, 0, 0, 0)
    return im


def trim(im, margin=1):
    box = im.getbbox()
    im = im.crop(box)
    out = Image.new("RGBA", (im.width + 2 * margin, im.height + 2 * margin), (0, 0, 0, 0))
    out.paste(im, (margin, margin))
    return out


def grow(im, n, colour):
    """n rings of `colour` round the solid pixels (8-connected), on a canvas n px larger on every side."""
    out = Image.new("RGBA", (im.width + 2 * n, im.height + 2 * n), (0, 0, 0, 0))
    out.paste(im, (n, n))
    for _ in range(n):
        src = out.copy().load()
        px = out.load()
        for y in range(out.height):
            for x in range(out.width):
                if src[x, y][3]:
                    continue
                if any(0 <= x + dx < out.width and 0 <= y + dy < out.height and src[x + dx, y + dy][3]
                       for dx in (-1, 0, 1) for dy in (-1, 0, 1)):
                    px[x, y] = colour
    return out


def main():
    raw, out = sys.argv[1], sys.argv[2]
    for name, src in PICKS.items():
        im = trim(binary(Image.open(os.path.join(raw, src + ".png"))), 0)
        if name.startswith("sticker-"):
            im = grow(grow(im, 2, PAPER), 1, EDGE)
        im = trim(im)
        im.save(os.path.join(out, name + ".png"), optimize=True)
        print(name, im.size)


if __name__ == "__main__":
    main()
