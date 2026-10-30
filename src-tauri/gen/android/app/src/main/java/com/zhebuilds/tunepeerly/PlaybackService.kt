// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.drawable.Icon
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.IBinder
import org.json.JSONObject

/**
 * What the web view is playing. The web view owns the audio element, so Android needs a copy of the
 * title, the artist and the position to describe playback outside the app.
 */
object PlaybackSnapshot {
    @Volatile
    var title: String = ""

    @Volatile
    var artist: String = ""

    @Volatile
    var playing: Boolean = false

    @Volatile
    var positionMs: Long = 0

    @Volatile
    var durationMs: Long = 0

    val hasTrack: Boolean
        get() = title.isNotBlank()

    fun apply(json: String) {
        val data = JSONObject(json)
        title = data.optString("title")
        artist = data.optString("artist")
        playing = data.optBoolean("playing")
        positionMs = data.optLong("positionMs")
        durationMs = data.optLong("durationMs")
    }
}

/**
 * Handed to Rust. The media session runs on Android threads, and the player lives in the web view,
 * so the action goes out as a Tauri event that the front end turns into `toggle`/`next`/`previous`.
 */
object NativeBridge {
    external fun mediaAction(action: String, positionMs: Long)
}

/**
 * Keeps playback alive while the app is in the background: a foreground service so Android does not
 * freeze the process, a media session so the lock screen and headphones can drive it, and a media
 * notification with the three buttons people expect.
 */
class PlaybackService : Service() {
    private var session: MediaSession? = null
    private var focus: AudioFocusRequest? = null

    private val audio: AudioManager by lazy {
        getSystemService(Context.AUDIO_SERVICE) as AudioManager
    }

    private val focusChange = AudioManager.OnAudioFocusChangeListener { change ->
        if (change == AudioManager.AUDIOFOCUS_LOSS) dispatch(ACTION_PAUSE)
    }

    override fun onCreate() {
        super.onCreate()
        session = MediaSession(this, "TunePeerly").apply {
            setCallback(
                object : MediaSession.Callback() {
                    override fun onPlay() = dispatch(ACTION_PLAY)

                    override fun onPause() = dispatch(ACTION_PAUSE)

                    override fun onSkipToNext() = dispatch(ACTION_NEXT)

                    override fun onSkipToPrevious() = dispatch(ACTION_PREVIOUS)

                    override fun onStop() = dispatch(ACTION_STOP)
                }
            )
            isActive = true
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_PLAY, ACTION_PAUSE, ACTION_NEXT, ACTION_PREVIOUS -> dispatch(intent.action!!)
            ACTION_STOP -> {
                session?.isActive = false
                stopSelf()
                return START_NOT_STICKY
            }
        }
        // The system expects startForeground within five seconds of startForegroundService, so the
        // notification is drawn even when nothing is playing yet.
        publish()
        return START_STICKY
    }

    override fun onDestroy() {
        session?.isActive = false
        session?.release()
        session = null
        abandonFocus()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun dispatch(action: String) {
        // The player lives in the web view; Rust owns the bridge back into it.
        runCatching { NativeBridge.mediaAction(action, PlaybackSnapshot.positionMs) }
    }

    /** Mirrors the snapshot into the notification and the media session, then gets out of the way. */
    private fun publish() {
        createChannel()
        val snapshot = PlaybackSnapshot
        session?.setMetadata(
            MediaMetadata.Builder()
                .putString(MediaMetadata.METADATA_KEY_TITLE, snapshot.title)
                .putString(MediaMetadata.METADATA_KEY_ARTIST, snapshot.artist)
                .putLong(MediaMetadata.METADATA_KEY_DURATION, snapshot.durationMs)
                .build()
        )
        session?.setPlaybackState(
            PlaybackState.Builder()
                .setActions(
                    PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or
                        PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_SKIP_TO_NEXT or
                        PlaybackState.ACTION_SKIP_TO_PREVIOUS or PlaybackState.ACTION_STOP or
                        PlaybackState.ACTION_SEEK_TO
                )
                .setState(
                    if (snapshot.playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED,
                    snapshot.positionMs,
                    if (snapshot.playing) 1f else 0f
                )
                .build()
        )
        // Asking every second would just pile up requests; only take focus when it is missing.
        if (snapshot.playing && focus == null) requestFocus()
        if (!snapshot.playing) abandonFocus()
        val notification = notification()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        if (!snapshot.hasTrack) stopSelf()
    }

    @Suppress("DEPRECATION")
    private fun notification(): Notification {
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).apply { flags = Intent.FLAG_ACTIVITY_SINGLE_TOP },
            flags()
        )
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL)
        } else {
            Notification.Builder(this)
        }
        builder
            .setSmallIcon(R.drawable.ic_stat_playback)
            .setContentTitle(PlaybackSnapshot.title.ifBlank { getString(R.string.notification_idle) })
            .setContentText(PlaybackSnapshot.artist)
            .setContentIntent(open)
            .setOngoing(PlaybackSnapshot.playing)
            .setOnlyAlertOnce(true)
            .setShowWhen(false)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .addAction(
                Notification.Action.Builder(
                    Icon.createWithResource(this, R.drawable.ic_stat_previous),
                    getString(R.string.notification_previous),
                    action(ACTION_PREVIOUS)
                ).build()
            )
            .addAction(
                Notification.Action.Builder(
                    Icon.createWithResource(this, if (PlaybackSnapshot.playing) R.drawable.ic_stat_pause else R.drawable.ic_stat_play),
                    getString(if (PlaybackSnapshot.playing) R.string.notification_pause else R.string.notification_play),
                    action(if (PlaybackSnapshot.playing) ACTION_PAUSE else ACTION_PLAY)
                ).build()
            )
            .addAction(
                Notification.Action.Builder(
                    Icon.createWithResource(this, R.drawable.ic_stat_next),
                    getString(R.string.notification_next),
                    action(ACTION_NEXT)
                ).build()
            )
        val style = Notification.MediaStyle().setShowActionsInCompactView(0, 1, 2)
        session?.let { style.setMediaSession(it.sessionToken) }
        builder.setStyle(style)
        return builder.build()
    }

    private fun action(action: String): PendingIntent = PendingIntent.getService(
        this,
        action.hashCode(),
        Intent(this, PlaybackService::class.java).setAction(action),
        flags()
    )

    private fun flags(): Int =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        } else {
            PendingIntent.FLAG_UPDATE_CURRENT
        }

    private fun createChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = getSystemService(NotificationManager::class.java) ?: return
        if (manager.getNotificationChannel(CHANNEL) != null) return
        val channel = NotificationChannel(CHANNEL, getString(R.string.notification_channel), NotificationManager.IMPORTANCE_LOW)
        channel.setShowBadge(false)
        channel.lockscreenVisibility = Notification.VISIBILITY_PUBLIC
        manager.createNotificationChannel(channel)
    }

    private fun requestFocus() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                .setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                        .build()
                )
                .setOnAudioFocusChangeListener(focusChange)
                .build()
            focus = request
            audio.requestAudioFocus(request)
        } else {
            @Suppress("DEPRECATION")
            audio.requestAudioFocus(focusChange, AudioManager.STREAM_MUSIC, AudioManager.AUDIOFOCUS_GAIN)
        }
    }

    private fun abandonFocus() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            focus?.let { audio.abandonAudioFocusRequest(it) }
            focus = null
        } else {
            @Suppress("DEPRECATION")
            audio.abandonAudioFocus(focusChange)
        }
    }

    companion object {
        private const val CHANNEL = "playback"
        private const val NOTIFICATION_ID = 4102

        const val ACTION_UPDATE = "com.zhebuilds.tunepeerly.playback.UPDATE"
        const val ACTION_PLAY = "com.zhebuilds.tunepeerly.playback.PLAY"
        const val ACTION_PAUSE = "com.zhebuilds.tunepeerly.playback.PAUSE"
        const val ACTION_NEXT = "com.zhebuilds.tunepeerly.playback.NEXT"
        const val ACTION_PREVIOUS = "com.zhebuilds.tunepeerly.playback.PREVIOUS"
        const val ACTION_STOP = "com.zhebuilds.tunepeerly.playback.STOP"

        /** Stops the service: the player it mirrored is gone with the activity. */
        fun shutdown(context: Context) {
            context.stopService(Intent(context, PlaybackService::class.java))
        }

        /** Starts or stops the service to match the snapshot the activity just wrote. */
        fun sync(context: Context) {
            val intent = Intent(context, PlaybackService::class.java).setAction(ACTION_UPDATE)
            if (!PlaybackSnapshot.hasTrack) {
                context.stopService(Intent(context, PlaybackService::class.java))
                return
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }
    }
}
