"""fetch.py <File:Title> <out>: download the original via Commons API (with licence print)."""
import json, sys, time, urllib.parse, urllib.request, urllib.error
UA = {"User-Agent": "pocket-ui-asset-research/0.1 (photo licence check)"}
def get(u):
    for i in range(10):
        try:
            return urllib.request.urlopen(urllib.request.Request(u, headers=UA), timeout=120).read()
        except urllib.error.HTTPError as e:
            if e.code != 429: raise
            time.sleep(20 * (i + 1))
t, out = sys.argv[1], sys.argv[2]
r = json.loads(get("https://commons.wikimedia.org/w/api.php?" + urllib.parse.urlencode(dict(action="query", titles=t, prop="imageinfo", iiprop="url|extmetadata|size", format="json"))))
ii = list(r["query"]["pages"].values())[0]["imageinfo"][0]
md = ii["extmetadata"]
print(t, md.get("LicenseShortName", {}).get("value"), "|", md.get("Artist", {}).get("value", "")[:80], "|", md.get("DateTimeOriginal", {}).get("value", "")[:30], ii["width"], ii["height"])
open(out, "wb").write(get(ii["url"]))
