//! Public-web HTML discovery and optional personal offline MP4/WebM downloads.
//! This module does not circumvent DRM, authentication, or site restrictions.
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};

const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;
const MAX_DOWNLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const OFFLINE_CHUNK_BYTES: u64 = 1024 * 1024;
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

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

#[tauri::command]
pub async fn hub_download_media(
    app: tauri::AppHandle,
    url: String,
    title: String,
) -> Result<HubDownload, String> {
    use tokio::io::AsyncWriteExt;
    let _slot = DownloadSlot::take()?;
    let target = public_target(&url)?;
    let ext = target.path().rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if ext != "mp4" && ext != "webm" {
        return Err("首个版本只支持网站允许下载的 MP4 / WebM 直链，暂不支持 HLS / DRM".into());
    }

    let mut response = crate::net::get(&url, Duration::from_secs(600), None, None).await?;
    if !response.status().is_success() {
        return Err(format!("下载服务器返回 HTTP {}", response.status()));
    }
    let ctype = response.headers().get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok()).unwrap_or("").to_ascii_lowercase();
    let allowed_type = ctype.starts_with("video/") || ctype.starts_with("application/octet-stream");
    if !allowed_type {
        return Err("服务器未返回视频文件，可能需要登录或视频受到保护".into());
    }
    let total = response.content_length();
    if total.is_some_and(|n| n > MAX_DOWNLOAD_BYTES) {
        return Err("视频超过 2GB 下载上限".into());
    }

    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_millis();
    let id = format!("h{stamp}");
    let filename = format!("{id}.{ext}");
    let path = download_root(&app)?.join(&filename);
    let temp_path = path.with_extension(format!("{ext}.part"));
    let mut file = tokio::fs::File::create(&temp_path).await.map_err(|e| e.to_string())?;
    let mut received = 0u64;

    let transfer = async {
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            received += chunk.len() as u64;
            if received > MAX_DOWNLOAD_BYTES { return Err("超过 2GB 下载上限".to_string()); }
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            let _ = app.emit("hub-download-progress", serde_json::json!({
                "received": received, "total": total, "title": title
            }));
        }
        file.flush().await.map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    }.await;
    if let Err(e) = transfer {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(e);
    }
    if received == 0 {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err("服务器返回空文件".into());
    }
    tokio::fs::rename(&temp_path, &path).await.map_err(|e| e.to_string())?;
    let item = HubDownload {
        id,
        title: title.trim().chars().take(120).collect(),
        filename,
        bytes: received,
        content_type: if ext == "mp4" { "video/mp4" } else { "video/webm" }.into(),
    };
    let mut items = load_index(&app)?;
    items.push(item.clone());
    save_index(&app, &items)?;
    Ok(item)
}

#[tauri::command]
pub fn hub_remove_download(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("无效的文件标识".into());
    }
    let mut items = load_index(&app)?;
    let item = items.iter().find(|it| it.id == id).cloned().ok_or("没有找到下载文件")?;
    std::fs::remove_file(download_root(&app)?.join(&item.filename)).map_err(|e| e.to_string())?;
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
