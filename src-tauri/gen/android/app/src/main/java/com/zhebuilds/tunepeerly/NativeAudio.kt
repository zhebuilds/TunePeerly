// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import android.content.Context
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import org.json.JSONObject
import java.io.File

/** What the page knows about the song lined up next, so the notification can show it before the page catches up. */
data class NextSong(
  val id: Long,
  val url: String,
  val title: String,
  val artist: String,
  val album: String,
  val coverPath: String?,
  val favorite: Boolean,
  val duration: Double,
)

/**
 * The phone's audio output. The web view's audio element is throttled, paused or starved once the
 * screen is off, so on Android the page drives this ExoPlayer instead (see `src/player/native.ts`).
 * It holds the CPU and Wi-Fi awake while it plays, and when a song ends it starts the one the page
 * lined up next by itself, so moving on never waits for the page's script to run.
 * Everything here runs on the main thread.
 */
@OptIn(UnstableApi::class)
object NativeAudio {
  private const val TAG = "NativeAudio"
  private val main = Handler(Looper.getMainLooper())
  private var context: Context? = null
  private var player: ExoPlayer? = null
  /** The page's id of the song loaded now; 0 when nothing is loaded. */
  @Volatile var current = 0L
    private set
  private var next: NextSong? = null
  /** The newest page command applied, echoed so the page can ignore reports older than its last press. */
  private var seq = 0L
  private var ended = false
  private var volume = 1f
  private var errored = false
  private val ticker = object : Runnable {
    override fun run() {
      report()
      if (player?.isPlaying == true) main.postDelayed(this, 250)
    }
  }

  fun attach(context: Context) {
    this.context = context.applicationContext
  }

  /** The page wants sound: it pressed play and the song has not ended or failed. */
  val wantsPlay: Boolean
    get() {
      val exo = player ?: return false
      return current != 0L && exo.playWhenReady && !ended && !errored
    }

  val position: Double get() = (player?.currentPosition ?: 0L).coerceAtLeast(0L) / 1000.0

  val duration: Double
    get() {
      val value = player?.duration ?: C.TIME_UNSET
      return if (value == C.TIME_UNSET || value <= 0) 0.0 else value / 1000.0
    }

  private fun exo(): ExoPlayer? {
    player?.let { return it }
    val ctx = context ?: return null
    // Remote songs come from the app's own server on 127.0.0.1, which may wait up to 15 s for the other device.
    val http = DefaultHttpDataSource.Factory()
      .setConnectTimeoutMs(20_000)
      .setReadTimeoutMs(35_000)
      .setAllowCrossProtocolRedirects(false)
    val made = ExoPlayer.Builder(ctx)
      .setMediaSourceFactory(DefaultMediaSourceFactory(DefaultDataSource.Factory(ctx, http)))
      .setAudioAttributes(
        AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(),
        true,
      )
      .setHandleAudioBecomingNoisy(true)
      // Keeps the CPU and the Wi-Fi radio up while it plays, also with the screen off.
      .setWakeMode(C.WAKE_MODE_NETWORK)
      .build()
    made.volume = volume
    made.addListener(object : Player.Listener {
      override fun onPlaybackStateChanged(state: Int) {
        if (state == Player.STATE_ENDED && current != 0L && !ended) finished() else changed()
      }
      override fun onIsPlayingChanged(isPlaying: Boolean) {
        changed()
        if (isPlaying) {
          main.removeCallbacks(ticker)
          main.postDelayed(ticker, 250)
        }
      }
      override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) = changed()
      override fun onPositionDiscontinuity(old: Player.PositionInfo, new: Player.PositionInfo, reason: Int) = report()
      override fun onPlayerError(error: PlaybackException) {
        errored = true
        Log.w(TAG, "song $current: ${error.errorCodeName} ${error.message}")
        emit(JSONObject().put("t", "error").put("id", current).put("code", mediaErrorCode(error)).put("msg", error.errorCodeName))
        changed()
      }
    })
    player = made
    return made
  }

  /** HTMLMediaElement error codes, so the page's retry rules stay the same as on desktop. */
  private fun mediaErrorCode(error: PlaybackException): Int = when (error.errorCode) {
    in 3000..3999 -> if (error.errorCode == PlaybackException.ERROR_CODE_PARSING_CONTAINER_UNSUPPORTED ||
      error.errorCode == PlaybackException.ERROR_CODE_PARSING_MANIFEST_UNSUPPORTED) 4 else 3
    PlaybackException.ERROR_CODE_DECODING_FORMAT_UNSUPPORTED, PlaybackException.ERROR_CODE_DECODING_FORMAT_EXCEEDS_CAPABILITIES -> 4
    in 4000..4999 -> 3
    PlaybackException.ERROR_CODE_IO_FILE_NOT_FOUND -> 4
    else -> 2
  }

  private fun uriOf(url: String): Uri {
    val uri = Uri.parse(url)
    // Songs on the phone come as the web view's asset URL; ExoPlayer reads the file itself.
    if (uri.host == "asset.localhost") {
      val path = Uri.decode(url.substringAfter("asset.localhost/").substringBefore('?'))
      return Uri.fromFile(File(path))
    }
    return uri
  }

  /** Runs one command from the page. */
  fun command(op: String, args: JSONObject) {
    val sent = args.optLong("seq", 0L)
    if (sent > seq) seq = sent
    when (op) {
      "load" -> load(args.getLong("id"), args.getString("url"))
      "play" -> if (args.optLong("id") == current) setPlaying(true)
      "pause" -> if (args.optLong("id") == current) setPlaying(false)
      "seek" -> if (args.optLong("id") == current) seek(args.optDouble("sec", 0.0))
      "volume" -> {
        volume = args.optDouble("v", 1.0).toFloat().coerceIn(0f, 1f)
        player?.volume = volume
      }
      "stop" -> stop()
      "next" -> next = if (args.has("url")) NextSong(
        id = args.getLong("id"),
        url = args.getString("url"),
        title = args.optString("title"),
        artist = args.optString("artist"),
        album = args.optString("album"),
        coverPath = args.optString("coverPath").ifEmpty { null },
        favorite = args.optBoolean("favorite"),
        duration = args.optDouble("duration", 0.0),
      ) else null
    }
    report()
  }

  private fun load(id: Long, url: String) {
    val exo = exo() ?: return
    current = id
    ended = false
    errored = false
    exo.playWhenReady = false
    exo.setMediaItem(MediaItem.Builder().setMediaId(id.toString()).setUri(uriOf(url)).build())
    exo.prepare()
  }

  /** Also used by the notification and lock screen, so play and pause answer even before the page does. */
  fun setPlaying(play: Boolean) {
    val exo = player ?: return
    if (current == 0L) return
    if (play && (errored || exo.playbackState == Player.STATE_IDLE)) {
      errored = false
      exo.prepare()
    }
    if (play && ended) {
      ended = false
      exo.seekTo(0)
    }
    exo.playWhenReady = play
  }

  private fun seek(seconds: Double) {
    val exo = player ?: return
    ended = false
    if (errored || exo.playbackState == Player.STATE_IDLE) {
      errored = false
      exo.prepare()
    }
    exo.seekTo((seconds * 1000).toLong().coerceAtLeast(0))
  }

  fun stop() {
    current = 0L
    ended = false
    errored = false
    main.removeCallbacks(ticker)
    player?.let {
      it.playWhenReady = false
      it.stop()
      it.clearMediaItems()
    }
  }

  fun release() {
    stop()
    next = null
    player?.release()
    player = null
  }

  /** The song played to its end. Start the next one now; the page adopts it when its script runs. */
  private fun finished() {
    val exo = player ?: return
    val from = current
    val upcoming = next
    next = null
    if (upcoming == null || upcoming.id == from) {
      ended = true
      report()
      changed()
      return
    }
    current = upcoming.id
    ended = false
    errored = false
    exo.setMediaItem(MediaItem.Builder().setMediaId(upcoming.id.toString()).setUri(uriOf(upcoming.url)).build())
    exo.prepare()
    exo.playWhenReady = true
    val before = NowPlaying.snapshot
    NowPlaying.update(
      before.copy(
        title = upcoming.title,
        artist = upcoming.artist,
        album = upcoming.album,
        coverPath = upcoming.coverPath,
        favorite = upcoming.favorite,
        duration = upcoming.duration,
        position = 0.0,
        playing = true,
        at = System.currentTimeMillis(),
      )
    )
    context?.let { NowPlayingWidget.refresh(it) }
    emit(JSONObject().put("t", "advanced").put("from", from).put("to", upcoming.id))
    report()
  }

  /** Play state changed: tell the page, and keep the notification, lock screen and widget in step. */
  private fun changed() {
    report()
    if (current == 0L) return
    val playing = wantsPlay
    val before = NowPlaying.snapshot
    if (before.hasTrack && before.playing != playing) {
      NowPlaying.update(before.copy(playing = playing, position = position, at = System.currentTimeMillis()))
      context?.let { NowPlayingWidget.refresh(it) }
    }
  }

  private fun report() {
    val exo = player ?: return
    if (current == 0L) return
    val buffered = exo.bufferedPosition.coerceAtLeast(0L) / 1000.0
    emit(
      JSONObject()
        .put("t", "state")
        .put("id", current)
        .put("seq", seq)
        .put("pos", position)
        .put("dur", duration)
        .put("buf", buffered)
        .put("pwr", exo.playWhenReady)
        .put("playing", exo.isPlaying)
        .put("ready", exo.playbackState == Player.STATE_READY || (exo.playbackState == Player.STATE_ENDED && !errored))
        .put("ended", ended)
    )
  }

  private fun emit(event: JSONObject) {
    TunePlugin.instance?.audioEvent(event.toString())
  }
}
