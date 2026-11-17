#!/usr/bin/env python3
"""Inspect bounded, public HTML player structures without downloading streams or executing scripts."""
import concurrent.futures
from html.parser import HTMLParser
import json
import re
import urllib.request
from urllib.parse import urlparse

PAGES = {
 "nnyy": "https://nnyy.in/dongman/20096378.html",
 "dm84": "https://dm84.tv/v/1773.html",
 "dm84-episode": "https://dm84.tv/p/1773-1-1.html",
 "ikanbot": "https://www1.ikanbot.com/play/381898",
 "bandai": "https://www.b-ch.com/titles/2277/001",
 "zhaojixie": "https://zhaojixie.com/category/jilupian.html",
}
KW = re.compile(r"player_|playlist|playfrom|playurl|m3u8|\\.mp4|vod_play|episode|第0?1集|第0?26集|iframe|data-src|data-url|data-href|source|video",re.I)
class Scan(HTMLParser):
 def __init__(self):
  super().__init__(convert_charrefs=True)
  self.tags=[]; self.scripts=[]; self.curr=None; self.ln=0; self.stack=[]
 def handle_starttag(self,tag,attrs):
  d=dict(attrs)
  if tag in ("a","iframe","video","source","li","input","button") or ("play" in (" ".join(d.get("class",""))).lower()):
   self.tags.append((tag,d))
  if tag=="script":
   self.curr={"src":d.get("src"),"data":""}
 def handle_endtag(self,tag):
  if tag=="script" and self.curr:
   self.scripts.append(self.curr);self.curr=None
 def handle_data(self,data):
  if self.curr is not None:self.curr["data"]+=data

def inspect(name,url):
 req=urllib.request.Request(url,headers={"User-Agent":"Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/131.0.0.0 Mobile Safari/537.36"})
 try:
  with urllib.request.urlopen(req,timeout=20) as f: content=f.read(1000000).decode("utf-8","replace");final_url=f.url
 except Exception as e:return name,{"error":str(e)[:170]}
 parsed=Scan()
 try:parsed.feed(content)
 except Exception as e:return name,{"parse_error":str(e)}
 # show patterns, lengths and script src, without raw media tokens
 interesting=[]
 for tag,attrs in parsed.tags:
  keys={k:v for k,v in attrs.items() if k in ("class","id","href","data-url","data-href","src","data-src","onclick","data-episode","data-play","title","type","name")}
  if tag=="a" and not KW.search(" ".join(str(v) for v in keys.values())):continue
  if tag=="a" and len(interesting)>=40:continue
  interesting.append({"tag":tag,"attrs":{k:(v[:150] if k not in ("href","src","data-url","data-src") else _safe_url(v)) for k,v in keys.items()}})
 def safe_script(s):
  snippets=[]
  for match in KW.finditer(s):
   st=max(0,match.start()-80);en=min(len(s),match.end()+100)
   chunk=s[st:en]
   # hide literal arbitrary links and long tokens
   chunk=re.sub(r'https?:(?://|\\/\\/)[^\s"\'<>]{30,}',lambda m:_safe_url(m.group())+" [url]",chunk)
   chunk=re.sub(r'[A-Za-z0-9_-]{65,}',"<long-string>",chunk)
   snippets.append(" ".join(chunk.split())[:230])
   if len(snippets)>=10:break
  return snippets
 scripts=[{"src":_safe_url(x["src"]) if x["src"] else None,"chars":len(x["data"]),"snippet":safe_script(x["data"])} for x in parsed.scripts if x["src"] or KW.search(x["data"])]
 return name,{"inline_play_logic": [re.sub(r"https?://[^\\s\\\"\\\x27<>]+","[remote-url]", x["data"][:2400]) for x in parsed.scripts if x["data"] and ("s_video_plays" in x["data"] or "play_changed(" in x["data"])][:2], "status":200,"final_url":final_url,"html_len":len(content),"tag_count":len(parsed.tags),"tags":interesting[:55],"script_count":len(parsed.scripts),"scripts":scripts[:28],
    "html_player_hits":len(KW.findall(content))}
def _safe_url(x):
 if not x:return ""
 try:
  u=urlparse(x)
  if u.scheme in ("https","http"):
   return f"{u.scheme}://{u.hostname}{u.path[:95]}"+("?[query]" if u.query else "")
 except Exception:pass
 return x[:140]


# Inspect only metadata, not full media URLs, of the site's public episode response.
def check_nnyy_public_episode():
 url="https://nnyy.in/_gp/20096378/ep26"
 req=urllib.request.Request(url,headers={
   "User-Agent":"Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/131.0.0.0 Mobile Safari/537.36",
   "Accept":"application/json,text/javascript,*/*;q=0.1",
   "Referer":"https://nnyy.in/dongman/20096378.html",
 })
 try:
  with urllib.request.urlopen(req,timeout=15) as resp:
   raw=resp.read(150000)
   data=json.loads(raw)
   tracks=data.get("video_plays",[]) if isinstance(data,dict) else []
   print("=== nnyy public episode API ===")

   if tracks and isinstance(tracks[0],dict) and isinstance(tracks[0].get("play_data"),str):
    media_url=tracks[0]["play_data"]
    # Verify only manifest header; don't download the video itself or expose signed URLs.
    try:
     media_req=urllib.request.Request(media_url,headers={
       "User-Agent":"Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/131.0.0.0 Mobile Safari/537.36",
       "Accept":"application/vnd.apple.mpegurl,application/x-mpegURL,*/*",
     })
     with urllib.request.urlopen(media_req,timeout=12) as manifest:
      prefix=manifest.read(131072)
      content=prefix.decode("utf-8","replace")
      tags=[line.strip().split(":",1)[0] for line in content.splitlines() if line.startswith("#")]
      if "#EXT-X-STREAM-INF:" in content:
       from urllib.parse import urljoin
       variant_lines=content.splitlines()
       variant_uri=None
       for index,line in enumerate(variant_lines):
        if line.startswith("#EXT-X-STREAM-INF:"):
         variant_uri=next((x.strip() for x in variant_lines[index+1:] if x.strip() and not x.startswith("#")),None)
         break
       if variant_uri:
        child_url=urljoin(media_url,variant_uri)
        try:
         child_req=urllib.request.Request(child_url,headers={
           "User-Agent":"Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/131.0.0.0 Mobile Safari/537.36",
           "Accept":"application/vnd.apple.mpegurl,application/x-mpegURL,*/*",
         })
         with urllib.request.urlopen(child_req,timeout=15) as child_resp:
          child_content=child_resp.read(150000).decode("utf-8","replace")
          child_tags=[line.strip().split(":",1)[0] for line in child_content.splitlines() if line.startswith("#")]
          uris=[line.strip() for line in child_content.splitlines() if line.strip() and not line.startswith("#")]
          print("Nnyy child HLS:",json.dumps({
            "http_status":child_resp.status,
            "has_endlist":"#EXT-X-ENDLIST" in child_tags,
            "has_key":"#EXT-X-KEY" in child_tags or "#EXT-X-SESSION-KEY" in child_tags,
            "key_methods": sorted(set(re.search(r"METHOD=([^,]+)", line).group(1) for line in child_content.splitlines() if line.startswith("#EXT-X-KEY:") and re.search(r"METHOD=([^,]+)", line)))[:5],
            "has_map":"#EXT-X-MAP" in child_tags,
            "has_byterange":"#EXT-X-BYTERANGE" in child_tags,
            "segment_count":len(uris),
            "extensions":list(sorted(set(urlparse(urljoin(child_url,x)).path.rsplit(".",1)[-1] for x in uris)))[:8],
            "tags":sorted(set(child_tags))[:25],
          },ensure_ascii=False))
        except Exception as e:print("Nnyy child HLS:",str(e)[:150])
      print("Nnyy HLS manifest check:",
       json.dumps({"status":manifest.status,
        "content_type":manifest.headers.get("Content-Type",""),
        "is_hls":prefix.lstrip().startswith(b"#EXTM3U"),
        "host":urlparse(media_url).hostname,
        "has_key":any(x.startswith("#EXT-X-KEY") or x.startswith("#EXT-X-SESSION-KEY") for x in tags),
        "has_variants":"#EXT-X-STREAM-INF" in tags,
        "has_map":"#EXT-X-MAP" in tags,
        "has_byterange":"#EXT-X-BYTERANGE" in tags,
        "has_endlist":"#EXT-X-ENDLIST" in tags,
        "segment_count":sum(1 for line in content.splitlines() if line and not line.startswith("#")),
        "tags":sorted(set(tags))[:30]},ensure_ascii=False))
    except Exception as e: print("Nnyy HLS manifest check:",str(e)[:180])
   print(json.dumps({"http_status":resp.status,"keys":list(data)[:20] if isinstance(data,dict) else [],
     "tracks":len(tracks),"track_keys":[list(x) for x in tracks[:3] if isinstance(x,dict)],
     "track_data_types":[
       {"type":type(x.get("play_data")).__name__,
        "media_extension":(urlparse(x.get("play_data","")).path.rsplit(".",1)[-1] if isinstance(x.get("play_data"),str) else None),
        "media_host":(urlparse(x.get("play_data","")).hostname if isinstance(x.get("play_data"),str) else None)
       } for x in tracks[:3] if isinstance(x,dict)
     ]},ensure_ascii=False))
 except Exception as e:print("=== nnyy public episode API ===\n"+str(e)[:250])


def inspect_dm84_episode_links():
 req=urllib.request.Request("https://dm84.tv/v/1773.html",headers={
   "User-Agent":"Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/131.0.0.0 Mobile Safari/537.36",
 })
 try:
  with urllib.request.urlopen(req,timeout=15) as resp:
   body=resp.read(1000000).decode("utf-8","replace")
  class Links(HTMLParser):
   def __init__(self): super().__init__();self.a=[];self.current=None
   def handle_starttag(self,tag,attrs):
    if tag=="a":self.current={"attrs":dict(attrs),"text":""}
   def handle_data(self,text):
    if self.current is not None:self.current["text"]+=text
   def handle_endtag(self,tag):
    if tag=="a" and self.current is not None:
     self.a.append(self.current);self.current=None
  parser=Links();parser.feed(body)
  opts=[]
  for link in parser.a:
   href=link["attrs"].get("href","")
   if href and re.search(r"(?:/p/|/play/|episode|1773)",href,re.I):
    opts.append({"href":href[:170],"text":link["text"].strip()[:45],"class":link["attrs"].get("class","")[:80]})
  print("=== dm84 episode links ===")
  print(json.dumps(opts[:35],ensure_ascii=False))
  # No automatic stream fetch here; need separately confirm a legitimate public play page.
 except Exception as e:print("=== dm84 episode links ===",str(e)[:130])

check_nnyy_public_episode()
inspect_dm84_episode_links()
with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
 for name,record in sorted(pool.map(lambda x:inspect(*x),PAGES.items())):
  print("\n=== "+name+" ===")
  print(json.dumps(record,ensure_ascii=False,indent=2)[:12500])
