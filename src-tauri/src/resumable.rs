//! Crash-safe resumable downloads for public MP4/WebM and finite unencrypted HLS.
//! A durable task identity is based on source/episode rather than a timestamp.
//! A .part file is never considered complete; HLS segments are committed by rename.
use super::*;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

static PAUSE_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDownload {
    pub id: String,
    pub key: String,
    pub title: String,
    pub source_url: String,
    pub kind: String,
    pub received: u64,
    pub total: Option<u64>,
    pub completed_parts: Option<usize>,
    pub total_parts: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResumeRecord {
    id: String,
    key: String,
    title: String,
    source_url: String,
    kind: String,
    etag: Option<String>,
    last_modified: Option<String>,
    total: Option<u64>,
    fingerprint: Option<String>,
    total_parts: Option<usize>,
}

fn stable_id(key: &str) -> Result<String, String> {
    if key.len() < 5 || key.len() > 2048 || key.chars().any(|c| c.is_control()) {
        return Err("下载任务标识无效".into());
    }
    // Stable FNV-1a, no randomized DefaultHasher or OS-specific paths.
    let mut hash = 0xcbf29ce484222325u64;
    for b in key.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok(format!("h{hash:016x}"))
}

fn metadata_path(root: &Path, id: &str) -> PathBuf {
    root.join(format!("pending-{id}.json"))
}

async fn write_record(root: &Path, entry: &ResumeRecord) -> Result<(), String> {
    let path = metadata_path(root, &entry.id);
    let temp = root.join(format!("pending-{}.json.tmp", entry.id));
    let data = serde_json::to_vec(entry).map_err(|e| e.to_string())?;
    tokio::fs::write(&temp, data).await.map_err(|e| e.to_string())?;
    tokio::fs::rename(&temp, &path).await.map_err(|e| e.to_string())
}

fn read_record(root: &Path, id: &str) -> Result<Option<ResumeRecord>, String> {
    let path = metadata_path(root, id);
    match std::fs::read(&path) {
        Ok(data) => serde_json::from_slice(&data)
            .map(Some).map_err(|_| "续传记录损坏；请删除该任务后重新下载".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn pending_for(root: &Path, record: &ResumeRecord) -> PendingDownload {
    let (received, completed_parts) = if record.kind == "hls" {
        let mut size = 0u64;
        let mut parts = 0usize;
        for i in 0..record.total_parts.unwrap_or(0).min(MAX_HLS_ITEMS) {
            let path = root.join(&record.id).join(format!("seg-{i:05}.ts"));
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.is_file() && meta.len() > 0 && meta.len() <= MAX_HLS_SEG_BYTES {
                    size += meta.len();
                    parts += 1;
                }
            }
        }
        (size, Some(parts))
    } else {
        let path = root.join(format!("{}.{}.part", record.id, record.kind));
        (std::fs::metadata(path).map(|m| m.len()).unwrap_or(0), None)
    };
    PendingDownload {
        id: record.id.clone(),
        title: record.title.clone(),
        key: record.key.clone(),
        source_url: record.source_url.clone(),
        kind: record.kind.clone(),
        received,
        total: record.total,
        completed_parts,
        total_parts: record.total_parts,
    }
}

pub fn list_pending(app: &tauri::AppHandle) -> Result<Vec<PendingDownload>, String> {
    let root = download_root(app)?;
    let completed = load_index(app)?;
    let mut pending = Vec::new();
    for item in std::fs::read_dir(&root).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        let name = item.file_name();
        let Some(name) = name.to_str() else { continue; };
        let Some(id) = name.strip_prefix("pending-").and_then(|x| x.strip_suffix(".json")) else { continue; };
        if id.len() != 17 || !id.starts_with('h') ||
            !id[1..].chars().all(|c| c.is_ascii_hexdigit()) ||
            completed.iter().any(|i| i.id == id) { continue; }
        if let Some(record) = read_record(&root, id)? {
            if record.id == id {
                pending.push(pending_for(&root, &record));
            }
        }
    }
    pending.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(pending)
}

pub fn discard_pending(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    if id.len() != 17 || !id.starts_with('h') ||
        !id[1..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("无效的续传任务编号".into());
    }
    if DOWNLOADING.load(Ordering::SeqCst) {
        return Err("请先暂停当前下载再删除任务".into());
    }
    let root = download_root(app)?;
    if load_index(app)?.iter().any(|x| x.id == id) {
        return Err("已完成的下载请在离线列表中删除".into());
    }
    let record = read_record(&root, id)?.ok_or("找不到续传任务")?;
    if record.id != id { return Err("续传任务编号不一致".into()); }
    if record.kind == "hls" {
        match std::fs::remove_dir_all(root.join(id)) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
    } else if record.kind == "mp4" || record.kind == "webm" {
        let part = root.join(format!("{id}.{}.part", record.kind));
        match std::fs::remove_file(part) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
    }
    std::fs::remove_file(metadata_path(&root, id)).map_err(|e| e.to_string())
}

pub fn request_pause() {
    PAUSE_REQUESTED.store(true, Ordering::SeqCst);
}
fn check_pause() -> Result<(), String> {
    if PAUSE_REQUESTED.load(Ordering::SeqCst) {
        Err("下载已暂停；已保存进度，可在「离线」中继续".into())
    } else { Ok(()) }
}
fn emit_progress(app: &tauri::AppHandle, record: &ResumeRecord, received: u64, total: Option<u64>,
    parts: Option<usize>, total_parts: Option<usize>) {
    let _ = app.emit("hub-download-progress", serde_json::json!({
        "received": received, "total": total,
        "completedParts": parts, "totalParts": total_parts, "title": record.title,
    }));
}

fn headers_record(response: &reqwest::Response, record: &mut ResumeRecord) {
    record.etag = response.headers().get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok()).map(str::to_owned);
    record.last_modified = response.headers().get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok()).map(str::to_owned);
}
fn parse_content_range(header: &str) -> Option<(u64, u64, u64)> {
    let value = header.strip_prefix("bytes ")?;
    let (span, total) = value.split_once('/')?;
    let (start, end) = span.split_once('-')?;
    let (start, end, total) = (start.parse::<u64>().ok()?,end.parse::<u64>().ok()?,total.parse::<u64>().ok()?);
    if end < start || end >= total { return None; }
    Some((start, end, total))
}
fn expected_bytes(response: &reqwest::Response, prior_size: u64, prior_total: Option<u64>) -> Result<Option<u64>, String> {
    if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let header = response.headers().get(reqwest::header::CONTENT_RANGE)
            .and_then(|h| h.to_str().ok()).ok_or("服务器没有返回 Content-Range")?;
        let (start, end, total) = parse_content_range(header)
            .ok_or("服务器返回无效 Content-Range")?;
        if start != prior_size { return Err("续传位置与服务器返回的字节范围不一致".into()); }
        if let Some(previous) = prior_total {
            if previous != total { return Err("服务器文件大小变化；请删除旧任务后重试".into()); }
        }
        if response.content_length().is_some_and(|n| n != end-start+1) {
            return Err("服务器返回的分片大小与 Content-Range 不一致".into());
        }
        if total > MAX_DOWNLOAD_BYTES { return Err("视频超过 2GB 上限".into()); }
        Ok(Some(total))
    } else if response.status() == reqwest::StatusCode::OK {
        let total = response.content_length();
        if total.is_some_and(|n| n > MAX_DOWNLOAD_BYTES) {
            return Err("视频超过 2GB 上限".into());
        }
        Ok(total)
    } else {
        Err(format!("视频服务器返回 HTTP {}",response.status()))
    }
}
fn verify_media_type(response: &reqwest::Response) -> Result<(), String> {
    let ctype = response.headers().get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok()).unwrap_or("").to_ascii_lowercase();
    if !(ctype.starts_with("video/") || ctype.starts_with("application/octet-stream")) {
        return Err("服务器没有返回视频文件".into());
    }
    Ok(())
}
fn loaded_record(root: &Path, id: &str, key: &str, kind: &str, url: &str, title: &str) -> Result<ResumeRecord, String> {
    let existing = read_record(root, id)?;
    if let Some(mut record) = existing {
        if record.key != key || record.kind != kind {
            return Err("续传标识碰撞或文件类型变化；请先移除旧任务".into());
        }
        record.title = title.trim().chars().take(120).collect();
        Ok(record)
    } else {
        Ok(ResumeRecord { id: id.into(), key: key.into(), title: title.trim().chars().take(120).collect(),
            source_url: url.into(), kind: kind.into(), etag: None, last_modified: None,
            total: None, fingerprint: None, total_parts: None })
    }
}
fn check_completed(app: &tauri::AppHandle, id: &str) -> Result<Option<HubDownload>, String> {
    Ok(load_index(app)?.into_iter().find(|x| x.id == id))
}
fn save_completed(app: &tauri::AppHandle, entry: HubDownload) -> Result<HubDownload, String> {
    let mut index = load_index(app)?;
    if !index.iter().any(|x| x.id == entry.id) {
        index.push(entry.clone());
        save_index(app, &index)?;
    }
    Ok(entry)
}
async fn finalize_record(root: &Path, id: &str) {
    let _ = tokio::fs::remove_file(metadata_path(root, id)).await;
}

pub async fn download_media(app: tauri::AppHandle, url: String, title: String, key: String) -> Result<HubDownload, String> {
    let _slot = DownloadSlot::take()?;
    PAUSE_REQUESTED.store(false, Ordering::SeqCst);
    let target = public_target(&url)?;
    let kind = target.path().rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if kind != "mp4" && kind != "webm" {
        return Err("直链下载只支持 MP4/WebM".into());
    }
    let id = stable_id(&key)?;
    if let Some(completed) = check_completed(&app, &id)? { return Ok(completed); }
    let root = download_root(&app)?;
    let mut record = loaded_record(&root, &id, &key, &kind, &url, &title)?;
    let part = root.join(format!("{id}.{kind}.part"));
    let mut prior_size = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
    if prior_size > MAX_DOWNLOAD_BYTES { return Err("临时文件超过 2GB 限制".into()); }
    let range = (prior_size > 0).then(|| format!("bytes={prior_size}-"));
    let validator = record.etag.as_deref().filter(|s| !s.starts_with("W/"))
        .or(record.last_modified.as_deref()).map(str::to_owned);
    let mut response = crate::net::get_with_range(&url, Duration::from_secs(600), None, None,
        range.as_deref(), if prior_size > 0 { validator.as_deref() } else { None }).await?;
    verify_media_type(&response)?;
    let total = expected_bytes(&response, prior_size, record.total)?;
    let restarting = prior_size > 0 && response.status() == reqwest::StatusCode::OK;
    if restarting { prior_size = 0; }
    if !restarting && prior_size > 0 {
        // When no strong validator exists, require the URL to remain identical.
        if validator.is_none() && record.source_url != url {
            return Err("视频地址已改变且服务器没有续传校验信息，请删除旧任务重新下载".into());
        }
    }
    let remaining = total.map(|n| n.saturating_sub(prior_size));
    if remaining == Some(0) { return Err("服务器返回空的续传范围".into()); }
    if restarting {
        headers_record(&response, &mut record);
    } else if prior_size == 0 {
        headers_record(&response, &mut record);
    }
    record.total = total;
    record.source_url = url;
    write_record(&root, &record).await?;

    let mut file = tokio::fs::OpenOptions::new().create(true).write(true)
        .append(prior_size > 0).truncate(prior_size == 0).open(&part).await.map_err(|e| e.to_string())?;
    let mut received = prior_size;
    emit_progress(&app, &record, received, total, None, None);
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        check_pause()?;
        received = received.checked_add(chunk.len() as u64).ok_or("文件大小溢出")?;
        if received > MAX_DOWNLOAD_BYTES || total.is_some_and(|n| received > n) {
            return Err("下载超过文件预期大小".into());
        }
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        emit_progress(&app, &record, received, total, None, None);
    }
    file.flush().await.map_err(|e| e.to_string())?;
    file.sync_all().await.map_err(|e| e.to_string())?;
    drop(file);
    check_pause()?;
    if received == 0 || total.is_some_and(|n| received != n) {
        return Err("视频尚未完整下载；已保存部分数据供续传".into());
    }
    let filename = format!("{id}.{kind}");
    tokio::fs::rename(&part,root.join(&filename)).await.map_err(|e| e.to_string())?;
    let completed = HubDownload {
        id: id.clone(), title: record.title.clone(), filename,
        bytes: received, content_type: if kind == "mp4" { "video/mp4" } else { "video/webm" }.into()
    };
    let result = save_completed(&app, completed)?;
    finalize_record(&root,&id).await;
    Ok(result)
}

fn playlist_fingerprint(snapshot: &HlsSnapshot) -> String {
    let mut h = 0xcbf29ce484222325u64;
    // Exclude expiring query parameters but require stable segment paths and durations.
    for (segment, _) in &snapshot.segments {
        if let Ok(parsed) = url::Url::parse(segment) {
            for b in parsed.path().as_bytes().iter().chain([b'\n'].iter()) {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x100000001b3);
            }
        }
    }
    for row in snapshot.playlist.lines().filter(|line| line.starts_with("#EXTINF:")) {
        for b in row.as_bytes().iter().chain([b'\n'].iter()) {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}-{}",snapshot.segments.len())
}
fn valid_segment(path: &Path) -> Option<u64> {
    let meta = std::fs::metadata(path).ok()?;
    (meta.is_file() && meta.len() > 0 && meta.len() <= MAX_HLS_SEG_BYTES).then_some(meta.len())
}

async fn atomic_hls_segment(target: &str, path: &Path, remaining: u64) -> Result<u64, String> {
    public_target(target)?;
    let mut response = crate::net::get(target,Duration::from_secs(90),
        Some("video/mp2t,application/octet-stream,*/*;q=0.1"),None).await?;
    if !response.status().is_success() {
        return Err(format!("视频分片 HTTP {}",response.status()));
    }
    if response.content_length().is_some_and(|n| n > MAX_HLS_SEG_BYTES || n > remaining) {
        return Err("分片超过大小上限".into());
    }
    // Truncated .part segments are never reused; only atomic renamed TS files count.
    let partial = path.with_extension("ts.part");
    let mut file = tokio::fs::File::create(&partial).await.map_err(|e| e.to_string())?;
    let mut received = 0u64;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        check_pause()?;
        received = received.checked_add(chunk.len() as u64).ok_or("分片大小溢出")?;
        if received > MAX_HLS_SEG_BYTES || received > remaining {
            return Err("分片或视频大小超限".into());
        }
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }
    if received == 0 { return Err("空的视频分片".into()); }
    if response.content_length().is_some_and(|n| n != received) {
        return Err("视频分片传输不完整".into());
    }
    file.flush().await.map_err(|e| e.to_string())?;
    file.sync_all().await.map_err(|e| e.to_string())?;
    drop(file);
    check_pause()?;
    tokio::fs::rename(&partial,path).await.map_err(|e| e.to_string())?;
    Ok(received)
}

pub async fn download_hls(app: tauri::AppHandle, url: String, title: String, key: String) -> Result<HubDownload, String> {
    let _slot = DownloadSlot::take()?;
    PAUSE_REQUESTED.store(false, Ordering::SeqCst);
    let target = public_target(&url)?;
    if target.scheme() != "https" || !target.path().to_ascii_lowercase().ends_with(".m3u8") {
        return Err("只支持公开 HTTPS 的普通 HLS 播放列表".into());
    }
    let id = stable_id(&key)?;
    if let Some(completed) = check_completed(&app, &id)? { return Ok(completed); }
    let root = download_root(&app)?;
    let mut record = loaded_record(&root,&id,&key,"hls",&url,&title)?;
    let (media_url, media_playlist) = resolve_hls_media(&url).await?;
    let snapshot = hls_media_segments(&media_url,&media_playlist,&id)?;
    let fingerprint = playlist_fingerprint(&snapshot);
    if let Some(previous) = &record.fingerprint {
        if previous != &fingerprint {
            return Err("视频源的分片列表发生变化；为避免拼接错误，请删除旧任务再重新下载".into());
        }
    }
    record.fingerprint = Some(fingerprint);
    record.total_parts = Some(snapshot.segments.len());
    let dir = root.join(&id);
    tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
    write_record(&root,&record).await?;
    let mut completed = 0u64;
    let mut completed_parts = 0usize;
    for (_,name) in &snapshot.segments {
        if let Some(size) = valid_segment(&dir.join(name)) {
            completed += size;
            completed_parts += 1;
        }
    }
    emit_progress(&app,&record,completed,None,Some(completed_parts),record.total_parts);
    for (index,(remote,filename)) in snapshot.segments.iter().enumerate() {
        check_pause()?;
        let path = dir.join(filename);
        if valid_segment(&path).is_some() { continue; }
        let _ = tokio::fs::remove_file(&path).await;
        let size = atomic_hls_segment(remote,&path,MAX_DOWNLOAD_BYTES.saturating_sub(completed))
            .await.map_err(|e| format!("第 {} 个分片下载失败：{e}。已保存前面的分片",index+1))?;
        completed += size;
        completed_parts += 1;
        emit_progress(&app,&record,completed,None,Some(completed_parts),record.total_parts);
    }
    check_pause()?;
    let tmp = dir.join("index.m3u8.part");
    tokio::fs::write(&tmp,snapshot.playlist.as_bytes()).await.map_err(|e| e.to_string())?;
    tokio::fs::rename(&tmp,dir.join("index.m3u8")).await.map_err(|e| e.to_string())?;
    let saved = HubDownload {
        id:id.clone(), title:record.title.clone(),filename:"index.m3u8".into(),
        bytes:completed,content_type:"application/vnd.apple.mpegurl".into(),
    };
    let result = save_completed(&app,saved)?;
    finalize_record(&root,&id).await;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_keys_and_parse_range() {
        assert_eq!(stable_id("kubb:episode:001").unwrap(),stable_id("kubb:episode:001").unwrap());
        assert_ne!(stable_id("kubb:episode:001").unwrap(),stable_id("kubb:episode:002").unwrap());
        assert_eq!(parse_content_range("bytes 100-199/500"),Some((100,199,500)));
        assert_eq!(parse_content_range("bytes 200-100/500"),None);
    }
}
