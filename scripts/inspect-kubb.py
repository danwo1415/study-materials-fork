#!/usr/bin/env python3
"""Inspect only publicly available Kubb HTML and JS; never request media files or tokens."""
import json
import re
import urllib.request
import urllib.error
from html.parser import HTMLParser
from urllib.parse import urljoin, urlparse

SITES = ["https://m.kubb.tv/", "https://m.kubb.tv/play/qVyzCcnhWtG", "https://www.kubb.tv/"]
HEADERS = {"User-Agent": "Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36", "Accept": "text/html,application/xhtml+xml"}
class Inspect(HTMLParser):
    def __init__(self):
        super().__init__()
        self.links = []; self.scripts = []; self.forms = []; self.frames = []; self.videos = []; self.current = None; self.title = []
    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "a" and a.get("href"): self.links.append((a.get("href"), a.get("title","")))
        if tag == "script" and a.get("src"): self.scripts.append(a.get("src"))
        if tag == "iframe" and a.get("src"): self.frames.append(a.get("src"))
        if tag == "video": self.videos.append(bool(a.get("src")))
        if tag == "form": self.forms.append({"method":a.get("method"),"action":a.get("action")})
        if tag == "title": self.current = "title"
    def handle_data(self, data):
        if self.current == "title": self.title.append(data)
    def handle_endtag(self, tag):
        if tag == "title": self.current = None

for source in SITES:
    print("==== KUBB PUBLIC SITE", source)
    req = urllib.request.Request(source, headers=HEADERS)
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            body = resp.read(600000).decode("utf-8","replace")
            final = resp.url
            parser = Inspect()
            parser.feed(body)
            links = [(urljoin(final, url), name) for url, name in parser.links]
            paths = [urlparse(url).path for url, _ in links if (urlparse(url).hostname or "").endswith("kubb.tv")]
            scripts = [urljoin(final, s) for s in parser.scripts]
            print(json.dumps({"status": resp.status, "redirected_host":urlparse(final).hostname, "final_path":urlparse(final).path,
              "content_type":resp.headers.get("Content-Type"),"size":len(body),"title":"".join(parser.title)[:120],
              "forms":parser.forms[:8], "iframe_hosts":[urlparse(urljoin(final,s)).hostname for s in parser.frames][:12],
              "video_tags":len(parser.videos), "link_count":len(paths),
              "play_paths":list(dict.fromkeys(x for x in paths if x.startswith("/play/")))[:15],
              "search_paths":list(dict.fromkeys(x for x in paths if "search" in x or "sousuo" in x))[:12],
              "other_paths":list(dict.fromkeys(paths))[:35],
              "script_files":[urlparse(u).path[-120:] for u in scripts[:28]],
              "bundled_data_markers":[x for x in ("__NEXT_DATA__", "__NUXT__", "__INITIAL_STATE__", "window.__", "m3u8", "playData", "player_", "Hls", "videoplayer", "search") if x in body],
              "public_media_matches":len(re.findall(r"\\.m3u8|\\.mp4",body,re.I))
              },ensure_ascii=False))
            if source.endswith("/") or "/play/" in source:
                for js in scripts[-6:]:
                    try:
                        parsed = urlparse(js)
                        if not parsed.scheme == "https" or not parsed.hostname or not parsed.hostname.endswith("kubb.tv"): continue
                        with urllib.request.urlopen(urllib.request.Request(js,headers=HEADERS),timeout=12) as jresp:
                            data = jresp.read(450000).decode("utf-8","replace")
                        if parsed.path.startswith("/script/app."):
                            print("KUBB_JS_HEADER", data[:650])
                            for pattern in ("baseURL", "/api/List/", "/api/Video/", "/api/home/", "search", "Search", "/play/", "playInfo", "playUrl", "videoUrl", "GetPlay", "getPlay", "m3u8", "getVideo", "GetVideo", "GetMovie", "getmovie", "videoInfo", "request", "GetMovieInfo"):
                                found = list(re.finditer(re.escape(pattern), data, re.I))
                                print("KUBB_CONTEXT", pattern, "hits", len(found))
                                for match in found[:3]:
                                    print("KUBB_CHUNK", repr(data[max(0, match.start()-280):min(len(data), match.end()+440)]))
                            hosts = list(dict.fromkeys(re.findall(r"https?://[a-z0-9.:-]+",data,re.I)))
                            print("KUBB_PUBLIC_HOSTS",hosts[:30])
                        api_paths = list(dict.fromkeys(re.findall(r"[/][a-zA-Z0-9_/-]*(?:search|play|video|media|episode|source|stream)[a-zA-Z0-9_/?=&.-]*",data,re.I)))
                        print("public_script",json.dumps({"path":parsed.path[-100:],"bytes_sampled":len(data),"api_path_shapes":[p[:130] for p in api_paths[:20]],
                          "uses_hls":"m3u8" in data.lower(),"uses_drm":any(x in data.lower() for x in ("widevine","licenseurl","eme")),
                          "api_prefixes":[x[:85] for x in list(dict.fromkeys(re.findall(r"/(?:api|search|play|video|media)/[a-zA-Z0-9_?=&./-]{1,80}", data)))[:20]]},ensure_ascii=False))
                    except Exception as e:
                        print("public_script_error",type(e).__name__,str(e)[:100])
    except Exception as e:
        print("site_error",json.dumps({"type":type(e).__name__,"message":str(e)[:150]},ensure_ascii=False))

# Targeted adapter investigation using only the public JavaScript bundles.
try:
    base = "https://m.kubb.tv/script/"
    for filename in ["app.1791277306588.js", "chunk-ce7e84a0.1791277306588.js"]:
        print("==== KUBB ADAPTER BUNDLE", filename, flush=True)
        with urllib.request.urlopen(urllib.request.Request(base+filename, headers=HEADERS), timeout=18) as resp:
            data = resp.read(1500000).decode("utf-8","replace")
        for term in ["function v(", "function b(", "function v(t)", "function b(t)", "function o(t)", "url:\"/api/Video/VideoDetails\"", "url:\"/api/Video/getPlayData\"", "url:\"/api/video/GetPlayAddress\"", "mediaKey", "videoKey", "GetSearch", "SearchVideo", "/api/Search/", "/api/search/", "search?"]:
            indexes = [m.start() for m in re.finditer(re.escape(term),data,re.I)]
            print("BUNDLE_TERM",term,"count",len(indexes))
            for i in indexes[:4]:
                print("BUNDLE_CONTEXT",repr(data[max(0,i-220):min(len(data),i+460)]))
        paths = list(dict.fromkeys(re.findall(r'url:"(/api/[^"]{2,100})"',data,re.I)))
        print("BUNDLE_API_PATHS",json.dumps([x for x in paths if any(t in x.lower() for t in ("search","play","video","list","home"))][:70],ensure_ascii=False))
        if filename.startswith("app."):
            for match in list(re.finditer(r'path:"/search[^"]*"',data,re.I))[:3]:
                print("SEARCH_ROUTE_CONTEXT",repr(data[max(0,match.start()-280):match.end()+320]))
except Exception as e:
    print("ADAPTER_BUNDLE_ERROR",type(e).__name__,str(e)[:200])

# Public API availability and lazily loaded search / video page adapter metadata.
from urllib.parse import urlencode
def summarize_response(value, depth=0):
    if depth > 3: return type(value).__name__
    if isinstance(value,dict): return {k:summarize_response(v, depth+1) for k,v in list(value.items())[:20]}
    if isinstance(value,list): return {"list_len":len(value),"first":summarize_response(value[0],depth+1) if value else None}
    if isinstance(value,str): return {"string_len":len(value),"sample": value[:80] if len(value)<80 and "://" not in value else "[redacted]"}
    return value

for filename in ["chunk-18913acc.1791277306588.js","chunk-cc6e1d50.1791277306588.js",
                 "chunk-2fcb4b44.1791277306588.js","chunk-80737fe6.1791277306588.js"]:
    try:
        req=urllib.request.Request("https://m.kubb.tv/script/"+filename,headers=HEADERS)
        with urllib.request.urlopen(req,timeout=12) as resp: data=resp.read(1600000).decode("utf-8","replace")
        print("KUBB_LAZY_CHUNK",filename,"bytes",len(data),flush=True)
        paths=list(dict.fromkeys(re.findall(r'url:"(/api/[^"]{2,90})"',data,re.I)))
        print("KUBB_LAZY_ENDPOINTS",json.dumps(paths[:75],ensure_ascii=False),flush=True)
        for term in ["search", "keywords", "keyword", "mediaKey", "VideoDetails", "getPlayData","episodeKey","GetPlayAddress"]:
            matches=list(re.finditer(re.escape(term),data,re.I))
            print("KUBB_LAZY_TERM",term,len(matches),flush=True)
            for m in matches[:2]:print("KUBB_LAZY_CTX",repr(data[max(0,m.start()-150):m.end()+240]),flush=True)
    except Exception as e:print("KUBB_LAZY_ERROR",filename,type(e).__name__,str(e)[:120],flush=True)

for endpoint,params in [
    ("/api/Video/VideoDetails",{"mediaKey":"qVyzCcnhWtG"}),
    ("/api/Video/VideoChooseGather",{"mediaKey":"qVyzCcnhWtG"}),
    ("/api/Video/getPlayData",{"mediaKey":"qVyzCcnhWtG"}),
    ("/api/video/GetPlayAddress",{"mediaKey":"qVyzCcnhWtG"}),
]:
    url="https://m.kubb.tv"+endpoint+"?"+urlencode(params)
    try:
        req=urllib.request.Request(url,headers={**HEADERS, "Accept":"application/json","Referer":"https://m.kubb.tv/"})
        with urllib.request.urlopen(req,timeout=10) as resp:
            raw=resp.read(18000).decode("utf-8","replace")
            kind=resp.headers.get("Content-Type")
        print("KUBB_GET_API",endpoint,"status",resp.status,"type",kind,"data",repr(summarize_response(json.loads(raw)))[:2500],flush=True)
    except Exception as e: print("KUBB_GET_ERROR",endpoint,type(e).__name__,str(e)[:150],flush=True)

# Inspect host selection and GET search argument names without guessing a protected endpoint.
for filename, terms in [
  ("app.1791277306588.js", ["var C=s.a.create", "function v(t){", "function b(t){", "https://api.", "m10.", "function y(t)", "ppt.{Host}"]),
  ("chunk-cc6e1d50.1791277306588.js", ["/api/Home/GetKeyWord","/api/List/GetTitleGetData","keyWord", "keyWord:", "page:", "searchList", "getSearchData"])
]:
  try:
    with urllib.request.urlopen(urllib.request.Request("https://m.kubb.tv/script/"+filename,headers=HEADERS),timeout=14) as response:
      content=response.read(800000).decode("utf-8","replace")
    print("==== ROUTER SEARCH",filename,flush=True)
    for term in terms:
      matches=list(re.finditer(re.escape(term),content))
      print("ROUTE_TERM",term,"occurrences",len(matches),flush=True)
      for match in matches[:3]:
        print("ROUTE_CTX",repr(content[max(0,match.start()-1600):min(len(content),match.end()+550)]),flush=True)
  except Exception as e:print("ROUTE_ERROR",str(e)[:120],flush=True)
