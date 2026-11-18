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
  type: "mp4" | "webm" | "hls" | "embed";
};

export type HubDownload = {
  id: string;
  title: string;
  filename: string;
  bytes: number;
  contentType: string;
};

export type HubPendingDownload = {
  id: string;
  key: string;
  title: string;
  sourceUrl: string;
  kind: "hls" | "mp4" | "webm";
  received: number;
  total: number | null;
  completedParts: number | null;
  totalParts: number | null;
};

const KEY = "myvideo-multi-sources-v1";

/**
 * Sites that do not satisfy the required trio (search + in-app playback +
 * permitted offline downloads) are not offered as active sources.
 * iKanBot is excluded until those capabilities are implemented and verified.
 * Filter persisted V2 configs too, otherwise existing installations keep it.
 */
export function isExcludedHubSource(source: Pick<HubSource, "id" | "homepage">): boolean {
  // A site enters the active catalog only after search + in-app playback
  // + offline download have been verified. Keep old saved sites hidden.
  return source.id !== "nnyy" && source.id !== "kubb";
}

const defaultSites: HubSource[] = videoSourceCandidates.map((s) => ({
  id: s.id,
  name: s.name,
  homepage: s.homepage,
  searchTemplate: s.id === "kubb"
    ? "https://api.kubb.tv/api/List/GetTitleGetData?SearchCriteria={query}"
    : "https://nnyy.in/so?q={query}",
  enabled: true,
  configured: true,
}));

export function readHubSources(): HubSource[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) || "null");
    if (Array.isArray(parsed)) {
      const arr = parsed.filter((s): s is HubSource => !!s && typeof s.id === "string" &&
        typeof s.name === "string" && typeof s.homepage === "string" &&
        typeof s.searchTemplate === "string" && typeof s.enabled === "boolean" &&
        !isExcludedHubSource(s));
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

export function makeHubSource(_url: string): HubSource {
  throw new Error("新增网站需要先完成搜索、APP 内播放及离线下载验证，暂不可直接添加");
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

/** Restrict hits to candidate work/detail routes instead of site navigation links. */
function looksLikeWorkLink(sourceId: string, pathname: string): boolean {
  const routes: Record<string, RegExp> = {
    "nnyy": /^\/(?:dongman|dianying|dianshiju|zongyi|movie|tv|voddetail)\/[^/]+\.(?:html?)$/i,
    "bandai-channel": /^\/titles\/\d+(?:\/|$)/i,
    "dm84": /^\/v\/\d+(?:\.html)?$/i,
    "ikanbot": /^\/(?:play|movie|video|detail)\//i,
    "iyf": /^\/(?:play|detail|video|movie)\//i,
    "zhaojixie": /^\/(?:video|play|jilupian|archives)\//i,
  };
  const pattern = routes[sourceId];
  return pattern ? pattern.test(pathname) : !/^\/(?:tag|category|search|index|user)\//i.test(pathname);
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
    if (!looksLikeWorkLink(source.id, parsed.pathname)) continue;
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
  if (isExcludedHubSource(source)) throw new Error("该网站尚未通过完整功能验收，已从聚合搜索排除");
  if (source.id === "kubb") {
    return { items: await searchKubb(keywordNormalized(query)), template: source.searchTemplate, discovered: false };
  }
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


/** MyVideo category browsing uses public Nnyy listings, not hardcoded titles. */
export const NNY_CATEGORIES = [
  { id: "dongman", label: "动漫" },
  { id: "dianying", label: "电影" },
  { id: "dianshiju", label: "电视剧" },
  { id: "zongyi", label: "综艺" },
] as const;
export type NnyCategory = typeof NNY_CATEGORIES[number]["id"];
export type NnySort = "latest" | "popular";
export type NnyGenre = { label: string; url: string };
export type NnyCatalog = { results: HubResult[]; genres: NnyGenre[] };

export function parseNnyCatalog(html: string, pageUrl: string, category: NnyCategory): NnyCatalog {
  const doc = new DOMParser().parseFromString(html, "text/html");
  doc.querySelectorAll("script,style,noscript").forEach((x) => x.remove());
  const titles: HubResult[] = [];
  const visited = new Set<string>();
  const route = new RegExp("^/" + category + "/[0-9]+\\.html$","i");
  for (const a of doc.querySelectorAll("a[href]")) {
    const href = absoluteUrl(pageUrl, a.getAttribute("href") || "");
    if (!href) continue;
    const u = new URL(href);
    if (u.hostname !== "nnyy.in" || !route.test(u.pathname) || visited.has(u.pathname)) continue;
    // The card image/heading may be in a sibling link; prefer the heading when present.
    const card = a.closest("li,.item,.video,.module-item,.movie,.vod-item,.list-item") || a;
    const heading = card.querySelector("h2,h3,h4,.title,.name");
    const title = (heading?.textContent || a.getAttribute("title") || a.querySelector("img")?.getAttribute("alt") ||
      a.textContent || "").trim().replace(/\s+/g, " ").slice(0, 125);
    if (title.length < 2 || TITLE_SKIP.test(title)) continue;
    const poster = normalizePoster(a, pageUrl);
    titles.push({ id: "nnyy|" + u.origin + u.pathname, sourceId: "nnyy",
      sourceName: "努努影院", title, url: u.origin + u.pathname,
      ...(poster ? { poster } : {}) });
    visited.add(u.pathname);
    if (titles.length >= 48) break;
  }
  const genres: NnyGenre[] = [{ label: "全部", url: "https://nnyy.in/" + category + "/" }];
  const seen = new Set(["全部"]);
  for (const a of doc.querySelectorAll("a[href]")) {
    const href = absoluteUrl(pageUrl, a.getAttribute("href") || "");
    if (!href) continue;
    const u = new URL(href);
    if (u.hostname !== "nnyy.in" || u.pathname !== "/" + category + "/" ||
        !u.searchParams.has("genre")) continue;
    const label = (a.textContent || "").trim().replace(/\s+/g, "");
    if (!label || label.length > 12 || seen.has(label)) continue;
    genres.push({ label, url: u.origin + u.pathname + "?genre=" +
      encodeURIComponent(u.searchParams.get("genre") || "") });
    seen.add(label);
    if (genres.length >= 24) break;
  }
  return { results: titles, genres };
}

export async function fetchNnyCatalog(
  category: NnyCategory, sort: NnySort, genre?: string,
): Promise<NnyCatalog> {
  if (!NNY_CATEGORIES.some((entry) => entry.id === category)) throw new Error("无效分类");
  const url = new URL("https://nnyy.in/" + category + "/");
  if (genre) {
    const g = new URL(genre);
    if (g.protocol !== "https:" || g.hostname !== "nnyy.in" ||
        g.pathname !== url.pathname || !g.searchParams.has("genre")) {
      throw new Error("无效类别过滤地址");
    }
    url.searchParams.set("genre", g.searchParams.get("genre") || "");
  }
  if (sort === "popular") url.searchParams.set("ob", "click");
  return parseNnyCatalog(await fetchHubHtml(url.href), url.href, category);
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

/**
 * DM84 exposes an ordinary third-party iframe player on its episode pages.
 * Embed only the observed, explicitly allowlisted player host. This does not
 * extract its streams or circumvent its access controls; the provider may
 * decline to be framed on Android.
 */
export function findAllowedSiteEmbeds(html: string, pageUrl: string): HubMedia[] {
  const source = new URL(pageUrl);
  if (source.protocol !== "https:" || source.hostname !== "dm84.tv" ||
      !/^\/p\/\d+-\d+-\d+\.html$/.test(source.pathname)) {
    return [];
  }
  const doc = new DOMParser().parseFromString(html, "text/html");
  const found: HubMedia[] = [];
  for (const iframe of doc.querySelectorAll("iframe[src]")) {
    const target = absoluteUrl(pageUrl, iframe.getAttribute("src") || "");
    if (!target) continue;
    const uri = new URL(target);
    if (uri.protocol === "https:" && uri.hostname === "hhjx.hhplayer.com") {
      found.push({ type: "embed", url: uri.href });
    }
  }
  return found.slice(0, 3);
}

export const listHubDownloads = () => invoke<HubDownload[]>("hub_list_downloads");
export const listHubPending = () => invoke<HubPendingDownload[]>("hub_list_pending_downloads");
export const pauseHubDownload = () => invoke<void>("hub_pause_download");
export const discardHubPending = (id: string) =>
  invoke<void>("hub_discard_pending_download", { id });
export const downloadHubMedia = (url: string, title: string, key: string) =>
  invoke<HubDownload>("hub_download_media", { url, title, key });
export const downloadHubHls = (url: string, title: string, key: string) =>
  invoke<HubDownload>("hub_download_hls", { url, title, key });
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
  // Nnyy exposes episode buttons instead of ordinary <a href> episode pages.
  // Its public UI selects an episode through GET /_gp/<work-id>/<ep-slug>.
  if (sourceId === "nnyy") {
    const workId = /\/(?:dongman|dianying|dianshiju|zongyi|movie|tv|voddetail)\/(\d+)\.html(?:[?#]|$)/i.exec(pageUrl)?.[1];
    if (workId) {
      const document = new DOMParser().parseFromString(html, "text/html");
      const buttons = [...document.querySelectorAll("[ep_slug]")];
      const out: HubEpisode[] = [];
      const seen = new Set<string>();
      for (const button of buttons) {
        const slug = button.getAttribute("ep_slug") || "";
        if (!/^[a-z0-9_-]{1,48}$/i.test(slug) || seen.has(slug)) continue;
        seen.add(slug);
        out.push({
          title: (button.textContent || button.getAttribute("title") || slug).trim().slice(0, 65) || slug,
          url: new URL("/_gp/" + workId + "/" + slug, pageUrl).href,
        });
      }
      if (out.length) return out;
    }
  }

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


/** Public ordinary episode selector of nnyy.in; not a DRM or login bypass. */
export async function fetchNnyyEpisode(episodeUrl: string): Promise<HubMedia[]> {
  const target = new URL(episodeUrl);
  if (target.protocol !== "https:" || target.hostname !== "nnyy.in" ||
      !/^\/_gp\/[0-9]+\/[a-zA-Z0-9_-]{1,48}$/.test(target.pathname)) {
    throw new Error("无效的努努影院选集地址");
  }
  const payload = await invoke<unknown>("hub_fetch_public_json", { url: target.href });
  if (!payload || typeof payload !== "object") {
    throw new Error("选集接口未返回播放线路");
  }
  const tracks = (payload as { video_plays?: unknown }).video_plays;
  if (!Array.isArray(tracks)) {
    throw new Error("选集接口缺少 video_plays");
  }
  const seen = new Set<string>();
  const result: HubMedia[] = [];
  for (const item of tracks) {
    if (!item || typeof item !== "object") continue;
    const value = (item as { play_data?: unknown }).play_data;
    if (typeof value !== "string" || value.length > 2500) continue;
    try {
      const mediaUrl = new URL(value);
      if (mediaUrl.protocol !== "https:" && mediaUrl.protocol !== "http:") continue;
      if (seen.has(mediaUrl.href)) continue;
      const kind = /\.m3u8(?:[?#]|$)/i.test(mediaUrl.href)
        ? "hls" : /\.mp4(?:[?#]|$)/i.test(mediaUrl.href)
          ? "mp4" : /\.webm(?:[?#]|$)/i.test(mediaUrl.href)
            ? "webm" : null;
      if (!kind) continue;
      seen.add(mediaUrl.href);
      result.push({ url: mediaUrl.href, type: kind });
    } catch { /* ignore unusable playback records */ }
  }
  return result;
}


/** Kubb TV public, anonymous GET endpoints (https://api.kubb.tv).
 * The adapter uses only content explicitly returned for public viewing, not
 * account tokens, restricted VIP streams or encryption workarounds.
 */
function keywordNormalized(query: string): string {
  return query.trim().slice(0, 100);
}
type KubbRecord = Record<string, unknown>;
function kubbObject(value: unknown): KubbRecord | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as KubbRecord : null;
}
function kubbText(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}
function kubbMediaKey(url: string): string {
  const target = new URL(url);
  if (target.protocol !== "https:" || target.hostname !== "m.kubb.tv") {
    throw new Error("无效的 Kubb 播放地址");
  }
  const mediaKey = /^\/play\/([A-Za-z0-9_-]{6,50})\/?$/.exec(target.pathname)?.[1];
  if (!mediaKey) throw new Error("Kubb 播放地址缺少作品标识");
  return mediaKey;
}
async function kubbGet(endpoint: "GetTitleGetData" | "VideoDetails" | "VideoChooseGather" | "getPlayData", params: Record<string,string>): Promise<KubbRecord> {
  const path = endpoint === "GetTitleGetData" ? "/api/List/GetTitleGetData"
    : "/api/Video/" + endpoint;
  const target = new URL("https://api.kubb.tv" + path);
  for (const [name,value] of Object.entries(params)) target.searchParams.set(name,value);
  const json: unknown = await invoke("hub_fetch_public_json", { url: target.href });
  const payload = kubbObject(json);
  if (!payload || payload.ret !== 200) throw new Error("Kubb 公开接口未返回成功结果");
  return kubbObject(payload.data) || {};
}
function kubbEpisodes(items: unknown): KubbRecord[] {
  if (!Array.isArray(items)) return [];
  return items.map(kubbObject).filter((item): item is KubbRecord => item !== null);
}
export async function searchKubb(query: string): Promise<HubResult[]> {
  const search = keywordNormalized(query);
  if (!search) return [];
  const data = await kubbGet("GetTitleGetData", { SearchCriteria: search });
  const out: HubResult[] = [];
  const known = new Set<string>();
  for (const item of kubbEpisodes(data.list)) {
    const nested = kubbEpisodes(item.episodes);
    const mediaKey = kubbText(item.mediaKey) || nested.map((e) => kubbText(e.mediaKey)).find(Boolean) || "";
    const title = kubbText(item.title);
    if (!/^[A-Za-z0-9_-]{6,50}$/.test(mediaKey) || !title || known.has(mediaKey)) continue;
    const url = "https://m.kubb.tv/play/" + mediaKey;
    known.add(mediaKey);
    const poster = kubbText(item.coverImgUrl);
    out.push({ id: "kubb|" + mediaKey, sourceId: "kubb", sourceName: "Kubb TV 酷播",
      title, url, ...(poster.startsWith("https://") ? { poster } : {}) });
    if (out.length >= 60) break;
  }
  return out;
}
function playableKubbLink(value: unknown): HubMedia | null {
  const item = kubbObject(value);
  if (!item || item.isVip !== false || item.isLimit === true) return null;
  const raw = kubbText(item.mediaUrl);
  let target: URL;
  try { target = new URL(raw); } catch { return null; }
  if (target.protocol !== "https:") return null;
  const type = /\.m3u8(?:[?#]|$)/i.test(target.href) ? "hls"
    : /\.mp4(?:[?#]|$)/i.test(target.href) ? "mp4"
    : /\.webm(?:[?#]|$)/i.test(target.href) ? "webm" : null;
  return type ? { url: target.href, type } : null;
}
export async function fetchKubbDetail(url: string): Promise<{ title: string; episodes: HubEpisode[] }> {
  const mediaKey = kubbMediaKey(url);
  const [details, tracks] = await Promise.all([
    kubbGet("VideoDetails", { mediaKey }),
    kubbGet("VideoChooseGather", { mediaKey }),
  ]);
  const detail = kubbObject(details.detailInfo);
  const title = kubbText(detail?.title) || "Kubb 视频";
  const episodes: HubEpisode[] = [];
  const seen = new Set<string>();
  for (const item of kubbEpisodes(tracks.list)) {
    const episodeKey = kubbText(item.episodeKey);
    // Directory results legitimately omit mediaUrl. Fetch actual playback URL only on selection.
    if (item.isVip === true || item.isLimit === true ||
        !/^[A-Za-z0-9_-]{6,50}$/.test(episodeKey) || seen.has(episodeKey)) continue;
    const variantKey = kubbText(item.mediaKey) || mediaKey;
    if (!/^[A-Za-z0-9_-]{6,50}$/.test(variantKey)) continue;
    const label = kubbText(item.episodeTitle) || kubbText(item.resolutionDes) ||
      kubbText(item.title) || "免费线路";
    episodes.push({
      title: label,
      url: "https://m.kubb.tv/play/" + variantKey + "?id=" + encodeURIComponent(episodeKey),
    });
    seen.add(episodeKey);
    if (episodes.length >= 180) break;
  }
  return { title, episodes };
}
export async function fetchKubbEpisode(episodeUrl: string): Promise<HubMedia[]> {
  const mediaKey = kubbMediaKey(episodeUrl);
  const episodeKey = new URL(episodeUrl).searchParams.get("id") || "";
  if (!/^[A-Za-z0-9_-]{6,50}$/.test(episodeKey)) {
    throw new Error("Kubb 选集缺少有效 episodeKey");
  }
  const data = await kubbGet("getPlayData", { mediaKey });
  for (const item of kubbEpisodes(data.list)) {
    if (kubbText(item.episodeKey) !== episodeKey) continue;
    const link = playableKubbLink(item);
    if (!link) throw new Error("这条线路需要 VIP 权限或未提供公开播放地址");
    return [link];
  }
  throw new Error("Kubb 公开接口中没有找到对应选集");
}
