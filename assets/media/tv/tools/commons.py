import json,sys,urllib.request,urllib.parse
q=sys.argv[1]
u="https://commons.wikimedia.org/w/api.php?"+urllib.parse.urlencode(dict(action="query",generator="search",gsrsearch=q,gsrnamespace=6,gsrlimit=15,prop="imageinfo",iiprop="url|extmetadata|size",iiurlwidth=1280,format="json"))
r=json.load(urllib.request.urlopen(urllib.request.Request(u,headers={"User-Agent":"pocket-ui-art/1.0"})))
for p in r.get("query",{}).get("pages",{}).values():
    i=p["imageinfo"][0];m=i["extmetadata"]
    print(p["title"],i["width"],i["height"],m.get("LicenseShortName",{}).get("value"),"|",m.get("Artist",{}).get("value","")[:50])
