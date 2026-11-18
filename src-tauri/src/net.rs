//! MyVideo public HTTPS networking only.
//! No Huangguo endpoints, accounts, credentials, or edge-request envelopes.
use crate::network_policy;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const USER_AGENT: &str = "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36";
const MAX_MEDIA_BYTES: usize = 32 * 1024 * 1024;
const CLIENT_TTL: Duration = Duration::from_secs(30);
type ClientCache = HashMap<String, (Instant, Arc<reqwest::Client>)>;
static CLIENTS: Lazy<Mutex<ClientCache>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn build_client(host: &str, addresses: &[SocketAddr]) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(35))
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
        .gzip(true)
        .brotli(true);
    if !addresses.is_empty() {
        builder = builder.resolve_to_addrs(host, addresses);
    }
    // Prevent the app from silently forwarding source requests to a historical API.
    // Use the device's ordinary network configuration; no hardcoded external proxy.
    builder.build().map_err(|_| "无法初始化网络客户端".to_string())
}

async fn client_for(raw: &str) -> Result<(url::Url, Arc<reqwest::Client>), String> {
    let target = network_policy::parse_target_with_mode(raw, true)?;
    if target.scheme() != "https" {
        return Err("MyVideo 仅连接公开 HTTPS 站点".into());
    }
    let host = target.host_str().ok_or("缺少主机名")?;
    let port = target.port_or_known_default().ok_or("缺少端口")?;
    let key = format!("{host}:{port}");
    {
        let map = CLIENTS.lock().map_err(|_| "客户端锁出错")?;
        if let Some((expiry, http)) = map.get(&key) {
            if *expiry > Instant::now() { return Ok((target, http.clone())); }
        }
    }
    // Check every DNS answer, then pin the vetted addresses to the HTTP client.
    let addresses = network_policy::resolve_target(&target, true).await?;
    let client = Arc::new(build_client(host, &addresses)?);
    let mut map = CLIENTS.lock().map_err(|_| "客户端锁出错")?;
    map.retain(|_, (until, _)| *until > Instant::now());
    if map.len() > 48 { map.clear(); }
    map.insert(key, (Instant::now() + CLIENT_TTL, client.clone()));
    Ok((target, client))
}

pub async fn get(
    url: &str,
    timeout: Duration,
    accept: Option<&str>,
    referer: Option<&str>,
) -> Result<reqwest::Response, String> {
    get_with_range(url, timeout, accept, referer, None, None).await
}

/// Redirect-safe ranged GET for resumable media.
pub async fn get_with_range(
    url: &str,
    timeout: Duration,
    accept: Option<&str>,
    referer: Option<&str>,
    range: Option<&str>,
    if_range: Option<&str>,
) -> Result<reqwest::Response, String> {
    if !network_policy::strict_security() {
        return Err("MyVideo 需要严格安全模式".into());
    }
    tokio::time::timeout(timeout, async {
        let mut next = network_policy::parse_target_with_mode(url, true)?;
        if next.scheme() != "https" {
            return Err("仅支持 HTTPS 站点".into());
        }
        for hop in 0..=5 {
            let (target, client) = client_for(next.as_str()).await?;
            let mut request = client.get(target.clone()).timeout(timeout);
            if let Some(value) = accept { request = request.header("Accept", value); }
            if let Some(value) = referer { request = request.header("Referer", value); }
            if range.is_some() { request = request.header("Accept-Encoding", "identity"); }
            if let Some(value) = range { request = request.header("Range", value); }
            if let Some(value) = if_range { request = request.header("If-Range", value); }
            let response = request.send().await.map_err(|e| e.to_string())?;
            if !response.status().is_redirection() { return Ok(response); }
            if hop >= 5 { return Err("重定向次数过多".into()); }
            let location = response.headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or("重定向地址无效")?;
            next = network_policy::redirect_target_with_mode(&target, location, true)?;
            if next.scheme() != "https" { return Err("拒绝不安全的重定向".into()); }
        }
        Err("重定向次数过多".into())
    }).await.map_err(|_| "请求超时".to_string())?
}

pub async fn fetch_bytes(url: &str) -> Result<(Option<String>, Vec<u8>), String> {
    let mut response = get(url, Duration::from_secs(35), None, None).await?;
    if !response.status().is_success() {
        return Err(format!("媒体资源返回 HTTP {}", response.status()));
    }
    if response.content_length().is_some_and(|n| n > MAX_MEDIA_BYTES as u64) {
        return Err("单个媒体分片超过 32MB 上限".into());
    }
    let content_type = response.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|x| x.to_str().ok())
        .map(str::to_owned);
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > MAX_MEDIA_BYTES {
            return Err("单个媒体分片超过 32MB 上限".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((content_type, bytes))
}
