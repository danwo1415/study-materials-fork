import { useEffect, useRef, useState } from "react";
import Hls from "hls.js";
import { listen } from "@tauri-apps/api/event";
// Standalone playback origin; there is no Huangguo API import in MyVideo.
const LOCAL = /Android|Windows/i.test(navigator.userAgent) ? "http://hg.localhost" : "hg://localhost";
import {
  type HubDownload, type HubMedia, type HubResult, type HubSource, type HubEpisode,
  readHubSources, saveHubSources, searchPublicSite,
  fetchHubHtml, findDirectMedia, findAllowedSiteEmbeds, findPublicEpisodes, fetchNnyyEpisode, listHubDownloads,
  downloadHubMedia, downloadHubHls, deleteHubDownload,
  NNY_CATEGORIES, fetchNnyCatalog, type NnyCategory, type NnySort, type NnyGenre,
  fetchKubbDetail, fetchKubbEpisode,
} from "../sourceHub";

type SourceStatus = { state: "searching" | "ok" | "empty" | "error"; detail?: string };

type AdSpan = { start: number; end: number };
const AD_SECONDS = 6;
const AD_STORAGE_PREFIX = "myvideo-ad-skips-v1:";

function readAdSkips(key: string): AdSpan[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(AD_STORAGE_PREFIX + key) || "[]");
    if (!Array.isArray(value)) return [];
    return value.filter((item): item is AdSpan =>
      !!item && typeof item === "object" &&
      typeof item.start === "number" && Number.isFinite(item.start) && item.start >= 0 &&
      typeof item.end === "number" && Number.isFinite(item.end) &&
      item.end > item.start && item.end - item.start <= 30
    ).slice(0, 30);
  } catch { return []; }
}
function adSkipLabel(seconds: number) {
  const mins = Math.floor(seconds / 60);
  return mins + ":" + String(Math.floor(seconds % 60)).padStart(2, "0");
}


function EmbeddedVideo({ media, adMarkerKey }: {
  media: HubMedia | { url: string; type: "offline" | "offline-hls" };
  adMarkerKey?: string;
}) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [adSpans, setAdSpans] = useState<AdSpan[]>(() => adMarkerKey ? readAdSkips(adMarkerKey) : []);
  const [adNotice, setAdNotice] = useState("");
  const spansRef = useRef(adSpans);
  useEffect(() => { spansRef.current = adSpans; }, [adSpans]);
  useEffect(() => {
    if (adMarkerKey) localStorage.setItem(AD_STORAGE_PREFIX + adMarkerKey, JSON.stringify(adSpans));
  }, [adMarkerKey, adSpans]);

  const markAd = () => {
    const video = videoRef.current;
    if (!video || !adMarkerKey || !Number.isFinite(video.currentTime)) return;
    const start = video.currentTime;
    const end = start + AD_SECONDS;
    // Overlapping manual marks describe one advertisement, not multiple skips.
    const next = [...adSpans.filter((span) => span.end < start || span.start > end), { start, end }]
      .sort((a, b) => a.start - b.start).slice(-30);
    setAdSpans(next);
    video.currentTime = Number.isFinite(video.duration)
      ? Math.min(Math.max(0, video.duration - 0.1), end + 0.1) : end + 0.1;
    setAdNotice("已记住 " + adSkipLabel(start) + " 起的 " + AD_SECONDS + " 秒，下次自动跳过");
  };
  const clearAdMarks = () => {
    setAdSpans([]);
    setAdNotice("已清除这集的广告时间记录");
  };

  useEffect(() => {
    const video = videoRef.current;
    if (!video || media.type === "embed") return;
    const skipMarkedAd = () => {
      const time = video.currentTime;
      const matched = spansRef.current.find((span) => time >= span.start && time < span.end);
      if (matched) {
        const jump = Number.isFinite(video.duration)
          ? Math.min(Math.max(0, video.duration - 0.1), matched.end + 0.1)
          : matched.end + 0.1;
        if (jump > time) video.currentTime = jump;
      }
    };
    if (adMarkerKey) video.addEventListener("timeupdate", skipMarkedAd);
    let hls: Hls | undefined;
    if ((media.type === "hls" || media.type === "offline-hls") && Hls.isSupported()) {
      hls = new Hls({ enableWorker: false });
      hls.loadSource(media.type === "offline-hls" ? media.url : LOCAL + "/hls?u=" + encodeURIComponent(media.url));
      hls.attachMedia(video);
    } else if (media.type === "hls" || media.type === "offline-hls") {
      video.src = media.type === "offline-hls" ? media.url : LOCAL + "/hls?u=" + encodeURIComponent(media.url);
    } else if (media.type === "offline") {
      video.src = media.url;
    } else {
      video.src = LOCAL + "/seg?u=" + encodeURIComponent(media.url);
    }
    return () => {
      video.removeEventListener("timeupdate", skipMarkedAd);
      hls?.destroy();
      video.removeAttribute("src");
      video.load();
    };
  }, [media.url, media.type, adMarkerKey]);
  if (media.type === "embed") {
    return <div className="space-y-2">
      <iframe src={media.url} title="视频站公开嵌入播放器" sandbox="allow-scripts allow-same-origin allow-forms allow-presentation"
        referrerPolicy="strict-origin-when-cross-origin" allow="autoplay; fullscreen; picture-in-picture"
        allowFullScreen className="w-full aspect-video bg-black rounded-(--r-md)" />
      <div className="text-[11px] text-mut">此播放线路由第三方嵌入页面提供。若网站禁止嵌入，请使用下方「在原网站打开当前剧集」。</div>
    </div>;
  }
  return <div className="space-y-2">
    <video ref={videoRef} controls playsInline preload="metadata"
      className="w-full max-h-[270px] bg-black rounded-(--r-md)" />
    {adMarkerKey && <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-2">
        <button type="button" onClick={markAd}
          className="px-3 py-2 rounded-(--r-ctl) bg-s3 text-on text-[12px] pressable">
          广告开始时点：跳过并记住 6 秒
        </button>
        {adSpans.length > 0 && <button type="button" onClick={clearAdMarks}
          className="px-3 py-2 rounded-(--r-ctl) bg-s3 text-on text-[12px] pressable">
          清除广告标记
        </button>}
      </div>
      <div className="text-[11px] text-mut">
        已记住 {adSpans.length} 段；只按你标记的时间自动跳过，不会删除视频数据。
      </div>
      {adNotice && <div className="text-[11px] text-mut" role="status">{adNotice}</div>}
    </div>}
  </div>;
}

export default function SourceHub() {
  const [sources, setSources] = useState<HubSource[]>(readHubSources);
  const [query, setQuery] = useState("");
  const [chosen, setChosen] = useState("all");
  const [results, setResults] = useState<HubResult[]>([]);
  const [statuses, setStatuses] = useState<Record<string, SourceStatus>>({});
  const [searching, setSearching] = useState(false);
  const [selected, setSelected] = useState<HubResult | null>(null);
  const [media, setMedia] = useState<HubMedia[]>([]);
  const [episodes, setEpisodes] = useState<HubEpisode[]>([]);
  const [activeEpisode, setActiveEpisode] = useState<HubEpisode | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [message, setMessage] = useState("");
  const [downloads, setDownloads] = useState<HubDownload[]>([]);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState("");
  const [view, setView] = useState<"categories" | "search" | "downloads">("categories");
  const [category, setCategory] = useState<NnyCategory>("dongman");
  const [sort, setSort] = useState<NnySort>("latest");
  const [genre, setGenre] = useState("");
  const [genres, setGenres] = useState<NnyGenre[]>([]);
  const [catalogItems, setCatalogItems] = useState<HubResult[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState("");
  const catalogRequest = useRef(0);
  const searchCounter = useRef(0);
  const hubRoot = useRef<HTMLDivElement>(null);

  const scrollToHubTop = () => {
    const viewport = hubRoot.current?.closest(".scroll-y");
    if (viewport instanceof HTMLElement) viewport.scrollTop = 0;
  };

  useEffect(() => { saveHubSources(sources); }, [sources]);
  useEffect(() => {
    void listHubDownloads().then(setDownloads).catch(() => {});
    let unlisten: (() => void) | undefined;
    void listen<{ received: number; total: number | null; completedParts?: number; totalParts?: number }>("hub-download-progress", (event) => {
      const { received, total, completedParts, totalParts } = event.payload;
      setDownloadProgress(total ? Math.round(received / total * 100) + "%"
        : (completedParts && totalParts ? completedParts + "/" + totalParts + " 片 · " : "") + (received / 1048576).toFixed(1) + " MB");
    }).then((fn) => { unlisten = fn; });
    return () => unlisten?.();
  }, []);

  const loadCatalog = async (cat: NnyCategory, order: NnySort, filter: string) => {
    const req = ++catalogRequest.current;
    setCatalogLoading(true);
    setCatalogError("");
    try {
      const result = await fetchNnyCatalog(cat, order, filter || undefined);
      if (req !== catalogRequest.current) return;
      setCatalogItems(result.results);
      if (!filter) setGenres(result.genres);
      if (!result.results.length) setCatalogError("该分类没有解析到作品。可刷新重试，或使用搜索功能。");
    } catch (error) {
      if (req === catalogRequest.current) {
        setCatalogItems([]);
        setCatalogError("分类加载失败：" + (error as Error).message);
      }
    } finally { if (req === catalogRequest.current) setCatalogLoading(false); }
  };

  useEffect(() => { void loadCatalog(category, sort, genre); }, [category, sort, genre]);

  const updateSource = (id: string, patch: Partial<HubSource>) =>
    setSources((prev) => prev.map((s) => s.id === id ? { ...s, ...patch } : s));

  const searchAll = async (keyword: string) => {
    const k = keyword.trim();
    if (!k) { setMessage("请输入搜索关键词"); return; }
    const selectedSources = sources.filter((s) => s.enabled && (chosen === "all" || s.id === chosen));
    if (!selectedSources.length) { setMessage("请先启用至少一个网站"); return; }
    const seq = ++searchCounter.current;
    setResults([]);
    setSearching(true);
    setSelected(null);
    setMessage("");
    setStatuses(Object.fromEntries(selectedSources.map((s) => [s.id, { state: "searching" }])));
    await Promise.all(selectedSources.map(async (source) => {
      try {
        const search = await searchPublicSite(source, k);
        const items = search.items;
        if (searchCounter.current !== seq) return;
        if (search.discovered) updateSource(source.id, { searchTemplate: search.template, configured: true });
        setResults((prev) => {
          const ids = new Set(prev.map((r) => r.id));
          return [...prev, ...items.filter((r) => !ids.has(r.id))];
        });
        setStatuses((prev) => ({ ...prev, [source.id]: {
          state: items.length ? "ok" : "empty",
          detail: items.length ? items.length + " 个可能结果" : "没有匹配项，或该站不支持当前搜索规则",
        }}));
      } catch (e) {
        if (searchCounter.current !== seq) return;
        setStatuses((prev) => ({ ...prev, [source.id]: { state: "error", detail: (e as Error).message } }));
      }
    }));
    if (searchCounter.current === seq) setSearching(false);
  };

  const openResult = async (result: HubResult) => {
    setSelected(result);
    scrollToHubTop();
    setMedia([]);
    setEpisodes([]);
    setActiveEpisode(null);
    setDetailLoading(true);
    setMessage("");
    try {
      if (result.sourceId === "kubb") {
        const detail = await fetchKubbDetail(result.url);
        setMedia([]);
        setEpisodes(detail.episodes);
        if (!detail.episodes.length) setMessage("Kubb 暂时没有可供匿名播放的免费线路；VIP 内容不会绕过授权。");
      } else {
        const html = await fetchHubHtml(result.url);
        setMedia(findDirectMedia(html, result.url));
        setEpisodes(findPublicEpisodes(html, result.url, result.sourceId));
      }
    } catch (e) { setMessage((e as Error).message); }
    finally { setDetailLoading(false); }
  };

  const openEpisode = async (ep: HubEpisode) => {
    setActiveEpisode(ep);
    setMedia([]);
    setDetailLoading(true);
    setMessage("");
    try {
      if (selected?.sourceId === "kubb") {
        setMedia(await fetchKubbEpisode(ep.url));
      } else if (selected?.sourceId === "nnyy" && new URL(ep.url).pathname.startsWith("/_gp/")) {
        const streams = await fetchNnyyEpisode(ep.url);
        if (!streams.length) setMessage("该剧集没有公开可识别的播放地址");
        setMedia(streams);
      } else {
        const html = await fetchHubHtml(ep.url);
        setMedia([...findDirectMedia(html, ep.url), ...findAllowedSiteEmbeds(html, ep.url)]);
      }
    } catch (e) { setMessage((e as Error).message); }
    finally { setDetailLoading(false); }
  };

  const download = async (item: HubMedia) => {
    if (!selected || item.type === "embed") return;
    setDownloading(true);
    setDownloadProgress("开始下载");
    setMessage("");
    try {
      const fullTitle = activeEpisode ? selected.title + " - " + activeEpisode.title : selected.title;
      const saved = item.type === "hls"
        ? await downloadHubHls(item.url, fullTitle)
        : await downloadHubMedia(item.url, fullTitle);
      setDownloads((prev) => [...prev, saved]);
      setMessage("已下载到 App 私有目录，可在「离线」查看");
    } catch (e) { setMessage((e as Error).message); }
    finally { setDownloading(false); }
  };

  const removeDownload = async (id: string) => {
    try {
      await deleteHubDownload(id);
      setDownloads((prev) => prev.filter((x) => x.id !== id));
    } catch (e) { setMessage((e as Error).message); }
  };

  const closeDetail = () => {
    setSelected(null);
    setActiveEpisode(null);
    setEpisodes([]);
    setMedia([]);
    setMessage("");
    scrollToHubTop();
  };

  const btn = "px-3 py-2 rounded-(--r-ctl) bg-s3 text-on text-[12px] pressable";
  const field = "w-full min-w-0 p-2.5 rounded-(--r-sm) bg-s3 text-on text-[12px] outline-none border border-white/10";
  return <div ref={hubRoot} className="flex flex-col gap-3 text-on">
    <div className="flex gap-2 flex-wrap">
      <button className={btn + (view === "categories" ? " bg-primary text-on-primary" : "")} onClick={() => setView("categories")}>分类</button>
      <button className={btn + (view === "search" ? " bg-primary text-on-primary" : "")} onClick={() => setView("search")}>聚合搜索</button>
      <button className={btn + (view === "downloads" ? " bg-primary text-on-primary" : "")} onClick={() => setView("downloads")}>离线 ({downloads.length})</button>
    </div>
    {view === "categories" && <div className="space-y-3">
      <div className="text-[13px] font-semibold">努努影院 · 分类浏览</div>
      <div className="flex gap-2 overflow-x-auto pb-1">
        {NNY_CATEGORIES.map((c) => <button key={c.id} type="button"
          onClick={() => { setCategory(c.id); setGenre(""); setGenres([]); setCatalogItems([]); }}
          className={btn + " shrink-0" + (category === c.id ? " bg-primary text-on-primary" : "")}>{c.label}</button>)}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <button type="button" className={btn + (sort === "latest" ? " bg-primary text-on-primary" : "")}
          onClick={() => setSort("latest")}>最近更新</button>
        <button type="button" className={btn + (sort === "popular" ? " bg-primary text-on-primary" : "")}
          onClick={() => setSort("popular")}>按人气</button>
        <button type="button" className={btn} onClick={() => void loadCatalog(category, sort, genre)}>刷新</button>
      </div>
      {genres.length > 1 && <div className="flex gap-2 overflow-x-auto pb-1" aria-label="类型筛选">
        {genres.map((g) => <button key={g.url} type="button" onClick={() => setGenre(g.label === "全部" ? "" : g.url)}
          className={btn + " shrink-0" + ((genre === g.url || (!genre && g.label === "全部")) ? " bg-primary text-on-primary" : "")}>{g.label}</button>)}
      </div>}
      {catalogLoading && <p className="text-[12px] text-mut" role="status">正在加载真实分类作品…</p>}
      {catalogError && <p className="text-[12px] text-mut" role="status">{catalogError}</p>}
      <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
        {catalogItems.map((item) => <button type="button" key={item.id}
          className="min-w-0 text-left rounded-(--r-md) bg-s2 overflow-hidden pressable"
          onClick={() => { setView("search"); void openResult(item); }}>
          <div className="aspect-[3/4] bg-s3 flex items-center justify-center overflow-hidden">
            {item.poster ? <img src={LOCAL + "/seg?u=" + encodeURIComponent(item.poster)} alt="" loading="lazy"
              className="w-full h-full object-cover" onError={(e) => { e.currentTarget.style.display = "none"; }} />
              : <span className="text-2xl text-mut">▶</span>}
          </div>
          <div className="px-2 py-2 text-[12px] font-medium break-words line-clamp-2 min-h-11">{item.title}</div>
        </button>)}
      </div>
      <div className="text-[11px] text-mut">内容来自努努影院的公开分类页面。作品是否可播放仍以实际剧集线路为准。</div>
    </div>}
    {view === "search" && <>
      {!selected && <>
      <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); void searchAll(query); }}>
        <input className={field + " flex-1"} value={query} onChange={(e) => setQuery(e.target.value)}
          placeholder="输入任意动漫、电影或纪录片名称" />
        <button type="submit" className="px-3 rounded-(--r-sm) bg-primary text-on-primary text-[12px]"
          disabled={searching}>{searching ? "搜索中" : "搜索"}</button>
      </form>
      <select className={field} value={chosen} onChange={(e) => setChosen(e.target.value)}>
        <option value="all">全部已启用网站</option>
        {sources.filter((s) => s.enabled).map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
      </select>
      <div className="text-[11px] text-mut">仅搜索已验证的视频源。Kubb TV 提供公开免费线路；VIP 清晰度不支持绕过授权。</div>
      {Object.keys(statuses).length > 0 && <div className="rounded-(--r-md) bg-s2 p-3 space-y-1">
        {sources.filter((s) => !!statuses[s.id]).map((s) => <div key={s.id} className="text-[11px] text-mut">
          <span className="text-on">{s.name}</span> · {statuses[s.id].state === "searching" ? "搜索中" :
          statuses[s.id].state === "ok" ? "完成" : statuses[s.id].state === "empty" ? "暂无结果" : "失败"}
          {statuses[s.id].detail ? " · " + statuses[s.id].detail : ""}
        </div>)}
      </div>}
      {results.length > 0 && <div className="space-y-2">
        <div className="text-[12px] font-semibold">找到 {results.length} 个候选结果</div>
        {results.map((r) => <button key={r.id} onClick={() => { void openResult(r); }}
          className="w-full text-left rounded-(--r-md) bg-s2 p-3 flex gap-3 items-center">
          <div className="w-12 h-16 bg-s3 shrink-0 rounded-(--r-sm) flex items-center justify-center">▶</div>
          <div className="min-w-0">
            <div className="font-semibold text-[13px] break-words">{r.title}</div>
            <div className="text-[11px] text-mut mt-1">{r.sourceName}</div>
          </div>
        </button>)}
      </div>}
      </>}
      {selected && <div className="rounded-(--r-md) bg-s2 p-3 space-y-3">
        <button type="button" className={btn + " self-start"} onClick={closeDetail}>← 返回搜索结果</button>
        <div className="font-semibold text-[17px]">{selected.title}</div>
        <div className="text-[11px] text-mut">{selected.sourceName}{selected.sourceId === "nnyy" ? " · 已支持公开选集接口" : ""}</div>
        {selected.sourceId === "ikanbot" && <div className="rounded-(--r-sm) bg-s3 p-3 space-y-2">
          <div className="text-[12px] font-semibold">iKanBot 的播放需要网站动态授权</div>
          <div className="text-[11px] text-mut">已确认其网页播放器使用带 token 的接口加载视频，并设置 SAMEORIGIN 禁止跨网站嵌入。此版本可展示搜索结果，但尚未获得能在本 App 内稳定播放的授权接口。</div>

        </div>}

        {episodes.length > 0 && <div className="space-y-2">
          <div className="text-[12px] font-semibold">公开剧集 / 播放入口（{episodes.length}）</div>
          <div className="flex flex-wrap gap-2 max-h-44 overflow-auto">{episodes.map((ep) =>
            <button key={ep.url} className={btn + (activeEpisode?.url === ep.url ? " bg-primary text-on-primary" : "")}
              onClick={() => { void openEpisode(ep); }}>{ep.title}</button>)}</div>
        </div>}
        {episodes.length > 0 && !activeEpisode && <div className="text-[11px] text-mut">请先选择上面的剧集，才能加载对应的视频线路。</div>}
        {detailLoading ? <div className="text-[13px] rounded-(--r-sm) bg-s3 p-3" role="status">正在获取剧集及播放线路，请查看网络连接…</div> : media.length ? media.map((m, i) =>
          <div key={m.url} className="space-y-2 border-t border-white/10 pt-2">
            <div className="text-xs">播放地址 {i + 1} · {m.type.toUpperCase()}</div>
            <EmbeddedVideo key={m.url + "|" + (activeEpisode?.url || "")} media={m}
              adMarkerKey={selected.sourceId === "nnyy" ? "nnyy|" + (activeEpisode ? selected.title + " - " + activeEpisode.title : selected.title) : undefined} />
            {(m.type === "mp4" || m.type === "webm" || m.type === "hls") && <button className={btn} disabled={downloading}
              onClick={() => { void download(m); }}>{downloading ? "下载中 " + downloadProgress : "下载此视频"}</button>}
            {m.type === "hls" && <div className="text-[11px] text-mut">仅下载无加密、完整且可合法保存的 HLS 视频。加密或特殊分片会显示不支持。</div>}
          </div>) : <div className="text-[11px] text-mut">未识别到公开的直接视频文件；该站可能使用脚本播放器、登录或受保护的视频接口。</div>}

      </div>}
      {!selected && !searching && !results.length && query.trim() && Object.keys(statuses).length > 0 &&
        <p className="text-[12px] text-mut">未找到候选结果。可到「管理网站」中调整搜索规则，或使用原网站搜索。</p>}
    </>}
    {view === "downloads" && <div className="space-y-2">
      <div className="text-[12px] text-mut">MP4、WebM 与普通 HLS 均保存在 MyVideo 私有目录。卸载 MyVideo 可能删除这些文件。</div>
      {downloads.length ? downloads.map((d) => <div key={d.id} className="rounded-(--r-md) bg-s2 p-3 space-y-2">
        <div className="text-[13px] font-semibold">{d.title}</div>
        <div className="text-[11px] text-mut">{(d.bytes / 1048576).toFixed(1)} MB</div>
        <EmbeddedVideo adMarkerKey={d.contentType === "application/vnd.apple.mpegurl" ? "nnyy|" + d.title : undefined}
          media={d.contentType === "application/vnd.apple.mpegurl"
          ? { url: LOCAL + "/offline-hls?id=" + encodeURIComponent(d.id) + "&file=index.m3u8", type: "offline-hls" }
          : { url: LOCAL + "/offline?id=" + encodeURIComponent(d.id), type: "offline" }} />
        <button className={btn} onClick={() => { void removeDownload(d.id); }}>删除本地文件</button>
      </div>) : <div className="text-[12px] text-mut">还没有离线视频。先搜索作品并选择允许下载的播放线路。</div>}
    </div>}
    {message && <p className="text-[12px] rounded-(--r-md) bg-s3 p-3 break-words">{message}</p>}
  </div>;
}
