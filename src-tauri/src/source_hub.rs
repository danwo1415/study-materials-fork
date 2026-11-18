//! Public-web HTML discovery and optional personal offline MP4/WebM downloads.
//! This module does not circumvent DRM, authentication, or site restrictions.
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{Emitter, Manager};

const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;
const MAX_DOWNLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const OFFLINE_CHUNK_BYTES: u64 = 1024 * 1024;
static DOWNLOADING: AtomicBool = AtomicBool::new(false);
#[path = "resumable.rs"]
mod resumable;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubDownload {
    id: String,
    title: String,
    filename: String,
    bytes: u64,
    content_type: String,
}

fn public_target(url: &str) -> Result<url::Url, String> {
    // Never use relaxed security for arbitrary user-added web sources.
    if !crate::network_policy::strict_security() {
        return Err("添加网站和下载需要开启设置中的严格安全模式".into());
    }
    crate::network_policy::parse_target_with_mode(url, true)
}

#[tauri::command]
pub async fn hub_fetch_html(url: String) -> Result<String, String> {
    public_target(&url)?;
    let mut response = crate::net::get(&url, Duration::from_secs(20), Some("text/html"), None).await?;
    if !response.status().is_success() {
        return Err(format!("网站返回 HTTP {}", response.status()));
    }
    let ctype = response.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !ctype.contains("text/html") && !ctype.contains("application/xhtml+xml") {
        return Err("该网址不是可解析的 HTML 页面".into());
    }
    if response.content_length().is_some_and(|n| n > MAX_HTML_BYTES as u64) {
        return Err("网站页面过大，无法安全解析".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > MAX_HTML_BYTES {
            return Err("页面超过 2MB 安全上限".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Read a bounded, public JSON endpoint used by a site's ordinary episode selector.
/// Requires strict network protections and does not forward login credentials.
#[tauri::command]
pub async fn hub_fetch_public_json(url: String) -> Result<serde_json::Value, String> {
    public_target(&url)?;
    let mut response = crate::net::get(
        &url,
        Duration::from_secs(18),
        Some("application/json,text/javascript;q=0.9,*/*;q=0.2"),
        None,
    ).await?;
    if !response.status().is_success() {
        return Err(format!("公开剧集接口返回 HTTP {}", response.status()));
    }
    if response.content_length().is_some_and(|n| n > MAX_HTML_BYTES as u64) {
        return Err("剧集数据超过 2MB 限制".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > MAX_HTML_BYTES {
            return Err("剧集 JSON 超过 2MB 限制".into());
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| "剧集接口没有返回有效 JSON 数据".to_string())
}

fn download_root(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?.join("hub-downloads");
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root)
}

fn index_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(download_root(app)?.join("index.json"))
}

fn load_index(app: &tauri::AppHandle) -> Result<Vec<HubDownload>, String> {
    let path = index_path(app)?;
    if !path.exists() { return Ok(Vec::new()); }
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

fn save_index(app: &tauri::AppHandle, items: &[HubDownload]) -> Result<(), String> {
    let path = index_path(app)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(items).map_err(|e| e.to_string())?)
        .and_then(|_| std::fs::rename(&tmp, &path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn hub_list_downloads(app: tauri::AppHandle) -> Result<Vec<HubDownload>, String> {
    load_index(&app)
}

struct DownloadSlot;
impl DownloadSlot {
    fn take() -> Result<Self, String> {
        if DOWNLOADING.swap(true, Ordering::SeqCst) {
            Err("已有一个下载任务正在运行，请完成后再试".into())
        } else { Ok(Self) }
    }
}
impl Drop for DownloadSlot {
    fn drop(&mut self) { DOWNLOADING.store(false, Ordering::SeqCst); }
}

/// Resumable MP4/WebM download. Stable identity is supplied by the episode.
#[tauri::command]
pub async fn hub_download_media(
    app: tauri::AppHandle,
    url: String,
    title: String,
    key: String,
) -> Result<HubDownload, String> {
    resumable::download_media(app, url, title, key).await
}

#[tauri::command]
pub fn hub_remove_download(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("无效的文件标识".into());
    }
    let mut items = load_index(&app)?;
    let item = items.iter().find(|it| it.id == id).cloned().ok_or("没有找到下载文件")?;
    if item.content_type == "application/vnd.apple.mpegurl" {
        std::fs::remove_dir_all(download_root(&app)?.join(&item.id)).map_err(|e| e.to_string())?;
    } else {
        std::fs::remove_file(download_root(&app)?.join(&item.filename)).map_err(|e| e.to_string())?;
    }
    items.retain(|it| it.id != id);
    save_index(&app, &items)
}

fn text_response(status: u16, body: &str) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder().status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Cache-Control", "no-store")
        .body(body.as_bytes().to_vec()).unwrap()
}

/// Read at most 1 MiB per request; handle the media element's Range seeks.
pub async fn serve_offline(
    app: &tauri::AppHandle,
    query: &str,
    range: Option<&str>,
) -> tauri::http::Response<Vec<u8>> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let id = url::form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == "id")
        .map(|(_, value)| value.into_owned()).unwrap_or_default();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return text_response(400, "invalid id");
    }
    let items = match load_index(app) {
        Ok(items) => items,
        Err(_) => return text_response(500, "offline index unavailable"),
    };
    let Some(item) = items.iter().find(|it| it.id == id) else {
        return text_response(404, "file not found");
    };
    let path = match download_root(app) {
        Ok(root) => root.join(&item.filename),
        Err(_) => return text_response(500, "offline directory unavailable"),
    };
    let result = async {
        let mut file = tokio::fs::File::open(path).await.map_err(|e| e.to_string())?;
        let size = file.metadata().await.map_err(|e| e.to_string())?.len();
        if size == 0 { return Err("empty file".to_string()); }
        let mut start = 0_u64;
        let mut requested_end = size - 1;
        if let Some(header) = range {
            let raw = header.strip_prefix("bytes=").ok_or("invalid range")?;
            let (first, last) = raw.split_once('-').ok_or("invalid range")?;
            if first.is_empty() { return Err("suffix ranges not supported".to_string()); }
            start = first.parse::<u64>().map_err(|_| "invalid range")?;
            if !last.is_empty() {
                requested_end = last.parse::<u64>().map_err(|_| "invalid range")?;
            }
        }
        if start >= size || requested_end < start { return Err("range beyond file".to_string()); }
        let end = requested_end.min(size - 1).min(start.saturating_add(OFFLINE_CHUNK_BYTES - 1));
        let mut bytes = vec![0u8; (end - start + 1) as usize];
        file.seek(std::io::SeekFrom::Start(start)).await.map_err(|e| e.to_string())?;
        file.read_exact(&mut bytes).await.map_err(|e| e.to_string())?;
        Ok::<_, String>((bytes, start, end, size))
    }.await;
    match result {
        Ok((bytes, start, end, size)) => {
            tauri::http::Response::builder()
                .status(206)
                .header("Content-Type", item.content_type.as_str())
                .header("Content-Length", bytes.len().to_string())
                .header("Accept-Ranges", "bytes")
                .header("Content-Range", format!("bytes {start}-{end}/{size}"))
                .header("Cache-Control", "private, no-store")
                .body(bytes).unwrap()
        }
        Err(_) => text_response(416, "invalid offline range or file"),
    }
}


// Offline HLS downloader. This is for unencrypted, finite VOD that the user
// is authorized to save. It does not fetch encryption keys or bypass DRM.
const MAX_HLS_ITEMS: usize = 3000;
const MAX_HLS_SEG_BYTES: u64 = 32 * 1024 * 1024;
const MAX_HLS_PLAYLIST_BYTES: usize = 1024 * 1024;

async fn hls_text(url: &str) -> Result<String, String> {
    public_target(url)?;
    let mut response = crate::net::get(
        url, Duration::from_secs(25),
        Some("application/vnd.apple.mpegurl,application/x-mpegURL,text/plain;q=0.8"),
        None,
    ).await?;
    if !response.status().is_success() {
        return Err(format!("HLS 播放列表 HTTP {}", response.status()));
    }
    if response.content_length().is_some_and(|n| n > MAX_HLS_PLAYLIST_BYTES as u64) {
        return Err("播放列表超过 1MB 限制".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > MAX_HLS_PLAYLIST_BYTES {
            return Err("播放列表超过 1MB 限制".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value = String::from_utf8(bytes).map_err(|_| "HLS 不是 UTF-8 文本")?;
    if !value.trim_start_matches('\u{feff}').starts_with("#EXTM3U") {
        return Err("服务器没有返回 HLS 播放列表".into());
    }
    Ok(value)
}

fn hls_uri(base: &str, raw: &str) -> Result<String, String> {
    let base = url::Url::parse(base).map_err(|_| "无效 HLS 基址")?;
    let resolved = base.join(raw.trim()).map_err(|_| "无效 HLS 分片地址")?;
    if resolved.scheme() != "https" {
        return Err("离线 HLS 只允许 HTTPS 分片地址".into());
    }
    public_target(resolved.as_str())?;
    Ok(resolved.to_string())
}

#[derive(Debug, Clone)]
struct HlsSnapshot {
    // Relative absolute HTTPS URL of segment plus its original EXTINF metadata.
    segments: Vec<(String, String)>,
    playlist: String,
}

fn hls_media_segments(source: &str, contents: &str, id: &str) -> Result<HlsSnapshot, String> {
    let contents = contents.trim_start_matches('\u{feff}');
    if !contents.starts_with("#EXTM3U") {
        return Err("不是 HLS 播放列表".into());
    }
    if contents.contains("#EXT-X-STREAM-INF") || contents.contains("#EXT-X-I-FRAME-STREAM-INF") {
        return Err("需要先解析 HLS 主播放列表".into());
    }
    if !contents.lines().any(|x| x.trim() == "#EXT-X-ENDLIST") {
        return Err("该 HLS 没有 ENDLIST，可能是直播或尚未结束的流，不支持离线下载".into());
    }
    let mut output = String::from("#EXTM3U\n");
    let mut items = Vec::new();
    let mut extinf_count = 0usize;
    for raw in contents.lines().skip(1) {
        let line = raw.trim();
        if line.is_empty() { continue; }
        // METHOD=NONE explicitly means subsequent segments are not encrypted.
        // Discard this directive in the offline copy; any other KEY stays blocked.
        if line.starts_with("#EXT-X-KEY:")
            && line.contains("METHOD=NONE")
            && !line.contains("URI=")
        {
            continue;
        }
        if line.starts_with("#EXT-X-KEY")
            || line.starts_with("#EXT-X-SESSION-KEY")
            || line.starts_with("#EXT-X-MAP")
            || line.starts_with("#EXT-X-BYTERANGE")
            || line.starts_with("#EXT-X-PART")
            || line.starts_with("#EXT-X-PRELOAD-HINT")
            || line.starts_with("#EXT-X-RENDITION-REPORT")
            || line.contains("URI=")
        {
            return Err("该 HLS 使用加密、字节范围或特殊分片结构；为保证离线文件可靠，此版本不支持".into());
        }
        if line.starts_with('#') {
            if line.starts_with("#EXTINF:") { extinf_count += 1; }
            // Drop any metadata which may contain remote URLs or dynamic hints.
            if line.starts_with("#EXT-X-DATERANGE") || line.starts_with("#EXT-X-START") { continue; }
            output.push_str(line);
            output.push('\n');
            continue;
        }
        if items.len() >= MAX_HLS_ITEMS {
            return Err("HLS 分片超过 3000 个，暂不支持".into());
        }
        let uri = hls_uri(source, line)?;
        // Limit this first version to ordinary MPEG-TS, never mistake fMP4 for TS.
        let path = url::Url::parse(&uri).map_err(|_| "分片地址无效")?.path().to_ascii_lowercase();
        if !path.ends_with(".ts") {
            return Err("此 HLS 使用非 TS 分片；暂不支持离线保存".into());
        }
        let filename = format!("seg-{:05}.ts", items.len());
        output.push_str(&format!("/offline-hls?id={id}&file={filename}\n"));
        items.push((uri, filename));
    }
    if items.is_empty() || extinf_count != items.len() {
        return Err("没有识别到完整的 HLS 视频分片列表".into());
    }
    Ok(HlsSnapshot { segments: items, playlist: output })
}

async fn resolve_hls_media(url: &str) -> Result<(String, String), String> {
    let mut next = url.to_string();
    for _ in 0..3 {
        let text = hls_text(&next).await?;
        if !text.lines().any(|x| x.trim().starts_with("#EXT-X-STREAM-INF:")) {
            return Ok((next, text));
        }
        let mut next_variant = None;
        let mut waiting = false;
        for line in text.lines() {
            let l = line.trim();
            if l.starts_with("#EXT-X-STREAM-INF:") { waiting = true; continue; }
            if waiting && !l.is_empty() && !l.starts_with('#') {
                next_variant = Some(hls_uri(&next, l)?);
                break;
            }
        }
        next = next_variant.ok_or("主播放列表没有可用清晰度线路")?;
    }
    Err("播放列表嵌套过多".into())
}

#[tauri::command]
pub async fn hub_download_hls(
    app: tauri::AppHandle,
    url: String,
    title: String,
    key: String,
) -> Result<HubDownload, String> {
    resumable::download_hls(app, url, title, key).await
}

#[tauri::command]
pub fn hub_list_pending_downloads(app: tauri::AppHandle) -> Result<Vec<resumable::PendingDownload>, String> {
    resumable::list_pending(&app)
}

#[tauri::command]
pub fn hub_discard_pending_download(app: tauri::AppHandle, id: String) -> Result<(), String> {
    resumable::discard_pending(&app, &id)
}

#[tauri::command]
pub fn hub_pause_download() {
    resumable::request_pause();
}

/// Serve only stored, indexed local media. Remote URLs are never accepted here.
pub async fn serve_hls_offline(
    app: &tauri::AppHandle,
    query: &str,
) -> tauri::http::Response<Vec<u8>> {
    let params: std::collections::HashMap<String, String> =
        url::form_urlencoded::parse(query.as_bytes()).into_owned().collect();
    let id = params.get("id").map(String::as_str).unwrap_or("");
    let name = params.get("file").map(String::as_str).unwrap_or("");
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return text_response(400, "invalid id");
    }
    let valid = name == "index.m3u8"
        || (name.starts_with("seg-") && name.ends_with(".ts")
            && name.len() == 12
            && name[4..9].chars().all(|c| c.is_ascii_digit()));
    if !valid { return text_response(400, "invalid filename"); }
    let items = match load_index(app) {
        Ok(items) => items,
        Err(_) => return text_response(500, "offline index unavailable"),
    };
    if !items.iter().any(|x| x.id == id && x.content_type == "application/vnd.apple.mpegurl") {
        return text_response(404, "offline HLS not found");
    }
    let path = match download_root(app) {
        Ok(root) => root.join(id).join(name),
        Err(_) => return text_response(500, "offline directory unavailable"),
    };
    let contents = match tokio::fs::read(&path).await {
        Ok(value) if !value.is_empty() && value.len() <= MAX_HLS_SEG_BYTES as usize => value,
        _ => return text_response(404, "offline file unavailable"),
    };
    tauri::http::Response::builder()
        .status(200)
        .header("Content-Type", if name.ends_with(".m3u8") {
            "application/vnd.apple.mpegurl"
        } else { "video/mp2t" })
        .header("Content-Length", contents.len().to_string())
        .header("Cache-Control", "private, no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(contents).unwrap()
}


// Public Ikanbot playback adapter. Reads the same anonymous public page data
// its first-party player uses. No login credentials, key extraction, or DRM.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IkanbotEpisode {
    title: String,
    url: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IkanbotLine {
    name: String,
    episodes: Vec<IkanbotEpisode>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IkanbotCatalog {
    title: String,
    lines: Vec<IkanbotLine>,
}

fn ikanbot_input<'a>(html: &'a str, field: &str) -> Option<&'a str> {
    let needle = format!("id=\"{field}\"");
    for fragment in html.split("<input").skip(1) {
        let tag = fragment.split_once('>')?.0;
        if !tag.contains(&needle) { continue; }
        let value = tag.split_once("value=\"")?.1;
        return Some(value.split_once('"')?.0);
    }
    None
}

fn ikanbot_page_token(id: &str, encrypted: &str) -> Result<String, String> {
    if !id.is_ascii() || id.len() < 4 || !id.bytes().all(|b| b.is_ascii_digit()) ||
       !encrypted.is_ascii() || encrypted.len() < 40 {
        return Err("爱看机器人公开页面缺少有效播放字段".into());
    }
    // Mirrors the anonymous page's own get_tks() operation on its hidden fields.
    let mut rest = encrypted;
    let mut token = String::new();
    for digit in id[id.len() - 4..].bytes() {
        let skip = usize::from((digit - b'0') % 3 + 1);
        if rest.len() < skip + 8 {
            return Err("爱看机器人网页播放字段发生变化".into());
        }
        token.push_str(&rest[skip..skip + 8]);
        rest = &rest[skip + 8..];
    }
    Ok(token)
}

async fn ikanbot_bounded(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    if !response.status().is_success() {
        return Err(format!("爱看机器人返回 HTTP {}", response.status()));
    }
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("爱看机器人页面或选集数据过大".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > limit {
            return Err("爱看机器人页面或选集数据超出安全大小限制".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn ikanbot_title(html: &str) -> String {
    html.split("<h1 id=\"video_title\"").nth(1)
        .and_then(|s| s.split_once('>'))
        .and_then(|(_,s)| s.split_once('<'))
        .map(|(s,_)| s.trim().chars().take(120).collect::<String>())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "爱看机器人视频".into())
}

#[tauri::command]
pub async fn hub_ikanbot_catalog(url: String) -> Result<IkanbotCatalog, String> {
    let target = public_target(&url)?;
    if target.scheme() != "https" || target.host_str() != Some("www1.ikanbot.com") ||
        !target.path().starts_with("/play/") ||
        !target.path().trim_start_matches("/play/").bytes().all(|c| c.is_ascii_digit()) ||
        target.path().trim_start_matches("/play/").is_empty() ||
        target.query().is_some() || target.fragment().is_some() {
        return Err("只支持 www1.ikanbot.com 的公开作品播放页面".into());
    }
    let html_response = crate::net::get(&url, Duration::from_secs(25), Some("text/html"), None).await?;
    let html = String::from_utf8(ikanbot_bounded(html_response, 1_000_000).await?)
        .map_err(|_| "爱看机器人网页编码无效")?;
    let id = ikanbot_input(&html, "current_id").ok_or("页面没有作品编号")?;
    if id != target.path().trim_start_matches("/play/") {
        return Err("页面作品编号与地址不匹配".into());
    }
    let encrypted = ikanbot_input(&html, "e_token").ok_or("页面缺少播放字段")?;
    let mtype = ikanbot_input(&html, "mtype").ok_or("页面缺少媒体类型")?;
    if mtype.len() > 3 || !mtype.bytes().all(|c| c.is_ascii_digit()) {
        return Err("网站媒体类型无效".into());
    }
    let token = ikanbot_page_token(id, encrypted)?;
    let mut api = url::Url::parse("https://www1.ikanbot.com/api/getResN")
        .map_err(|e| e.to_string())?;
    api.query_pairs_mut().append_pair("videoId", id)
        .append_pair("mtype", mtype).append_pair("token", &token);
    let response = crate::net::get(api.as_str(), Duration::from_secs(30),
        Some("application/json"), Some(&url)).await?;
    let payload: serde_json::Value = serde_json::from_slice(
        &ikanbot_bounded(response, 1_000_000).await?
    ).map_err(|_| "网站播放接口没有返回有效 JSON")?;
    if payload.get("state").and_then(|x| x.as_i64()) != Some(1) {
        return Err("网站未开放当前播放线路；可在原网站检查播放状态".into());
    }
    let groups = payload.pointer("/data/list").and_then(|x| x.as_array())
        .ok_or("网站没有返回线路列表")?;
    let mut lines = Vec::new();
    for group in groups.iter().take(32) {
        let Some(raw) = group.get("resData").and_then(|v| v.as_str()) else { continue; };
        let Ok(data) = serde_json::from_str::<serde_json::Value>(raw) else { continue; };
        let Some(tracks) = data.as_array() else { continue; };
        for track in tracks.iter().take(4) {
            let Some(playlist) = track.get("url").and_then(|v| v.as_str()) else { continue; };
            let name = track.get("flag").and_then(|v| v.as_str()).unwrap_or("线路");
            let mut episodes = Vec::new();
            for part in playlist.split('#').take(240) {
                let Some((title, link)) = part.split_once(char::from(36u8)) else { continue; };
                let title = title.trim();
                if title.is_empty() || title.len() > 80 || link.len() > 4000 { continue; }
                let Ok(uri) = url::Url::parse(link.trim()) else { continue; };
                if uri.scheme() != "https" ||
                    !uri.path().to_ascii_lowercase().ends_with(".m3u8") ||
                    crate::network_policy::parse_target_with_mode(uri.as_str(), true).is_err() {
                    continue;
                }
                episodes.push(IkanbotEpisode {
                    title: title.chars().take(50).collect(),
                    url: uri.to_string(),
                });
            }
            if !episodes.is_empty() {
                lines.push(IkanbotLine {
                    name: format!("线路 {} · {}", lines.len() + 1, name.chars().take(24).collect::<String>()),
                    episodes,
                });
            }
        }
    }
    if lines.is_empty() {
        return Err("网站返回了播放数据，但没有找到可供原生播放器使用的 HTTPS HLS 线路".into());
    }
    Ok(IkanbotCatalog { title: ikanbot_title(&html), lines })
}

#[cfg(test)]
mod ikanbot_tests {
    use super::*;
    #[test]
    fn public_token_algorithm_and_input_extraction() {
        let id = "442095";
        let encrypted = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMN";
        assert_eq!(ikanbot_page_token(id, encrypted).unwrap().len(), 32);
        let html = r#"<input type="hidden" id="current_id" value="442095"/><input id="e_token" value="abcdef"/>"#;
        assert_eq!(ikanbot_input(html, "current_id"), Some("442095"));
        assert_eq!(ikanbot_input(html, "e_token"), Some("abcdef"));
    }
}

#[cfg(test)]
mod hls_snapshot_tests {
    use super::*;
    #[test]
    fn rejects_encryption_and_live_streams() {
        let encrypted = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n#EXTINF:4,\na.ts\n#EXT-X-ENDLIST\n";
        assert!(hls_media_segments("https://example.org/list.m3u8", encrypted, "h1").is_err());
        let clear = "#EXTM3U\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:4,\na.ts\n#EXT-X-ENDLIST\n";
        assert!(hls_media_segments("https://example.org/list.m3u8", clear, "h1").is_ok());
        let live = "#EXTM3U\n#EXTINF:4,\na.ts\n";
        assert!(hls_media_segments("https://example.org/list.m3u8", live, "h1").is_err());
    }
    #[test]
    fn rewrites_standard_ts_segments_to_private_offline_route() {
        let media = "#EXTM3U\n#EXT-X-TARGETDURATION:8\n#EXTINF:4.0,\nhttps://example.org/seg1.ts\n#EXTINF:4.0,\nseg2.ts\n#EXT-X-ENDLIST\n";
        let result = hls_media_segments("https://example.org/list.m3u8", media, "h123").unwrap();
        assert_eq!(result.segments.len(), 2);
        assert!(result.playlist.contains("/offline-hls?id=h123&file=seg-00000.ts"));
        assert!(result.playlist.contains("/offline-hls?id=h123&file=seg-00001.ts"));
    }
}
) else { continue; };
                let title = title.trim();
                if title.is_empty() || title.len() > 80 || link.len() > 4000 { continue; }
                let Ok(uri) = url::Url::parse(link.trim()) else { continue; };
                if uri.scheme() != "https" ||
                    !uri.path().to_ascii_lowercase().ends_with(".m3u8") ||
                    crate::network_policy::parse_target_with_mode(uri.as_str(), true).is_err() {
                    continue;
                }
                episodes.push(IkanbotEpisode {
                    title: title.chars().take(50).collect(),
                    url: uri.to_string(),
                });
            }
            if !episodes.is_empty() {
                lines.push(IkanbotLine {
                    name: format!("线路 {} · {}", lines.len() + 1, name.chars().take(24).collect::<String>()),
                    episodes,
                });
            }
        }
    }
    if lines.is_empty() {
        return Err("网站返回了播放数据，但没有找到可供原生播放器使用的 HTTPS HLS 线路".into());
    }
    Ok(IkanbotCatalog { title: ikanbot_title(&html), lines })
}

#[cfg(test)]
mod ikanbot_tests {
    use super::*;
    #[test]
    fn public_token_algorithm_and_input_extraction() {
        let id = "442095";
        let encrypted = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMN";
        assert_eq!(ikanbot_page_token(id, encrypted).unwrap().len(), 32);
        let html = r#"<input type="hidden" id="current_id" value="442095"/><input id="e_token" value="abcdef"/>"#;
        assert_eq!(ikanbot_input(html, "current_id"), Some("442095"));
        assert_eq!(ikanbot_input(html, "e_token"), Some("abcdef"));
    }
}

#[cfg(test)]
mod hls_snapshot_tests {
    use super::*;
    #[test]
    fn rejects_encryption_and_live_streams() {
        let encrypted = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n#EXTINF:4,\na.ts\n#EXT-X-ENDLIST\n";
        assert!(hls_media_segments("https://example.org/list.m3u8", encrypted, "h1").is_err());
        let clear = "#EXTM3U\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:4,\na.ts\n#EXT-X-ENDLIST\n";
        assert!(hls_media_segments("https://example.org/list.m3u8", clear, "h1").is_ok());
        let live = "#EXTM3U\n#EXTINF:4,\na.ts\n";
        assert!(hls_media_segments("https://example.org/list.m3u8", live, "h1").is_err());
    }
    #[test]
    fn rewrites_standard_ts_segments_to_private_offline_route() {
        let media = "#EXTM3U\n#EXT-X-TARGETDURATION:8\n#EXTINF:4.0,\nhttps://example.org/seg1.ts\n#EXTINF:4.0,\nseg2.ts\n#EXT-X-ENDLIST\n";
        let result = hls_media_segments("https://example.org/list.m3u8", media, "h123").unwrap();
        assert_eq!(result.segments.len(), 2);
        assert!(result.playlist.contains("/offline-hls?id=h123&file=seg-00000.ts"));
        assert!(result.playlist.contains("/offline-hls?id=h123&file=seg-00001.ts"));
    }
}
