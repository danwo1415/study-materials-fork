//! MyVideo: standalone, multi-source video client.
//! No Huangguo accounts, API request envelopes, feed, or login services.
mod media;
mod net;
mod network_policy;
mod source_hub;

use tauri::Manager;

fn frontend_origin_allowed(origin: &str) -> bool {
    matches!(origin, "tauri://localhost" | "http://tauri.localhost" | "https://tauri.localhost" | "null")
        || (cfg!(debug_assertions) && url::Url::parse(origin).is_ok_and(|u|
            u.scheme() == "http" && u.port() == Some(1420)
            && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "::1"))))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if let Ok(path) = app.path().app_data_dir() {
                let _ = std::fs::create_dir_all(&path);
                network_policy::bind_security(&path);
                // MyVideo does not load a Huangguo session, line, or auth token.
            }
            Ok(())
        })
        .plugin(
            tauri::plugin::Builder::<_, ()>::new("navigation-guard")
                .on_navigation(|_webview, target| {
                    target.scheme() == "tauri" && target.host_str() == Some("localhost")
                        || frontend_origin_allowed(&target.origin().ascii_serialization())
                }).build(),
        )
        .register_asynchronous_uri_scheme_protocol("hg", |ctx, request, responder| {
            let origin = request.headers().get("Origin")
                .and_then(|x| x.to_str().ok()).map(str::to_owned);
            if origin.as_deref().is_some_and(|value| !frontend_origin_allowed(value)) {
                responder.respond(tauri::http::Response::builder()
                    .status(403).body(b"origin denied".to_vec()).unwrap());
                return;
            }
            let app_handle = ctx.app_handle().clone();
            let range = request.headers().get("Range")
                .and_then(|h| h.to_str().ok()).map(str::to_owned);
            let uri = request.uri().clone();
            let path = uri.path().to_string();
            let query = uri.query().unwrap_or("").to_string();
            tauri::async_runtime::spawn(async move {
                let mut response = match path.as_str() {
                    "/offline" => source_hub::serve_offline(&app_handle, &query, range.as_deref()).await,
                    "/offline-hls" => source_hub::serve_hls_offline(&app_handle, &query).await,
                    _ => media::handle(&path, &query, media::LOCAL_ORIGIN).await,
                };
                if let Some(value) = origin.and_then(|o| tauri::http::HeaderValue::from_str(&o).ok()) {
                    response.headers_mut().insert("Access-Control-Allow-Origin", value);
                } else {
                    response.headers_mut().insert("Access-Control-Allow-Origin", tauri::http::HeaderValue::from_static("*"));
                }
                responder.respond(response);
            });
        })
        .invoke_handler(tauri::generate_handler![
            source_hub::hub_fetch_html,
            source_hub::hub_fetch_public_json,
            source_hub::hub_download_media,
            source_hub::hub_download_hls,
            source_hub::hub_list_downloads,
            source_hub::hub_remove_download,
            source_hub::hub_list_pending_downloads,
            source_hub::hub_discard_pending_download,
            source_hub::hub_pause_download,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run MyVideo");
}
