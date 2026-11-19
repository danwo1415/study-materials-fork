package com.daneg.nativevideo

import android.app.Activity
import android.graphics.Color
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.ViewGroup
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.exoplayer.analytics.AnalyticsListener
import androidx.media3.exoplayer.source.LoadEventInfo
import androidx.media3.exoplayer.source.MediaLoadData
import androidx.media3.datasource.HttpDataSource
import java.io.IOException
import androidx.media3.ui.PlayerView

class NativeVideoActivity : Activity() {
    private var player: ExoPlayer? = null
    private var playerView: PlayerView? = null
    private var statusView: TextView? = null
    private val mainHandler = Handler(Looper.getMainLooper())
    private var timeoutTask: Runnable? = null
    private var fatalError = false
    private var lastLoadProblem = ""
    private var lastPlayerState = "IDLE"

    private fun status(message: String) {
        statusView?.text = message
    }

    private fun safeReason(error: Throwable): String {
        val chain = mutableListOf<String>()
        var cause: Throwable? = error
        repeat(4) {
            val current = cause ?: return@repeat
            val detail = if (current is HttpDataSource.InvalidResponseCodeException) {
                "HTTP " + current.responseCode
            } else {
                current.javaClass.simpleName + ": " +
                    current.message.orEmpty().replace(Regex("https?://[^\\s]+"), "[URL]").take(130)
            }
            chain.add(detail)
            cause = current.cause
        }
        return chain.joinToString(" / ").take(300)
    }


    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        val videoUrl = intent.getStringExtra("videoUrl").orEmpty()
        val title = intent.getStringExtra("videoTitle").orEmpty()
        if (!videoUrl.startsWith("https://") || !videoUrl.substringBefore('?').endsWith(".m3u8")) {
            finish()
            return
        }
        val screen = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.BLACK)
        }
        val topBar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setBackgroundColor(Color.rgb(22, 22, 22))
        }
        val back = Button(this).apply {
            text = "返回"
            setOnClickListener { finish() }
        }
        topBar.addView(back, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT
        ))
        topBar.addView(TextView(this).apply {
            text = title.ifBlank { "MyVideo 原生播放" }
            setTextColor(Color.WHITE)
            textSize = 15f
            maxLines = 1
            setPadding(12, 0, 12, 0)
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        screen.addView(topBar)
        val surface = PlayerView(this).apply {
            useController = true
            controllerAutoShow = true
            setShowBuffering(PlayerView.SHOW_BUFFERING_ALWAYS)
        }
        playerView = surface
        screen.addView(surface, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        val info = TextView(this).apply {
            text = "正在连接公开播放线路..."
            setTextColor(Color.LTGRAY)
            textSize = 12f
            setPadding(16, 12, 16, 24)
            minLines = 2
            maxLines = 6
        }
        statusView = info
        screen.addView(info)
        setContentView(screen)
        try {
            val dataSource = DefaultHttpDataSource.Factory()
                .setUserAgent("Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/131.0 Mobile Safari/537.36")
                .setConnectTimeoutMs(15000)
                .setReadTimeoutMs(30000)
                .setAllowCrossProtocolRedirects(false)
                .setDefaultRequestProperties(mapOf("Referer" to "https://www1.ikanbot.com/"))
            val exo = ExoPlayer.Builder(this)
                .setMediaSourceFactory(DefaultMediaSourceFactory(dataSource))
                .build()
            player = exo
            surface.player = exo
            exo.addAnalyticsListener(object : AnalyticsListener {
                override fun onLoadError(
                    eventTime: AnalyticsListener.EventTime,
                    loadEventInfo: LoadEventInfo,
                    mediaLoadData: MediaLoadData,
                    error: IOException,
                    wasCanceled: Boolean
                ) {
                    if (wasCanceled || fatalError) return
                    val host = loadEventInfo.uri.host ?: "unknown-host"
                    lastLoadProblem = "线路请求失败（" + host + "）：" + safeReason(error)
                    status("正在重试... " + lastLoadProblem)
                }
            })
            exo.addListener(object : Player.Listener {
                override fun onPlaybackStateChanged(state: Int) {
                    lastPlayerState = when (state) {
                        Player.STATE_IDLE -> "IDLE"
                        Player.STATE_BUFFERING -> "BUFFERING"
                        Player.STATE_READY -> "READY"
                        Player.STATE_ENDED -> "ENDED"
                        else -> "UNKNOWN"
                    }
                    // Media3 sends STATE_IDLE after onPlayerError. Do not hide the error.
                    if (fatalError) return
                    when (state) {
                        Player.STATE_BUFFERING -> status("正在加载视频分片..." +
                            if (lastLoadProblem.isNotBlank()) "\n" + lastLoadProblem else "")
                        Player.STATE_READY -> status("视频已准备好，可以播放")
                        Player.STATE_ENDED -> status("视频播放完毕")
                        Player.STATE_IDLE -> status("播放器空闲；等待连接..." +
                            if (lastLoadProblem.isNotBlank()) "\n" + lastLoadProblem else "")
                    }
                }
                override fun onPlayerError(error: PlaybackException) {
                    fatalError = true
                    timeoutTask?.let { mainHandler.removeCallbacks(it) }
                    status("播放失败：" + error.errorCodeName + "\n" + safeReason(error) +
                        if (lastLoadProblem.isNotBlank()) "\n最近的线路错误：" + lastLoadProblem else "")
                }
            })
            exo.setMediaItem(MediaItem.Builder().setUri(videoUrl)
                .setMimeType(MimeTypes.APPLICATION_M3U8).build())
            exo.prepare()
            exo.playWhenReady = true
            timeoutTask = Runnable {
                if (!fatalError && exo.playbackState != Player.STATE_READY
                    && exo.playbackState != Player.STATE_ENDED) {
                    status("超过 25 秒未开始播放，状态：" + lastPlayerState +
                        if (lastLoadProblem.isNotBlank()) "\n" + lastLoadProblem
                        else "\n服务器或分片尚未响应，请截图诊断信息")
                }
            }.also { mainHandler.postDelayed(it, 25_000) }
        } catch (error: Exception) {
            statusView?.text = "启动原生播放器失败：" + error.message.orEmpty().take(180)
        }
    }

    override fun onStop() {
        player?.pause()
        super.onStop()
    }

    override fun onDestroy() {
        timeoutTask?.let { mainHandler.removeCallbacks(it) }
        playerView?.player = null
        player?.release()
        player = null
        super.onDestroy()
    }
}
