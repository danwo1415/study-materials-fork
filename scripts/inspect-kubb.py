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

# Kubb's own frontend maps relative /api/* requests to https://api.kubb.tv.
# Probe anonymous GET only; do not emulate signed tokens or request protected media.
for endpoint, params in [
  ("/api/Video/VideoDetails", {"mediaKey":"qVyzCcnhWtG"}),
  ("/api/Video/VideoChooseGather",{"mediaKey":"qVyzCcnhWtG"}),
  ("/api/Video/getPlayData",{"mediaKey":"qVyzCcnhWtG"}),
  ("/api/List/GetTitleGetData",{"keyWord":"犬夜叉","page":1}),
  ("/api/Home/GetKeyWord",{"keyWord":"犬夜叉"})
]:
    url = "https://api.kubb.tv"+endpoint+"?"+urlencode(params)
    try:
        req=urllib.request.Request(url,headers={**HEADERS,"Accept":"application/json","Referer":"https://m.kubb.tv/","Origin":"https://m.kubb.tv"})
        with urllib.request.urlopen(req,timeout=12) as resp:
            raw=resp.read(40000).decode("utf-8","replace")
            typ=resp.headers.get("Content-Type")
        try: result=summarize_response(json.loads(raw))
        except json.JSONDecodeError: result={"content_sample":raw[:130]}
        print("KUBB_REAL_API",endpoint,"status",resp.status,"type",typ,"shape",repr(result)[:3500],flush=True)
    except Exception as e: print("KUBB_REAL_API_ERROR",endpoint,type(e).__name__,str(e)[:160],flush=True)

# Examine public media URL types without fetching copyrighted video content.
for endpoint in ("/api/Video/VideoChooseGather", "/api/Video/getPlayData"):
    url="https://api.kubb.tv"+endpoint+"?mediaKey=qVyzCcnhWtG"
    try:
        with urllib.request.urlopen(urllib.request.Request(url,headers={**HEADERS,"Accept":"application/json"}),timeout=12) as response:
            payload=json.loads(response.read(50000))
        items=payload.get("data",{}).get("list",[])
        out=[]
        for item in items[:6]:
            link=item.get("mediaUrl","")
            parsed=urlparse(link) if isinstance(link,str) else None
            out.append({"episodeTitle":str(item.get("episodeTitle",""))[:80],
                "isVip":item.get("isVip"),"isLimit":item.get("isLimit"),
                "episodeKey":item.get("episodeKey"),
                "mediaUrl_nonempty":bool(link),"media_scheme":parsed.scheme if parsed else None,
                "media_host":parsed.hostname if parsed else None,
                "media_path_suffix":parsed.path[-45:] if parsed else None,
                "videoType":item.get("videoType"),"resolution":item.get("resolution"),
                "mediaId":item.get("mediaId")})
        print("KUBB_PLAYLIST_META",endpoint,json.dumps(out,ensure_ascii=False),flush=True)
    except Exception as e:print("KUBB_PLAYLIST_ERROR",endpoint,type(e).__name__,str(e)[:100],flush=True)

# Search endpoint controls: Kubb returns a JSON list; check public GET parameter names.
for params in [
    {"keyWord":"公民义警","page":1}, {"keyword":"公民义警","page":1},
    {"keyWord":"公民义警","pageIndex":1}, {"title":"公民义警","page":1},
    {"search":"公民义警","page":1}, {"name":"公民义警","page":1}
]:
    url="https://api.kubb.tv/api/List/GetTitleGetData?"+urlencode(params)
    try:
        with urllib.request.urlopen(urllib.request.Request(url,headers={**HEADERS,"Accept":"application/json"}),timeout=11) as response:
            obj=json.loads(response.read(25000))
        data=obj.get("data",{})
        list_=data.get("list",[]) if isinstance(data,dict) else []
        print("KUBB_SEARCH_PARAM",json.dumps(params,ensure_ascii=False),"ret",obj.get("ret"),"n",len(list_),"first_shape",repr(summarize_response(list_[0]))[:1100] if list_ else None,flush=True)
    except Exception as e: print("KUBB_SEARCH_PARAM_ERROR",str(e)[:130],flush=True)

# Find exact client-side search query params and anonymous media manifest availability.
try:
    url="https://m.kubb.tv/script/chunk-cc6e1d50.1791277306588.js"
    with urllib.request.urlopen(urllib.request.Request(url,headers=HEADERS),timeout=12) as response:
        content=response.read(750000).decode("utf-8","replace")
    for term in ['a("2f22")','Object(i["d"])','Object(s["d"])','Object(f["d"])','getTitleGetData','keyWord:this','keyword:this','keyWord:e','keyWord:t','searchKey','searchVal','SearchList','searchData']:
        matches=list(re.finditer(re.escape(term),content,re.I))
        print("SEARCH_CALL",term,"n",len(matches),flush=True)
        for m in matches[:4]:
            print("SEARCH_PARAMS_CTX",repr(content[max(0,m.start()-520):min(len(content),m.end()+850)]),flush=True)
except Exception as e: print("SEARCH_SCRIPT_ERROR",str(e)[:140],flush=True)

try:
    url="https://api.kubb.tv/api/Video/getPlayData?mediaKey=qVyzCcnhWtG"
    with urllib.request.urlopen(urllib.request.Request(url,headers={**HEADERS,"Accept":"application/json"}),timeout=10) as response:
        payload=json.load(response)
    items=payload.get("data",{}).get("list",[])
    candidate=next((item.get("mediaUrl") for item in items if not item.get("isVip") and item.get("mediaUrl","").startswith("https://")),None)
    if candidate:
        req=urllib.request.Request(candidate,headers={"User-Agent":HEADERS["User-Agent"],"Referer":"https://m.kubb.tv/"})
        with urllib.request.urlopen(req,timeout=13) as response:
            raw=response.read(55000).decode("utf-8","replace")
            ctype=response.headers.get("Content-Type")
        print("KUBB_HLS_MANIFEST",json.dumps({"type":ctype,"valid":raw.lstrip().startswith("#EXTM3U"),
            "encrypted":bool(re.search(r"#EXT-X-KEY:METHOD=(?!NONE)",raw)),
            "lines":len(raw.splitlines()),"segments":sum(1 for x in raw.splitlines() if x and not x.startswith("#")),
            "discontinuities":raw.count("#EXT-X-DISCONTINUITY"),"master":"#EXT-X-STREAM-INF" in raw},ensure_ascii=False),flush=True)
except Exception as e:print("KUBB_HLS_ERROR",type(e).__name__,str(e)[:130],flush=True)

# Confirm public search and stream requests work without user headers or login.
for endpoint, params in [
 ("/api/List/GetTitleGetData", {"SearchCriteria":"公民义警"}),
 ("/api/List/GetTitleGetData", {"SearchCriteria":"犬夜叉"}),
 ("/api/Video/getPlayData", {"mediaKey":"qVyzCcnhWtG"})
]:
 url="https://api.kubb.tv"+endpoint+"?"+urlencode(params)
 try:
  req=urllib.request.Request(url,headers={"User-Agent":HEADERS["User-Agent"],"Accept":"application/json"})
  with urllib.request.urlopen(req,timeout=12) as response:
   obj=json.loads(response.read(50000))
  data=obj.get("data",{})
  items=data.get("list",[])
  print("KUBB_NOAUTH_JSON",endpoint,"q",list(params.values())[0],"ret",obj.get("ret"),"n",len(items),"first",repr(summarize_response(items[0]))[:2400] if items else None,flush=True)
 except Exception as e: print("KUBB_NOAUTH_ERROR",endpoint,type(e).__name__,str(e)[:150],flush=True)


# Diagnosis of publicly visible anime episode collections; do not fetch video bytes.
print("==== INUYASHA PUBLIC EPISODE DIAGNOSTICS", flush=True)
def safe_str(value, n=50):
    return str(value)[:n] if isinstance(value,(str,int,float)) else ""
def api_data(endpoint, params):
    url="https://api.kubb.tv"+endpoint+"?"+urlencode(params)
    req=urllib.request.Request(url,headers={"User-Agent":HEADERS["User-Agent"],"Accept":"application/json"})
    with urllib.request.urlopen(req,timeout=12) as response:
        payload=json.loads(response.read(700000))
    return payload.get("data",{})
try:
    search=api_data("/api/List/GetTitleGetData",{"SearchCriteria":"犬夜叉"})
    works=search.get("list",[])
    print("INUYASHA_SEARCH",json.dumps([{"title":safe_str(w.get("title")),
      "count":len(w.get("episodes",[])),
      "sample":[{"title":safe_str(e.get("title")),"episodeTitle":safe_str(e.get("episodeTitle")),
        "mediaKey":safe_str(e.get("mediaKey")),"episodeKey":safe_str(e.get("episodeKey"))}
        for e in w.get("episodes",[])[:6]]} for w in works[:3]],ensure_ascii=False),flush=True)
    for work in works[:2]:
      items=work.get("episodes",[])
      for media_key in list(dict.fromkeys(safe_str(x.get("mediaKey")) for x in (items[:1]+items[-1:]) if x.get("mediaKey"))):
        for endpoint in ["/api/Video/VideoDetails","/api/Video/VideoChooseGather","/api/Video/getPlayData"]:
            try:
                payload=api_data(endpoint,{"mediaKey":media_key})
                li=payload.get("list",[])
                print("INUYASHA_API",json.dumps({"key":media_key,"endpoint":endpoint,
                    "detailTitle":safe_str(payload.get("detailInfo",{}).get("title")),
                    "listLen":len(li),
                    "sample":[{"title":safe_str(item.get("title")),
                               "episodeTitle":safe_str(item.get("episodeTitle")),
                               "episodeKey":safe_str(item.get("episodeKey")),
                               "mediaKey":safe_str(item.get("mediaKey")),
                               "isVip":item.get("isVip"),"isLimit":item.get("isLimit"),
                               "hasMediaUrl":bool(item.get("mediaUrl"))}
                               for item in (li[:3]+li[-3:])]},ensure_ascii=False),flush=True)
            except Exception as e:
                print("INUYASHA_API_ERROR",endpoint,type(e).__name__,str(e)[:120],flush=True)
except Exception as e:
    print("INUYASHA_DIAG_ERROR",type(e).__name__,str(e)[:140],flush=True)


# Confirm that selecting an episode returns ITS media, not the last-played one.
print("==== INUYASHA EPISODE PLAY SELECTION",flush=True)
for params in [
    {"mediaKey":"06TnrIsiRP7","episodeKey":"cFfQkN3Hfr8"},
    {"mediaKey":"06TnrIsiRP7","id":"cFfQkN3Hfr8"},
    {"mediaKey":"06TnrIsiRP7","episodeId":"122647"},
    {"episodeKey":"cFfQkN3Hfr8"},
    {"mediaKey":"06TnrIsiRP7","episodeKey":"aWGeGOGluBD"},
]:
    try:
        data=api_data("/api/Video/getPlayData",params)
        first=data.get("list",[None])[0]
        print("INUYASHA_PLAY_PARAM",json.dumps({"params":params,"count":len(data.get("list",[])),
            "returnedKey":safe_str(first.get("episodeKey")) if first else "",
            "returnedTitle":safe_str(first.get("episodeTitle")) if first else "",
            "hasUrl":bool(first.get("mediaUrl")) if first else False},ensure_ascii=False),flush=True)
    except Exception as e: print("INUYASHA_PLAY_PARAM_ERROR",type(e).__name__,str(e)[:120],flush=True)

# Full published browser bundle call site surrounding the public getPlayData method.
try:
    with urllib.request.urlopen(urllib.request.Request("https://m.kubb.tv/script/app.1791277306588.js",headers=HEADERS),timeout=12) as resp:
        bundle=resp.read(1500000).decode("utf-8","replace")
    for term in ['"/api/Video/getPlayData"', '"/api/Video/VideoChooseGather"']:
        for match in list(re.finditer(re.escape(term),bundle))[:3]:
            print("INUYASHA_JS_CALL",repr(bundle[max(0,match.start()-700):match.end()+500]),flush=True)
except Exception as e:print("INUYASHA_JS_ERROR",type(e).__name__,str(e)[:120],flush=True)


# Ikanbot public-page capability probe: metadata only, no streaming content.
print("==== IKANBOT PUBLIC PLAY PAGE",flush=True)
for site in ["https://www1.ikanbot.com/play/442095","https://www1.ikanbot.com/"]:
 try:
  request=urllib.request.Request(site,headers=HEADERS)
  with urllib.request.urlopen(request,timeout=15) as response:
   raw=response.read(850000).decode("utf-8","replace")
   final=response.url
   status=response.status
   typ=response.headers.get("Content-Type","")
  parser=Inspect()
  parser.feed(raw)
  print("IKANBOT_PAGE",json.dumps({"url":site,"status":status,"final_host":urlparse(final).hostname,
   "final_path":urlparse(final).path,"type":typ,"size":len(raw),
   "title":"".join(parser.title)[:130],
   "scripts":[urlparse(urljoin(final,s)).path[-130:] for s in parser.scripts[:30]],
   "iframe_hosts":[urlparse(urljoin(final,s)).hostname for s in parser.frames][:15],
   "links":len(parser.links),
   "video_tags":len(parser.videos),
   "play_links":list(dict.fromkeys(urlparse(urljoin(final,s)).path for s,_ in parser.links if "/play/" in s))[:14],
   "markers":{k:(k.lower() in raw.lower()) for k in ["m3u8","video","episode","player","iframe","__NEXT_DATA__","__NUXT__","axios","api/","playUrl","sources"]},
   "media_url_patterns":len(re.findall(r"\.m3u8|\.mp4",raw,re.I))},ensure_ascii=False),flush=True)
  for js in parser.scripts[:12]:
   target=urljoin(final,js)
   uri=urlparse(target)
   if uri.scheme!="https" or not uri.hostname or not (uri.hostname=="www1.ikanbot.com" or uri.hostname.endswith(".ikanbot.com")):continue
   if not uri.path.endswith(".js"):continue
   try:
    with urllib.request.urlopen(urllib.request.Request(target,headers=HEADERS),timeout=10) as jr:
     body=jr.read(600000).decode("utf-8","replace")
    print("IKANBOT_JS",json.dumps({"file":uri.path[-130:],"size":len(body),
     "markers":{k:(k.lower() in body.lower()) for k in ["m3u8","playUrl","playlist","episode","/api/","axios","videojs","hls.js","player","movie"]},
     "endpoint_shapes":list(dict.fromkeys(re.findall(r'/(?:api|play|video|search|movie)/[A-Za-z0-9_/?=&.-]{2,95}',body)))[:20]},ensure_ascii=False),flush=True)
   except Exception as e:
    print("IKANBOT_JS_ERROR",uri.path[-60:],type(e).__name__,str(e)[:100],flush=True)
 except Exception as e:
  print("IKANBOT_ERROR",site,type(e).__name__,str(e)[:180],flush=True)


# Ikanbot parser diagnostics: safe code excerpts, redact actual URLs and tokens.
print("==== IKANBOT PLAYER CONTRACT",flush=True)
try:
 page_url="https://www1.ikanbot.com/play/442095"
 with urllib.request.urlopen(urllib.request.Request(page_url,headers=HEADERS),timeout=12) as resp:
  page=resp.read(180000).decode("utf-8","replace")
 with urllib.request.urlopen(urllib.request.Request("https://www1.ikanbot.com/resources/play_new.js",headers=HEADERS),timeout=12) as resp:
  script=resp.read(180000).decode("utf-8","replace")
 for term in ["/api/getResN","getResN","playUrl","videojs","sources:","source:","/api/","m3u8","$.post","$.get","fetch(","POST","GET"]:
  locations=[m.start() for m in re.finditer(re.escape(term),script,re.I)]
  print("IKANBOT_JS_TERM",term,len(locations),flush=True)
  for pos in locations[:3]:
   snippet=script[max(0,pos-360):min(len(script),pos+500)]
   snippet=re.sub(r'https?://[^\s"\'<>]+', '[REMOTE_URL]',snippet)
   print("IKANBOT_JS_CONTEXT",term,repr(snippet[:900]),flush=True)
 for term in ["<video","data-","getResN","play_new.js","source","playId","vodId","episode","第1集","选集"]:
  locations=[m.start() for m in re.finditer(re.escape(term),page,re.I)]
  print("IKANBOT_HTML_TERM",term,len(locations),flush=True)
  for pos in locations[:3]:
   snippet=page[max(0,pos-250):min(len(page),pos+450)]
   snippet=re.sub(r'https?://[^\s"\'<>]+','[REMOTE_URL]',snippet)
   print("IKANBOT_HTML_CONTEXT",term,repr(snippet[:850]),flush=True)
except Exception as e:
 print("IKANBOT_CONTRACT_ERROR",type(e).__name__,str(e)[:150],flush=True)


# Direct verification of the public Ikanbot playback manifest API, without tokens or media bytes.
print("==== IKANBOT DIRECT PUBLIC ENDPOINT CHECK", flush=True)
try:
  target="https://www1.ikanbot.com/play/442095"
  with urllib.request.urlopen(urllib.request.Request(target,headers=HEADERS),timeout=15) as resp:
    text=resp.read(130000).decode("utf-8","replace")
  for needle in ["loadVideoId","mtype","v_tks","lineContent","video_title","get_tks","/search","searchForm"]:
    hit=list(re.finditer(re.escape(needle),text,re.I))
    contexts=[]
    for m in hit[:2]:
      view=text[max(0,m.start()-120):m.end()+170]
      # Avoid printing page tokens or signed remote URLs.
      view=re.sub(r'https?://[^\s"\'<>]+',"[REMOTE_URL]",view)
      view=re.sub(r'(v_tks\s*=\s*)[^;]+',r'\1[REDACTED]',view)
      if needle == "get_tks": view="[JS TOKEN FUNCTION PRESENT]"
      contexts.append(view[:340])
    print("IKANBOT_PARAMS",json.dumps({"term":needle,"matches":len(hit),"contexts":contexts},ensure_ascii=False),flush=True)
  inspect=Inspect();inspect.feed(text)
  print("IKANBOT_FORMS",json.dumps(inspect.forms[:12],ensure_ascii=False),flush=True)
except Exception as e:
  print("IKANBOT_PAGE_PARAMS_ERROR",type(e).__name__,str(e)[:160],flush=True)

for mtype in ["1","0","2"]:
  params={"videoId":"442095","mtype":mtype,"token":""}
  api_url="https://www1.ikanbot.com/api/getResN?"+urlencode(params)
  try:
    request=urllib.request.Request(api_url,headers={**HEADERS,"Accept":"application/json","Referer":"https://www1.ikanbot.com/play/442095"})
    with urllib.request.urlopen(request,timeout=15) as response:
      raw=response.read(500000).decode("utf-8","replace")
      status=response.status
      content_type=response.headers.get("Content-Type","")
    try:
      data=json.loads(raw)
      items=(data.get("data") or {}).get("list") if isinstance(data,dict) else None
      groups=items if isinstance(items,list) else []
      output={"http_status":status,"mtype":mtype,"content_type":content_type,
        "state":data.get("state"),"top_keys":list(data.keys())[:16],
        "data_keys":list(data.get("data",{}).keys())[:16] if isinstance(data.get("data"),dict) else [],
        "groups_count":len(groups),"group_types":[type(x).__name__ for x in groups[:3]],
        "group_keys":[list(x.keys())[:12] for x in groups[:3] if isinstance(x,dict)]}
      print("IKANBOT_GETRES",json.dumps(output,ensure_ascii=False),flush=True)
      # Summarize episode and stream shape, but never expose URLs.
      for g in groups[:2]:
        if not isinstance(g,dict):continue
        summaries={}
        for k,v in g.items():
          if isinstance(v,list):
            summaries[k]={"type":"list","length":len(v),"first_type":type(v[0]).__name__ if v else ""}
          elif isinstance(v,str):
            summaries[k]={"type":"str","length":len(v),"m3u8_count":v.lower().count(".m3u8"),
               "separator_count":v.count("#")}
          elif isinstance(v,(dict,int,float,bool)):
            summaries[k]={"type":type(v).__name__}
        print("IKANBOT_GROUP",json.dumps(summaries,ensure_ascii=False),flush=True)
    except json.JSONDecodeError:
      print("IKANBOT_GETRES_NOT_JSON",json.dumps({"status":status,"mtype":mtype,"type":content_type,"len":len(raw),"looks_html":"<html" in raw.lower()},ensure_ascii=False),flush=True)
  except urllib.error.HTTPError as e:
    print("IKANBOT_GETRES_HTTP_ERROR",json.dumps({"mtype":mtype,"status":e.code},ensure_ascii=False),flush=True)
  except Exception as e:
    print("IKANBOT_GETRES_ERROR",json.dumps({"mtype":mtype,"error":type(e).__name__,"desc":str(e)[:100]},ensure_ascii=False),flush=True)


print("==== IKANBOT ACTUAL MTYPE AND SEARCH",flush=True)
for params in [
 {"videoId":"442095","mtype":"18","token":""},
 {"videoId":"442095","mtype":"18"},
]:
 try:
  req=urllib.request.Request("https://www1.ikanbot.com/api/getResN?"+urlencode(params),headers={**HEADERS,"Accept":"application/json","Referer":"https://www1.ikanbot.com/play/442095"})
  with urllib.request.urlopen(req,timeout=12) as resp:
   raw=resp.read(200000)
   code=resp.status
  obj=json.loads(raw)
  print("IKANBOT_ACTUAL_MTYPE",json.dumps({"http":code,"params":params,"state":obj.get("state"),"data_type":type(obj.get("data")).__name__,"data_length":len(obj.get("data")) if isinstance(obj.get("data"),(dict,list)) else None},ensure_ascii=False),flush=True)
 except Exception as e:print("IKANBOT_ACTUAL_MTYPE_ERROR",type(e).__name__,str(e)[:100],flush=True)
try:
 req=urllib.request.Request("https://www1.ikanbot.com/search?q="+urlencode({"":"犬夜叉"}).split("=",1)[1],headers=HEADERS)
 with urllib.request.urlopen(req,timeout=12) as resp:
  body=resp.read(250000).decode("utf-8","replace")
  p=Inspect();p.feed(body)
  matches=list(dict.fromkeys(urlparse(urljoin(resp.url,h)).path for h,_ in p.links if "/play/" in h))
  print("IKANBOT_SEARCH_CHECK",json.dumps({"status":resp.status,"title":"".join(p.title)[:100],"play_links":len(matches),"target_present":"/play/442095" in matches,"samples":matches[:10]},ensure_ascii=False),flush=True)
except Exception as e:print("IKANBOT_SEARCH_CHECK_ERROR",type(e).__name__,str(e)[:100],flush=True)
