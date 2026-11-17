#!/usr/bin/env python3
"""Bounded public webpage connectivity/metadata checks; does not stream video.

The resulting report distinguishes reachability, metadata, search, and playback.
A successful HTML GET does not establish playable media in an Android WebView.
"""
import concurrent.futures
import datetime
import html
import json
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from html.parser import HTMLParser
from pathlib import Path

SITES = [
  {"id":"nnyy", "home":"https://nnyy.in/", "example":"https://nnyy.in/dongman/20096378.html", "query":"犬夜叉", "search":None},
  {"id":"bandai-channel", "home":"https://www.b-ch.com/", "example":"https://www.b-ch.com/titles/2277/001", "query":"犬夜叉", "search":"https://www.b-ch.com/search/?search_txt={query}"},
  {"id":"dm84", "home":"https://dm84.tv/", "example":"https://dm84.tv/v/1773.html", "query":"犬夜叉", "search":None},
  {"id":"iyf", "home":"https://m.iyf.tv/", "example":"https://m.iyf.tv/play/qmM3z64VxiV?id=zXfHcY2NIAU", "query":"犬夜叉", "search":None},
  {"id":"zhaojixie", "home":"https://zhaojixie.com/", "example":"https://zhaojixie.com/category/jilupian.html", "query":"纪录片", "search":None},
  {"id":"ikanbot", "home":"https://www1.ikanbot.com/", "example":"https://www1.ikanbot.com/play/381898", "query":"大明", "search":"https://www1.ikanbot.com/search?q={query}"},
]

MAX_BYTES = 1200000
TIMEOUT = 14

class Page(HTMLParser):
  def __init__(self):
    super().__init__(convert_charrefs=True)
    self.title_on = False
    self.title = []
    self.links = []
    self.current_a = None
    self.forms = []
    self.current_form = None
    self.media_tags = 0
    self.media_urls = []
  def handle_starttag(self, tag, attrs):
    attrs = dict(attrs)
    if tag == "title": self.title_on = True
    if tag == "a":
      self.current_a = {"href":attrs.get("href",""),"label":attrs.get("title","")}
    if tag == "form":
      self.current_form = {"action":attrs.get("action",""),"method":attrs.get("method","GET").upper(),"inputs":[]}
    if tag == "input" and self.current_form is not None:
      self.current_form["inputs"].append({"name":attrs.get("name",""),"type":attrs.get("type","text")})
    if tag in ("video","source"):
      self.media_tags += 1
      if attrs.get("src"): self.media_urls.append(attrs["src"][:300])
    if tag == "meta" and attrs.get("property") in ("og:video","og:video:url"):
      if attrs.get("content"): self.media_urls.append(attrs["content"][:300])
  def handle_endtag(self,tag):
    if tag=="title": self.title_on=False
    if tag=="a" and self.current_a is not None:
      self.links.append(self.current_a)
      self.current_a=None
    if tag=="form" and self.current_form is not None:
      self.forms.append(self.current_form)
      self.current_form=None
  def handle_data(self,data):
    if self.title_on: self.title.append(data)
    if self.current_a is not None:
      self.current_a["label"] += data
  def title_text(self):
    return re.sub(r"\s+"," ","".join(self.title)).strip()[:160]

def fetch(url):
  if not url.startswith("https://"): return {"ok":False,"error":"HTTPS only"}
  req=urllib.request.Request(url,headers={
    "User-Agent":"Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/131.0.0.0 Mobile Safari/537.36",
    "Accept":"text/html,application/xhtml+xml;q=0.9,*/*;q=0.5",
  })
  try:
    with urllib.request.urlopen(req,timeout=TIMEOUT) as resp:
      status=resp.status
      content_type=resp.headers.get("Content-Type","")
      raw=resp.read(MAX_BYTES+1)
      if len(raw)>MAX_BYTES: return {"ok":False,"status":status,"error":"HTML exceeded 1.2MB check limit"}
      try: decoded=raw.decode(resp.headers.get_content_charset() or "utf-8",errors="replace")
      except Exception: decoded=raw.decode("utf-8",errors="replace")
      parser=Page()
      parser.feed(decoded)
      return {"ok": 200<=status<400 and "html" in content_type.lower(),
        "status":status,"final_url":resp.url,"content_type":content_type,
        "bytes":len(raw),"title":parser.title_text(),"parser":parser}
  except urllib.error.HTTPError as e:
    return {"ok":False,"status":e.code,"error":"HTTP "+str(e.code)}
  except Exception as e:
    return {"ok":False,"error":str(e)[:240]}

def discover_form(page,home):
  if not page.get("ok"): return None
  parser=page["parser"]
  for form in parser.forms:
    if form["method"]!="GET": continue
    inp=next((i for i in form["inputs"] if i["name"] and (
        i["type"]=="search" or i["name"].lower() in ("q","s","search","search_txt","keyword","query","wd")
      )),None)
    if not inp: continue
    dest=urllib.parse.urljoin(home,form["action"])
    if urllib.parse.urlsplit(dest).netloc!=urllib.parse.urlsplit(home).netloc: continue
    parts=urllib.parse.urlsplit(dest)
    args=urllib.parse.parse_qsl(parts.query,keep_blank_values=True)
    args.append((inp["name"],"{query}"))
    return urllib.parse.urlunsplit((parts.scheme,parts.netloc,parts.path,urllib.parse.urlencode(args),"")).replace("%7Bquery%7D","{query}")
  return None

def summarize_fetch(page):
  return {k: v for k,v in page.items() if k!="parser"}

def check_site(site):
  home=fetch(site["home"])
  example=fetch(site["example"])
  template=site["search"] or discover_form(home,site["home"])
  search=None
  if template:
    search=fetch(template.replace("{query}",urllib.parse.quote(site["query"])))
  pattern={
    "nnyy":r"/(?:dongman|dianying|dianshiju|voddetail)/",
    "bandai-channel":r"/titles/\d+",
    "dm84":r"/v/\d+",
    "iyf":r"/(?:play|detail)/",
    "zhaojixie":r"/(?:video|jilupian)/",
    "ikanbot":r"/(?:play|movie|video|detail)/",
  }[site["id"]]
  def links(page):
    if not page or not page.get("ok"): return []
    base=page["final_url"]
    return [{"title":re.sub(r"\s+"," ",x["label"]).strip()[:85],
             "url":urllib.parse.urljoin(base,x["href"])}
       for x in page["parser"].links
       if re.search(pattern,urllib.parse.urljoin(base,x["href"]),re.I)
       and re.sub(r"\s+","",x["label"]).strip()]
  search_hits=links(search)
  episode_candidates=links(example)
  example_links=example.get("parser").links if example.get("ok") else []
  # Any published direct media URL is only a hint, not proof of valid playback.
  medias=(example["parser"].media_urls if example.get("ok") else [])
  return {
    "id":site["id"],"homepage":summarize_fetch(home),
    "example":summarize_fetch(example),
    "search_template":template,"search_query":site["query"],
    "search":summarize_fetch(search) if search is not None else None,
    "search_title_links":search_hits[:8], "search_title_link_count":len(search_hits),
    "work_link_count":len(episode_candidates),
    "work_link_examples":episode_candidates[:8],
    "html_media_tag_count":example["parser"].media_tags if example.get("ok") else 0,
    "html_media_url_samples":[x for x in medias if x.startswith("https://")][:4],
    "html_link_count":len(example_links),
    "android_playback_verified":False,
    "notes":"HTML metadata checks only; no media streamed, no DRM/authorization bypass.",
  }

def main():
  results=[]
  with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
    futures={pool.submit(check_site,site):site["id"] for site in SITES}
    for fut in concurrent.futures.as_completed(futures):
      try: results.append(fut.result())
      except Exception as e: results.append({"id":futures[fut],"error":str(e)})
  results.sort(key=lambda x:[s["id"] for s in SITES].index(x["id"]))
  output={"generated_at":datetime.datetime.now(datetime.timezone.utc).isoformat(),"checks":results}
  Path("reports").mkdir(exist_ok=True)
  Path("reports/site-smoke.json").write_text(json.dumps(output,ensure_ascii=False,indent=2),encoding="utf-8")
  lines=["# Six-site public HTTP verification","",f"Generated {output['generated_at']}","",
         "| Site | Homepage | Example | Search | Candidate title links | Direct media tags | Android playback |",
         "|---|---|---|---|---:|---:|---|"]
  for r in results:
    def status(page):
      if page is None: return "no public GET form"
      if page.get("ok"): return "HTTP "+str(page.get("status",200))
      return str(page.get("status",page.get("error","failed")))[:48].replace("|","/")
    lines.append("| "+r["id"]+" | "+status(r.get("homepage"))+" | "+status(r.get("example"))+" | "+status(r.get("search"))+
       " | "+str(r.get("search_title_link_count","?"))+" | "+str(r.get("html_media_tag_count","?"))+" | Not tested |")
    lines.append("")
    lines.append("- "+r["id"]+" search template: "+str(r.get("search_template")))
    if r.get("search_title_links"):
      lines.append("  - Example result: "+r["search_title_links"][0]["title"]+" ("+r["search_title_links"][0]["url"]+")")
    if r.get("example",{}).get("title"): lines.append("  - Example page title: "+r["example"]["title"])
    if r.get("search",{}):
      ss=r["search"]
      if ss and ss.get("error"): lines.append("  - Search error: "+ss["error"].replace("|","/"))
  lines.extend(["","This reports public HTML reachability and candidate markup only.",
                "No in-app WebView playback, user login, downloadable media, or Android device has been tested."])
  Path("reports/site-smoke.md").write_text("\n".join(lines)+"\n",encoding="utf-8")
  print("\n".join(lines))
  # Explicitly succeed even when websites block access: record honest results rather than hide them.
  return 0

if __name__=="__main__": sys.exit(main())
