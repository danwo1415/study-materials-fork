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
                        api_paths = list(dict.fromkeys(re.findall(r"[/][a-zA-Z0-9_/-]*(?:search|play|video|media|episode|source|stream)[a-zA-Z0-9_/?=&.-]*",data,re.I)))
                        print("public_script",json.dumps({"path":parsed.path[-100:],"bytes_sampled":len(data),"api_path_shapes":[p[:130] for p in api_paths[:20]],
                          "uses_hls":"m3u8" in data.lower(),"uses_drm":any(x in data.lower() for x in ("widevine","licenseurl","eme")),
                          "api_prefixes":[x[:85] for x in list(dict.fromkeys(re.findall(r"/(?:api|search|play|video|media)/[a-zA-Z0-9_?=&./-]{1,80}", data)))[:20]]},ensure_ascii=False))
                    except Exception as e:
                        print("public_script_error",type(e).__name__,str(e)[:100])
    except Exception as e:
        print("site_error",json.dumps({"type":type(e).__name__,"message":str(e)[:150]},ensure_ascii=False))
