#!/usr/bin/env python3
"""Inspect public iKanBot page and first-party JavaScript player behavior.
No tokens/cookies, stream URLs, or copyrighted media are downloaded/logged.
"""
import json, re, urllib.request, urllib.error
from html.parser import HTMLParser
from urllib.parse import urljoin
START="https://www1.ikanbot.com/play/381898"
class Elements(HTMLParser):
 def __init__(self):super().__init__();self.els=[]
 def handle_starttag(self,tag,attrs):
  a=dict(attrs)
  if tag in ("video","source","iframe","script","input"):
   allowed=["id","name","class","type","src","content","data-src","data-id","data-url"]
   if tag=="script" and not a.get("src"):return
   self.els.append((tag,{k:v[:130] for k,v in a.items() if k in allowed and k not in ("content","data-url")}))
def get(url, max_bytes=400000):
 req=urllib.request.Request(url,headers={"User-Agent":"Mozilla/5.0 (Linux; Android 14; Mobile) AppleWebKit/537.36 Chrome/131.0.0.0 Mobile Safari/537.36","Accept":"text/html,application/javascript,*/*;q=0.5"})
 with urllib.request.urlopen(req,timeout=16) as f:
  return {"url":f.url,"status":f.status,"xframe":f.headers.get("X-Frame-Options"),"csp":f.headers.get("Content-Security-Policy"),"type":f.headers.get("Content-Type"),"body":f.read(max_bytes).decode("utf-8","replace")}
def keys_script(s):
 pats=["fetch(","$.ajax","$.post","$.get","XMLHttpRequest","episode","playlist","play_url","src(","source(","current_id","e_token","mtype","iframe","new videojs","videojs(","player.src","play_new"]
 return {k:len(re.findall(re.escape(k),s,re.I)) for k in pats if k.lower() in s.lower()}
def snippets(s):
 pats=["fetch(","$.ajax","$.post","$.get(","XMLHttpRequest","e_token","current_id","mtype","videojs(","player.src","setSrc","/api/","/play/","iframe"]
 out=[]
 for x in pats:
  at=s.lower().find(x.lower())
  if at<0:continue
  t=s[max(0,at-80):min(len(s),at+200)]
  # Don't expose tokens or stream URLs
  t=re.sub(r"https?://[^\s'\"<>]+","[https-url]",t)
  t=re.sub(r"([A-Za-z0-9]{48,})","[long-value]",t)
  out.append({"match":x,"snippet":" ".join(t.split())[:260]})
 return out[:18]
result={}
try:
 page=get(START)
 parser=Elements();parser.feed(page["body"])
 result["page"]={k:v for k,v in page.items() if k!="body"}
 result["page"]["html_bytes"]=len(page["body"]);result["page"]["elements"]=parser.els[:40]
 result["page"]["js_keys"]=keys_script(page["body"])
except Exception as e:result["page"]={"error":str(e)}
try:
 script=get("https://www1.ikanbot.com/resources/play_new.js?v=1.64")
 result["script"]={k:v for k,v in script.items() if k!="body"}
 result["script"]["bytes"]=len(script["body"])
 result["script"]["keys"]=keys_script(script["body"])
 result["script"]["snippets"]=snippets(script["body"])
except Exception as e:result["script"]={"error":str(e)}
print(json.dumps(result,ensure_ascii=False,indent=2))
