package com.daneg.nativevideo

import android.app.Activity
import android.content.Intent
import android.net.Uri
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class PlayArgs {
    var url: String? = null
    var title: String? = null
}

@TauriPlugin
class NativeVideoPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun play(invoke: Invoke) {
        val args = invoke.parseArgs(PlayArgs::class.java)
        val url = args.url.orEmpty()
        val uri = Uri.parse(url)
        if (uri.scheme != "https" || !uri.path.orEmpty().lowercase().endsWith(".m3u8")
            || !uri.userInfo.isNullOrEmpty()) {
            invoke.reject("只允许播放公开 HTTPS HLS 视频")
            return
        }
        activity.startActivity(Intent(activity, NativeVideoActivity::class.java).apply {
            putExtra("videoUrl", url)
            putExtra("videoTitle", args.title.orEmpty().take(120))
        })
        invoke.resolve(JSObject())
    }
}
