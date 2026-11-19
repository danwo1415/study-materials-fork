//! Android Media3 HLS player plugin for public HTTPS streams.
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Runtime};
#[cfg(target_os = "android")]
use tauri::Manager;

#[cfg(target_os = "android")]
struct NativeVideo<R: Runtime>(tauri::plugin::PluginHandle<R>);

#[tauri::command]
async fn play<R: Runtime>(app: AppHandle<R>, url: String, title: Option<String>) -> Result<(), String> {
    let uri = url::Url::parse(&url).map_err(|_| "视频地址无效")?;
    if uri.scheme() != "https" || !uri.path().to_ascii_lowercase().ends_with(".m3u8")
        || !uri.username().is_empty() || uri.password().is_some() {
        return Err("原生播放器只接受公开 HTTPS HLS 视频".into());
    }
    #[cfg(target_os = "android")]
    {
        let _: serde_json::Value = app.state::<NativeVideo<R>>().0
            .run_mobile_plugin("play", serde_json::json!({"url": url, "title": title.unwrap_or_default()}))
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, title);
        Err("原生播放当前仅支持 Android".into())
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("nativevideo")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("com.daneg.nativevideo", "NativeVideoPlugin")?;
                app.manage(NativeVideo(handle));
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = (app, api);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![play])
        .build()
}
