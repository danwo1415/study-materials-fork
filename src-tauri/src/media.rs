//! MyVideo standalone HLS proxy. No legacy cover encryption or Huangguo API.
use crate::net;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use tauri::http::Response;

#[cfg(any(target_os = "windows", target_os = "android"))]
pub const LOCAL_ORIGIN: &str = "http://hg.localhost";
#[cfg(not(any(target_os = "windows", target_os = "android")))]
pub const LOCAL_ORIGIN: &str = "hg://localhost";

fn text_response(status: u16, message: &str) -> Response<Vec<u8>> {
    Response::builder().status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Cache-Control", "no-store")
        .body(message.as_bytes().to_vec()).unwrap()
}

fn media_response(mime: &str, bytes: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder().status(200)
        .header("Content-Type", mime)
        .header("Content-Length", bytes.len().to_string())
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "no-store")
        .body(bytes).unwrap()
}

fn checked_uri(base: &str, raw: &str) -> Result<String, String> {
    let base = url::Url::parse(base).map_err(|_| "播放列表基址无效")?;
    let resolved = base.join(raw.trim()).map_err(|_| "视频资源地址无效")?;
    if resolved.scheme() != "https" {
        return Err("只支持 HTTPS 视频资源".into());
    }
    crate::network_policy::parse_target_with_mode(resolved.as_str(), true)?;
    Ok(resolved.to_string())
}
fn routed_uri(base: &str, raw: &str, prefix: &str) -> Result<String, String> {
    let absolute = checked_uri(base, raw)?;
    Ok(format!("{prefix}/seg?u={}", utf8_percent_encode(&absolute, NON_ALPHANUMERIC)))
}

// HLS URI attributes are quoted. Rewrite every URI="..." (KEY, MAP, MEDIA etc).
// Never let the WebView fetch remote HLS segments or keys directly.
fn rewrite_uri_attributes(base: &str, line: &str, prefix: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = line;
    while let Some(pos) = rest.find("URI=\"") {
        let value_start = pos + 5;
        let Some(end) = rest[value_start..].find('"') else { return Err("HLS URI 属性不完整".into()); };
        out.push_str(&rest[..value_start]);
        let original = &rest[value_start..value_start + end];
        out.push_str(&routed_uri(base, original, prefix)?);
        out.push('"');
        rest = &rest[value_start + end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn rewrite_playlist(base: &str, contents: &str, prefix: &str) -> Result<String, String> {
    let stripped = contents.trim_start_matches('\u{feff}');
    if !stripped.starts_with("#EXTM3U") { return Err("响应不是有效 HLS 播放列表".into()); }
    let mut output = String::with_capacity(contents.len() + 128);
    for raw in stripped.lines() {
        let line = raw.trim();
        if line.starts_with("#EXT-X-CONTENT-STEERING:") {
            // No player-side remote steering endpoints.
            continue;
        }
        if line.starts_with("#EXT-X-DATERANGE:") {
            // Metadata can embed remote URLs; omit it without altering timestamps.
            continue;
        }
        if line.is_empty() { output.push('\n'); continue; }
        if line.starts_with('#') {
            output.push_str(&rewrite_uri_attributes(base, line, prefix)?);
        } else {
            output.push_str(&routed_uri(base, line, prefix)?);
        }
        output.push('\n');
    }
    Ok(output)
}

pub async fn handle(path: &str, query: &str, origin: &str) -> Response<Vec<u8>> {
    if path != "/hls" && path != "/seg" {
        return text_response(404, "unknown media route");
    }
    let url = url::form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == "u")
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default();
    if url.len() > 4000 || url.is_empty() {
        return text_response(400, "missing media URL");
    }
    if let Err(error) = crate::network_policy::parse_target_with_mode(&url, true) {
        return text_response(400, &error);
    }
    if !url.starts_with("https://") { return text_response(400, "HTTPS required"); }
    let (ctype, bytes) = match net::fetch_bytes(&url).await {
        Ok(parts) => parts,
        Err(error) => return text_response(502, &error),
    };
    let is_playlist = path == "/hls"
        || bytes.starts_with(b"#EXTM3U")
        || bytes.starts_with(b"\xef\xbb\xbf#EXTM3U")
        || ctype.as_deref().is_some_and(|x| {
            x.contains("mpegurl") || x.contains("mpegURL")
        });
    if is_playlist {
        let contents = match std::str::from_utf8(&bytes) {
            Ok(v) => v,
            Err(_) => return text_response(502, "HLS 播放列表文本编码无效"),
        };
        let rewritten = match rewrite_playlist(&url, contents, origin) {
            Ok(v) => v,
            Err(error) => return text_response(502, &error),
        };
        return media_response("application/vnd.apple.mpegurl", rewritten.into_bytes());
    }
    if path == "/hls" { return text_response(502, "源站未返回 HLS 播放列表"); }
    let mime = ctype.as_deref().filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if url.split('?').next().unwrap_or("").ends_with(".ts") { "video/mp2t" }
            else { "application/octet-stream" }
        });
    media_response(mime, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rewrites_nested_and_relative_hls_urls() {
        let original = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=900000\nvariant/a.m3u8\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.txt\"\n#EXTINF:6,\nseg-1.ts\n#EXT-X-ENDLIST\n";
        let result = rewrite_playlist("https://example.com/media/master.m3u8", original, LOCAL_ORIGIN).unwrap();
        assert_eq!(result.matches("/seg?u=").count(), 3);
        assert!(!result.contains("URI=\"key.txt\""));
    }
}
