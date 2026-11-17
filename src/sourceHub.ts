import { invoke } from "@tauri-apps/api/core";
import { videoSourceCandidates } from "./videoSourceCatalog";

export type HubSource = {
  id: string;
  name: string;
  homepage: string;
  /** Search page URL containing {query} placeholder; never assume each site shares an API. */
  searchTemplate: string;
  enabled: boolean;
  configured: boolean;
};

export type HubResult = {
  id: string;
  sourceId: string;
  sourceName: string;
  title: string;
  url: string;
  poster?: string;
};

export type HubMedia = {
  url: string;
  type: "mp4" | "webm" | "hls";
};

export type HubDownload = {
  id: string;
  title: string;
  filename: string;
  bytes: number;
  contentType: string;
};

const KEY = "hghub-multi-sources-v2";
const defaultSites: HubSource[] = videoSourceCandidates.map((s) => ({
  id: s.id,
  name: s.name,
  homepage: s.homepage,
  // These templates are hypotheses for user testing, not validated site endpoints.
  searchTemplate: s.id === "ikanbot"
    ? "https://www1.ikanbot.com/search?q={query}"
    : s.id === "bandai-channel"
      ? "https://www.b-ch.com/search/?search_txt={query}"
      : new URL("?s={query}", s.homepage).href.replace("%7Bquery%7D", "{query}"),
  enabled: true,
  configured: s.id === "ikanbot" || s.id === "bandai-channel",
}));

export function readHubSources(): HubSource[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) || "null");
    if (Array.isArray(parsed)) {
      const arr = parsed.filter((s): s is HubSource => !!s && typeof s.id === "string" &&
        typeof s.name === "string" && typeof s.homepage === "string" &&
        typeof s.searchTemplate === "string" && typeof s.enabled === "boolean");
      const ids = new Set(arr.map((x) => x.id));
      return [
        ...arr.map((x) => {
          const builtin = defaultSites.find((d) => d.id === x.id);
          // Migrate old untested generic guesses to verified search paths.
          if (builtin?.configured && !x.configured &&
              (x.searchTemplate.endsWith("/?s={query}") ||
               x.searchTemplate.endsWith("?s={query}"))) {
            return { ...x, searchTemplate: builtin.searchTemplate, configured: true };
          }
          return x;
        }),
        ...defaultSites.filter((x) => !ids.has(x.id)),
      ];
    }
  } catch { /* corrupted local settings: use defaults */ }
  return defaultSites;
}

export function saveHubSources(sources: HubSource[]): void {
  localStorage.setItem(KEY, JSON.stringify(sources));
}

function requirePublicHttps(raw: string): URL {
  const u = new URL(raw);
  if (u.protocol !== "https:" || u.username || u.password || !u.hostname.includes(".") ||
      /^(localhost|.*\.localhost|.*\.local)$/i.test(u.hostname) ||
      /^(?:\d{1,3}\.){3}\d{1,3}$/.test(u.hostname) || u.hostname.includes("[")) {
    throw new Error("只支持公开 HTTPS 网站域名，不支持本地、内网地址或包含密码的网址");
  }
  return u;
}

export function makeHubSource(url: string): HubSource {
  const u = requirePublicHttps(url.trim());
  const home = u.origin + "/";
  return {
    id: "custom-" + Date.now().toString(36),
    name: u.hostname,
    homepage: home,
    searchTemplate: home + "?s={query}",
    enabled: true,
    configured: false,
  };
}

export function validateSource(source: HubSource): void {
  requirePublicHttps(source.homepage);
  if (!source.searchTemplate.includes("{query}")) {
    throw new Error("搜索规则必须包含 {query} 作为关键词占位符");
  }
  const u = requirePublicHttps(source.searchTemplate.replace("{query}", "test"));
  if (u.origin !== new URL(source.homepage).origin) {
    throw new Error("搜索规则必须与网站主页使用相同域名");
  }
}

function searchUrl(source: HubSource, query: string): string {
  validateSource(source);
  return source.searchTemplate.replaceAll("{query}", encodeURIComponent(query));
}

export async function fetchHubHtml(url: string): Promise<string> {
  return invoke<string>("hub_fetch_html", { url });
}

const SKIP = /^(javascript:|data:|mailto:|tel:|#)/i;
const TITLE_SKIP = /^(首页|主页|搜索|登录|注册|更多|下一页|上一页|关于|联系我们|帮助|会员|查看全部|下载|播放)$/;
const MEDIA = /\.(mp4|webm|m3u8)(?:[?#]|$)/i;
function absoluteUrl(base: string, href: string): string | null {
  if (!href || SKIP.test(href)) return null;
  try {
    const url = new URL(href, base);
    if (!["http:", "https:"].includes(url.protocol)) return null;
    return url.href;
  } catch { return null; }
}

function normalizeTitle(node: Element): string {
  return (node.getAttribute("title") || node.getAttribute("aria-label") ||
      node.querySelector("img")?.getAttribute("alt") || node.textContent || "")
    .trim().replace(/\s+/g, " ").slice(0, 125);
}

function normalizePoster(a: Element, base: string): string | undefined {
  const img = a.querySelector("img") || a.closest("article,li,.item,.video,.module-item")?.querySelector("img");
  const url = img?.getAttribute("data-src") || img?.getAttribute("data-original") || img?.getAttribute("src");
  if (!url) return;
  return absoluteUrl(base, url) || undefined;
}

export function parsePublicSearch(html: string, pageUrl: string, source: HubSource, query: string): HubResult[] {
  const doc = new DOMParser().parseFromString(html, "text/html");
  for (const s of doc.querySelectorAll("script,style,noscript,nav,footer,header")) s.remove();
  const words = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  const candidates = Array.from(doc.querySelectorAll("main a[href], article a[href], .search a[href], .result a[href], .module a[href], .video a[href], .list a[href], .item a[href], a[href]"));
  const seen = new Set<string>();
  const results: HubResult[] = [];
  const homepageHost = new URL(source.homepage).hostname.replace(/^www\d*\./, "");
  for (const node of candidates) {
    const title = normalizeTitle(node);
    if (title.length < 2 || TITLE_SKIP.test(title) || title.length > 125) continue;
    const uri = absoluteUrl(pageUrl, node.getAttribute("href") || "");
    if (!uri) continue;
    const parsed = new URL(uri);
    if (parsed.hostname !== homepageHost && !parsed.hostname.endsWith("." + homepageHost)) continue;
    if (/\/(login|signup|register|user|account|category|tag|search)(\/|[?#]|$)/i.test(parsed.pathname)) continue;
    if (parsed.pathname === "/" && !parsed.search) continue;
    // Require a keyword match to avoid returning all navigation links when a site ignores a search template.
    let decodedPath = parsed.pathname;
    try { decodedPath = decodeURIComponent(parsed.pathname); } catch { /* ignore invalid escapes */ }
    const haystack = (title + " " + decodedPath).toLocaleLowerCase();
    if (!words.some((w) => haystack.includes(w))) continue;
    if (seen.has(uri)) continue;
    seen.add(uri);
    results.push({
      id: source.id + "|" + uri,
      sourceId: source.id,
      sourceName: source.name,
      title,
      url: uri,
      poster: normalizePoster(node, pageUrl),
    });
    if (results.length >= 60) break;
  }
  return results;
}

export async function searchPublicSite(source: HubSource, query: string): Promise<{
  items: HubResult[];
  template: string;
  discovered: boolean;
}> {
  let resolved = source;
  let discovered = false;
  if (!source.configured) {
    const found = await discoverSearchTemplate(source);
    if (!found) {
      throw new Error("未识别到可用的公开 GET 搜索表单；该网站需要专用适配器");
    }
    resolved = { ...source, searchTemplate: found, configured: true };
    discovered = true;
  }
  const pageUrl = searchUrl(resolved, query);
  const html = await fetchHubHtml(pageUrl);
  return {
    items: parsePublicSearch(html, pageUrl, resolved, query),
    template: resolved.searchTemplate,
    discovered,
  };
}

export function findDirectMedia(html: string, pageUrl: string): HubMedia[] {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const urls = new Set<string>();
  for (const item of doc.querySelectorAll("video[src],source[src],a[href]")) {
    const raw = item.getAttribute("src") || item.getAttribute("href") || "";
    const abs = absoluteUrl(pageUrl, raw);
    if (abs && MEDIA.test(abs)) urls.add(abs);
  }
  for (const item of doc.querySelectorAll('meta[property="og:video"],meta[property="og:video:url"],meta[name="twitter:player:stream"]')) {
    const abs = absoluteUrl(pageUrl, item.getAttribute("content") || "");
    if (abs && MEDIA.test(abs)) urls.add(abs);
  }
  // Some providers publish playable, unprotected assets through JSON-LD VideoObject.
  // Read only ordinary contentUrl; do not interpret scripts, tokens or DRM manifests.
  for (const script of doc.querySelectorAll('script[type="application/ld+json"]')) {
    try {
      const parsed: unknown = JSON.parse(script.textContent || "null");
      const visit = (obj: unknown, depth: number): void => {
        if (!obj || typeof obj !== "object" || depth > 4) return;
        if (Array.isArray(obj)) { obj.forEach((x) => visit(x, depth + 1)); return; }
        const data = obj as Record<string, unknown>;
        const type = data["@type"];
        const video = type === "VideoObject" || (Array.isArray(type) && type.includes("VideoObject"));
        if (video && typeof data.contentUrl === "string") {
          const abs = absoluteUrl(pageUrl, data.contentUrl);
          if (abs && MEDIA.test(abs)) urls.add(abs);
        }
        if (data["@graph"]) visit(data["@graph"], depth + 1);
      };
      visit(parsed, 0);
    } catch { /* ignore invalid JSON-LD */ }
  }
  return [...urls].slice(0, 15).map((url) => ({
    url,
    type: /\.m3u8(?:[?#]|$)/i.test(url) ? "hls" : /\.webm(?:[?#]|$)/i.test(url) ? "webm" : "mp4",
  }));
}

export const listHubDownloads = () => invoke<HubDownload[]>("hub_list_downloads");
export const downloadHubMedia = (url: string, title: string) =>
  invoke<HubDownload>("hub_download_media", { url, title });
export const deleteHubDownload = (id: string) => invoke<void>("hub_remove_download", { id });


/** Detect standard HTML GET search forms on an added website, without executing remote scripts. */
export async function discoverSearchTemplate(source: HubSource): Promise<string | null> {
  validateSource(source);
  const html = await fetchHubHtml(source.homepage);
  const doc = new DOMParser().parseFromString(html, "text/html");
  const forms = Array.from(doc.querySelectorAll("form"));
  for (const form of forms) {
    if ((form.getAttribute("method") || "GET").toUpperCase() !== "GET") continue;
    const input = Array.from(form.querySelectorAll<HTMLInputElement>("input[name]"))
      .find((el) => el.type === "search" || /^(s|q|query|keyword|search|wd|text)$/i.test(el.name));
    if (!input) continue;
    const action = new URL(form.getAttribute("action") || source.homepage, source.homepage);
    if (action.protocol !== "https:" || action.origin !== new URL(source.homepage).origin) continue;
    action.searchParams.set(input.name, "{query}");
    // URL serialization will encode braces; preserve a literal placeholder in our setting.
    return action.href.replace("%7Bquery%7D", "{query}");
  }
  return null;
}


/**
 * Extract public episode/detail links from a fetched work page.
 * This does not call private playback APIs or decode protected streams.
 */
export type HubEpisode = {
  title: string;
  url: string;
};
export function findPublicEpisodes(html: string, pageUrl: string, sourceId: string): HubEpisode[] {
  const doc = new DOMParser().parseFromString(html, "text/html");
  for (const n of doc.querySelectorAll("script,style,noscript,nav,footer,header")) n.remove();
  const origin = new URL(pageUrl).origin;
  const selectors: Record<string, string> = {
    nnyy: 'a[href*="/play/"],a[href*="/vodplay/"],a[href*="/dongman/"]',
    dm84: 'a[href*="/play/"],a[href*="/v/"],a[href*="/p/"]',
    ikanbot: 'a[href*="/play/"],a[href*="/video/"]',
    "bandai-channel": 'a[href*="/titles/"]',
    iyf: 'a[href*="/play/"],a[href*="/detail/"]',
    zhaojixie: 'a[href*="/play/"],a[href*="/video/"],a[href*="/jilupian/"]',
  };
  const candidates = doc.querySelectorAll((selectors[sourceId] || 'a[href*="/play/"]') + ',[class*="episode"] a[href],[class*="playlist"] a[href],[class*="play-list"] a[href]');
  const out: HubEpisode[] = [];
  const seen = new Set<string>([pageUrl.replace(/[#?].*$/, "")]);
  for (const a of candidates) {
    const raw = a.getAttribute("href") || "";
    const url = absoluteUrl(pageUrl, raw);
    if (!url) continue;
    const parsed = new URL(url);
    if (parsed.origin !== origin || !/^https?:$/.test(parsed.protocol)) continue;
    const canonical = url.replace(/#.*$/, "");
    if (seen.has(canonical)) continue;
    const label = normalizeTitle(a);
    if (!label || label.length > 65 || /^(首页|上一页|下一页|返回|更多|全部)$/i.test(label)) continue;
    seen.add(canonical);
    out.push({ title: label, url: canonical });
    if (out.length >= 150) break;
  }
  return out;
}
