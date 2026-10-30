import json, sys, time, urllib.parse, urllib.request, urllib.error, re
UA = {"User-Agent": "pocket-ui-asset-research/0.1 (photo licence check)"}
def api(**p):
    p.update(format="json")
    u = "https://commons.wikimedia.org/w/api.php?" + urllib.parse.urlencode(p)
    for i in range(10):
        try: return json.load(urllib.request.urlopen(urllib.request.Request(u, headers=UA), timeout=60))
        except urllib.error.HTTPError as e:
            if e.code != 429: raise
            time.sleep(20 * (i + 1))
for c in sys.argv[1:]:
    time.sleep(3)
    r = api(action="query", generator="categorymembers", gcmtitle=c, gcmlimit=200, gcmtype="file|subcat", prop="imageinfo", iiprop="extmetadata|size", iiextmetadatafilter="LicenseShortName|Artist")
    print("##", c)
    for pg in r.get("query", {}).get("pages", {}).values():
        if pg["ns"] == 14: print("   SUBCAT", pg["title"]); continue
        ii = pg.get("imageinfo", [{}])[0]; md = ii.get("extmetadata", {})
        lic = md.get("LicenseShortName", {}).get("value", "?")
        art = re.sub("<[^>]+>", "", md.get("Artist", {}).get("value", "?"))[:25]
        flag = "PD " if lic.startswith(("Public", "CC0", "PD")) else "   "
        print(f"{flag}{lic[:14]:14} {ii.get('width')}x{ii.get('height')} {art:25} {pg['title']}")
