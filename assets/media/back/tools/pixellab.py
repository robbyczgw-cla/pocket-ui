"""pixellab.py <in.png> <out.png> <seed> <description...>: PixelLab v2 create-image-pixflux with the starting picture of
draw_backs.py as init image, same size. With a background (`no_background` gives an empty picture for a frame
around a plain middle): finish.py cuts it to the starting picture's outline. Appends the request (never the key) and its usage
to pixellab-log.jsonl next to <out.png>. The key is read from ~/.config/pixellab/api-key."""
import base64, json, os, sys, time, urllib.request
src, out, seed = sys.argv[1], sys.argv[2], int(sys.argv[3])
desc = " ".join(sys.argv[4:])
key = open(os.path.expanduser("~/.config/pixellab/api-key")).read().strip()
from PIL import Image
w, h = Image.open(src).size
body = {"description": desc, "image_size": {"width": w, "height": h}, "no_background": False, "seed": seed,
        "outline": "single color black outline", "shading": "medium shading", "detail": "medium detail",
        "view": "side", "text_guidance_scale": 8.0,
        "negative_description": "text, letters, logo, brand name, label text, barcode, people, perspective, angled, isometric, shadow",
        "init_image": {"type": "base64", "base64": base64.b64encode(open(src, "rb").read()).decode()},
        "init_image_strength": int(os.environ.get("INIT_STRENGTH", "250"))}
for attempt in range(6):
    try:
        req = urllib.request.Request("https://api.pixellab.ai/v2/create-image-pixflux", data=json.dumps(body).encode(),
                                     headers={"Authorization": "Bearer " + key, "Content-Type": "application/json"})
        r = json.load(urllib.request.urlopen(req, timeout=300))
        break
    except urllib.error.HTTPError as e:
        if e.code in (429, 502, 503) and attempt < 5:
            time.sleep(30)
            continue
        raise
open(out, "wb").write(base64.b64decode(r["image"]["base64"].split(",", 1)[-1]))
body["init_image"] = os.path.basename(src)
with open(os.path.join(os.path.dirname(os.path.abspath(out)), "pixellab-log.jsonl"), "a") as f:
    f.write(json.dumps({"out": os.path.basename(out), "time": time.strftime("%Y-%m-%dT%H:%M:%S"), "endpoint": "create-image-pixflux", "request": body, "usage": r.get("usage")}) + "\n")
print(out, r.get("usage"))
