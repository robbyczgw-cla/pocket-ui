"""recolour.py: two corrections after the first review (2026-10-30), run from the repository's root.
- umd.png: the window ring round the label was a saturated blue (the photo's label edge); a real UMD shows the disc's
  dark edge through its clear window, so the blue becomes dark grey at the same brightness steps.
- ps2.png: the hub came out cream from the photo's white; it becomes the clear, slightly blue grey of the other hubs."""
from PIL import Image
D = "assets/media/disc/"
im = Image.open(D + "umd.png").convert("RGBA"); px = im.load()
for y in range(im.height):
    for x in range(im.width):
        r, g, b, a = px[x, y]
        if a == 255 and b > 0x90 and b > r + 0x50 and b > g + 0x30:
            l = (r + g + b) // 3
            v = max(0, min(255, l - 10))
            px[x, y] = (v, v + 2, v + 8, 255)
im.save(D + "umd.png")
im = Image.open(D + "ps2.png").convert("RGBA"); px = im.load()
for y in range(im.height):
    for x in range(im.width):
        r, g, b, a = px[x, y]
        if a == 255 and r > 0xe0 and g > 0xe0 and r >= b + 8 and (r, g, b) != (255, 0, 255):
            px[x, y] = (r - 14, g - 4, min(255, b + 14), 255)
im.save(D + "ps2.png")
