package com.daneg.nativevideo

import android.app.Activity
import android.graphics.Color
import android.os.Bundle
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
import androidx.media3.ui.PlayerView

class NativeVideoActivity : Activity() {
    private var player: ExoPlayer? = null
    private var playerView: PlayerView? = null
    private var statusView: TextView? = null

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
            setPadding(16, 12, 16, 20)
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
            val exo = ExoPlayer.Builder(this)
                .setMediaSourceFactory(DefaultMediaSourceFactory(dataSource))
                .build()
            player = exo
            surface.player = exo
            exo.addListener(object : Player.Listener {
                override fun onPlaybackStateChanged(state: Int) {
                    statusView?.text = when(state) {
                        Player.STATE_BUFFERING -> "原生播放器正在读取 HLS 分片..."
                        Player.STATE_READY -> ""
                        Player.STATE_ENDED -> "视频播放完毕"
                        else -> "正在准备播放..."
                    }
                }
                override fun onPlayerError(error: PlaybackException) {
                    val reason = error.cause?.message ?: error.message ?: "未知错误"
                    statusView?.text = "播放失败：" + error.errorCodeName + " / " + reason.take(200)
                }
            })
            exo.setMediaItem(MediaItem.Builder().setUri(videoUrl)
                .setMimeType(MimeTypes.APPLICATION_M3U8).build())
            exo.prepare()
            exo.playWhenReady = true
        } catch (error: Exception) {
            statusView?.text = "启动原生播放器失败：" + error.message.orEmpty().take(180)
        }
    }

    override fun onStop() {
        player?.pause()
        super.onStop()
    }

    override fun onDestroy() {
        playerView?.player = null
        player?.release()
        player = null
        super.onDestroy()
    }
}
