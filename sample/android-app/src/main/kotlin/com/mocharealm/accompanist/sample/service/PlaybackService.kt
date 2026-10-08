package com.mocharealm.accompanist.sample.service

import android.app.PendingIntent
import android.content.Intent
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import com.mocharealm.accompanist.sample.MainActivity
import com.mocharealm.accompanist.sample.PlaybackTimingTrace
import com.mocharealm.accompanist.sample.AudioOutputTimingTrace
import androidx.media3.exoplayer.audio.AudioSink
import android.content.Context

class PlaybackService : MediaSessionService() {

    private var mediaSession: MediaSession? = null
    private lateinit var player: ExoPlayer

    override fun onCreate() {
        super.onCreate()
        initializePlayerAndSession()
    }

    @OptIn(UnstableApi::class)
    private fun initializePlayerAndSession() {
        val renderersFactory =
            object : DefaultRenderersFactory(this) {
                override fun buildAudioSink(
                    context: Context,
                    enableFloatOutput: Boolean,
                    enableAudioOutputPlaybackParams: Boolean,
                ): AudioSink = AudioOutputTimingTrace.createSink(
                    context, enableFloatOutput, enableAudioOutputPlaybackParams,
                )
            }.apply {
                setExtensionRendererMode(DefaultRenderersFactory.EXTENSION_RENDERER_MODE_PREFER)
            }

        player =
            ExoPlayer.Builder(this, renderersFactory)
                // Media3 1.11's dynamic audio loop can sleep for half the output buffer.
                // Its non-offload currentPosition stays cached during that sleep, so
                // MediaSession publishes an old position with a new timestamp. Use the
                // 10 ms playback loop as a temporary workaround for:
                // https://github.com/androidx/media/issues/3286
                // TODO: After upgrading to a version containing the upstream fix,
                // verify release lyric/output clock alignment and re-enable dynamic
                // scheduling to restore its power savings.
                .experimentalSetDynamicSchedulingEnabled(false)
                .setAudioAttributes(
                    AudioAttributes.Builder()
                        .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
                        .setUsage(C.USAGE_MEDIA)
                        .build(),
                    true,
                )
                .build()

        val sessionActivityIntent =
            Intent(this, MainActivity::class.java).apply { flags = Intent.FLAG_ACTIVITY_SINGLE_TOP }

        PlaybackTimingTrace.player = player

        val sessionActivityPendingIntent =
            PendingIntent.getActivity(this, 0, sessionActivityIntent, PendingIntent.FLAG_IMMUTABLE)

        mediaSession =
            MediaSession.Builder(this, player)
                .setSessionActivity(sessionActivityPendingIntent)
                .build()
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? {
        return mediaSession
    }

    override fun onDestroy() {
        PlaybackTimingTrace.player = null
        AudioOutputTimingTrace.clear()
        mediaSession?.run {
            player.release()
            release()
            mediaSession = null
        }
        super.onDestroy()
    }
}
