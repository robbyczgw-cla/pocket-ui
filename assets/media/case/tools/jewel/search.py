import json, sys, urllib.parse, urllib.request, urllib.error
UA = {"User-Agent": "pocket-ui-asset-research/0.1 (photo licence check)"}
def api(**p):
    p.update(format="json")
    u = "https://commons.wikimedia.org/w/api.php?" + urllib.parse.urlencode(p)
    for i in range(10):
        try:
            return json.load(urllib.request.urlopen(urllib.request.Request(u, headers=UA), timeout=60))
        except urllib.error.HTTPError as e:
            if e.code != 429: raise
            import time; time.sleep(20 * (i + 1))
    raise SystemExit("429 forever")
import time
for q in sys.argv[1:]:
    time.sleep(4)
    r = api(action="query", generator="search", gsrsearch=q, gsrnamespace=6, gsrlimit=40, prop="imageinfo",
            iiprop="extmetadata|size", iiextmetadatafilter="LicenseShortName|Artist")
    print("##", q)
    for pg in sorted(r.get("query", {}).get("pages", {}).values(), key=lambda p: p.get("index", 0)):
        ii = pg["imageinfo"][0]; md = ii.get("extmetadata") or {}
        if not isinstance(md, dict): md = {}
        lic = md.get("LicenseShortName", {}).get("value", "?")
        art = md.get("Artist", {}).get("value", "?")
        import re; art = re.sub("<[^>]+>", "", art)[:30]
        if not (lic.startswith("Public") or lic.startswith("CC0") or lic.startswith("PD")) or pg["title"].endswith((".pdf",".djvu",".webm")): continue
        print(f"{lic[:18]:18} {ii['width']}x{ii['height']} {art:30} {pg['title']}")
