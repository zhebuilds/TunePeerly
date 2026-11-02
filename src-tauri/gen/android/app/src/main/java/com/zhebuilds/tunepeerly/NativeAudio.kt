// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import android.content.Context
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.database.StandaloneDatabaseProvider
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.datasource.cache.CacheDataSource
import androidx.media3.datasource.cache.CacheWriter
import androidx.media3.datasource.cache.LeastRecentlyUsedCacheEvictor
import androidx.media3.datasource.cache.SimpleCache
import androidx.media3.exoplayer.DefaultLoadControl
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.exoplayer.upstream.DefaultLoadErrorHandlingPolicy
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.concurrent.Executors

/** A song the phone's player may start by itself. `url` is set for songs on the phone; others stream by content id. */
data class Song(
  val key: String,
  val url: String?,
  val contentId: String?,
  val title: String,
  val artist: String,
  val album: String,
  val coverPath: String?,
  val favorite: Boolean,
  val duration: Double,
)

/**
 * The phone's audio output and, while the page's script is held back, its queue. The web view's audio
 * element is throttled, paused or starved once the screen is off, so on Android the page drives this
 * ExoPlayer instead (see `src/player/native.ts`).
 *
 * The page hands over every song it may play (`catalog`) and the whole queue with its rules (`queue`:
 * what is next, repeat, shuffle order, the list to start over from and the library to continue with).
 * When a song ends, or next or previous is pressed on the notification, the next song is chosen here
 * with the same rules as `src/player/advance.ts` and started at once, so the music never waits for the
 * page. Every change is reported back (`queue` events) and the page takes it over when it runs again.
 *
 * Remote songs play from the app's stream URLs, which pick a connected device on every request. A short
 * drop does not stop the music: the whole current song is loaded into memory ahead of time, the next
 * remote song is copied into a small cache on disk while this one plays, a load that fails is retried
 * in the background, and a song that cannot load at all is retried for a few minutes (the link is
 * called again meanwhile, see `Redial` in net.rs) before it counts as failed.
 *
 * Everything here runs on the main thread. Every stop, pause, skip, retry and error is logged with the
 * tag `NativeAudio`.
 */
@OptIn(UnstableApi::class)
object NativeAudio {
  private const val TAG = "NativeAudio"
  /** How long a remote song that cannot load is tried again before it counts as failed. */
  private const val RETRY_WINDOW_MS = 5 * 60_000L
  private const val RETRY_MAX_WAIT_MS = 10_000L
  private const val MAX_FAILED_IN_ROW = 3
  /** Remote songs copied ahead of time, least recently used dropped first. */
  private const val CACHE_BYTES = 200L * 1024 * 1024
  /** The current song is loaded this far ahead into memory (a whole song, as a rule). */
  private const val BUFFER_MS = 15 * 60_000
  private const val BUFFER_BYTES = 48 * 1024 * 1024
  private val main = Handler(Looper.getMainLooper())
  private var context: Context? = null
  private var player: ExoPlayer? = null

  /** The id of the song loaded now; 0 when nothing is loaded. The page numbers its loads from 1; ids picked here are larger. */
  @Volatile var current = 0L
    private set
  private var currentKey: String? = null
  private var currentUrl: String? = null
  private var nativeIds = 1_000_000_000L
  /** The newest page command applied, echoed so the page can ignore reports older than its last press. */
  private var seq = 0L
  private var ended = false
  private var volume = 1f
  private var errored = false

  // The queue, as last sent by the page and then moved on here.
  private val catalog = HashMap<String, Song>()
  private var streamBase = ""
  private val queue = NativeQueue({ playable(it) })
  /** Counts the changes made here. The page sends the one it saw last, so a stale queue never undoes a skip. */
  private var rev = 0L

  private var retries = 0
  /** When the current song first failed to load since it last played (elapsed realtime); 0 when it did not. */
  private var failingSince = 0L
  private var cache: SimpleCache? = null
  private var cacheFactory: CacheDataSource.Factory? = null
  private val prefetcher = Executors.newSingleThreadExecutor()
  private var prefetchKey: String? = null
  private var prefetchWriter: CacheWriter? = null
  /** When a copy ahead last failed; the next try waits a little (elapsed realtime). */
  private var prefetchFailedAt = 0L
  /** A song on the phone that could not be read is tried once from a linked device. */
  private var triedStream = false
  private var failedInRow = 0
  private var heardMs = 0L
  private var heardSent = false
  private var lastTick = 0L

  private val ticker = object : Runnable {
    override fun run() {
      tick()
      report()
      if (player?.isPlaying == true) main.postDelayed(this, 250)
    }
  }
  private var retry: Runnable? = null

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

  /** What is loaded now, when the page told about it. */
  val currentSong: Song? get() = if (current != 0L) currentKey?.let { catalog[it] } else null

  private fun exo(): ExoPlayer? {
    player?.let { return it }
    val ctx = context ?: return null
    // Remote songs come from the app's own server on 127.0.0.1, which may wait up to 15 s for the other device.
    val http = DefaultHttpDataSource.Factory()
      .setConnectTimeoutMs(20_000)
      .setReadTimeoutMs(35_000)
      .setAllowCrossProtocolRedirects(false)
    // Stream URLs go through the cache, so a song copied ahead of time plays from disk. Files on the
    // phone are read directly (DefaultDataSource only hands http to the cache).
    val cached = openCache(ctx, http)
    // A load that fails is tried again in the background for a few minutes while what is loaded plays.
    val retryLoads = object : DefaultLoadErrorHandlingPolicy() {
      override fun getMinimumLoadableRetryCount(dataType: Int) = 60
    }
    val loadControl = DefaultLoadControl.Builder()
      .setBufferDurationsMs(BUFFER_MS, BUFFER_MS, 2_500, 5_000)
      .setTargetBufferBytes(BUFFER_BYTES)
      .setPrioritizeTimeOverSizeThresholds(false)
      .build()
    val made = ExoPlayer.Builder(ctx)
      .setMediaSourceFactory(
        DefaultMediaSourceFactory(DefaultDataSource.Factory(ctx, cached ?: http)).setLoadErrorHandlingPolicy(retryLoads)
      )
      .setLoadControl(loadControl)
      .setAudioAttributes(
        AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build(),
        true,
      )
      .setHandleAudioBecomingNoisy(true)
      // Keeps the CPU and the Wi-Fi radio up while it plays or loads, also with the screen off.
      .setWakeMode(C.WAKE_MODE_NETWORK)
      .build()
    made.volume = volume
    made.addListener(object : Player.Listener {
      override fun onPlaybackStateChanged(state: Int) {
        Log.i(TAG, "state ${stateName(state)} song $current ($currentKey)")
        if (state == Player.STATE_ENDED && current != 0L && !ended) finished() else changed()
      }
      override fun onIsPlayingChanged(isPlaying: Boolean) {
        changed()
        if (isPlaying) {
          retries = 0
          failingSince = 0L
          failedInRow = 0
          lastTick = SystemClock.elapsedRealtime()
          main.removeCallbacks(ticker)
          main.postDelayed(ticker, 250)
        }
      }
      override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
        Log.i(TAG, "${if (playWhenReady) "play" else "pause"} song $current: ${pwrReason(reason)}")
        changed()
      }
      override fun onPlaybackSuppressionReasonChanged(reason: Int) {
        if (reason != Player.PLAYBACK_SUPPRESSION_REASON_NONE) Log.i(TAG, "sound held back (reason $reason), e.g. audio focus lost for a moment")
      }
      override fun onPositionDiscontinuity(old: Player.PositionInfo, new: Player.PositionInfo, reason: Int) = report()
      override fun onPlayerError(error: PlaybackException) = failed(error)
    })
    player = made
    return made
  }

  /** The disk cache for remote songs, opened once per process. Null when it cannot be opened. */
  private fun openCache(ctx: Context, http: DefaultHttpDataSource.Factory): CacheDataSource.Factory? {
    cacheFactory?.let { return it }
    val opened = cache ?: runCatching {
      SimpleCache(File(ctx.cacheDir, "stream-cache"), LeastRecentlyUsedCacheEvictor(CACHE_BYTES), StandaloneDatabaseProvider(ctx))
    }.onFailure { Log.w(TAG, "stream cache: ${it.message}") }.getOrNull()
    cache = opened ?: return null
    val factory = CacheDataSource.Factory()
      .setCache(opened)
      .setUpstreamDataSourceFactory(http)
      .setFlags(CacheDataSource.FLAG_IGNORE_CACHE_ON_ERROR)
    cacheFactory = factory
    return factory
  }

  /** Songs are content-addressed, so the cache key is the content id, whatever the URL's secret is. */
  private fun cacheKey(contentId: String) = "c:$contentId"

  private fun mediaItem(id: Long, url: String, key: String?): MediaItem {
    // The song being copied ahead starts now: the player takes over from what was copied so far.
    if (key != null && key == prefetchKey && prefetchWriter != null) cancelPrefetch()
    val builder = MediaItem.Builder().setMediaId(id.toString()).setUri(uriOf(url))
    val song = key?.let { catalog[it] }
    if (url.startsWith("http://127.0.0.1") && song?.contentId != null) builder.setCustomCacheKey(cacheKey(song.contentId))
    return builder.build()
  }

  /**
   * Copies the next remote song into the disk cache once the current one is loaded, so it starts at
   * once and plays through a short drop. One copy at a time; a copy that failed is tried again later.
   */
  private fun prefetchNext() {
    val factory = cacheFactory ?: return
    if (streamBase.isEmpty()) return
    val key = queue.future.firstOrNull { playable(it) } ?: return
    val song = catalog[key] ?: return
    val contentId = song.contentId
    if (song.url != null || contentId == null) return
    if (prefetchKey == key) return
    if (prefetchFailedAt != 0L && SystemClock.elapsedRealtime() - prefetchFailedAt < 15_000) return
    cancelPrefetch()
    val spec = DataSpec.Builder().setUri(Uri.parse(streamBase + contentId)).setKey(cacheKey(contentId)).build()
    val writer = CacheWriter(factory.createDataSource(), spec, null, null)
    prefetchKey = key
    prefetchWriter = writer
    Log.i(TAG, "copy the next song ($key) ahead of time")
    prefetcher.execute {
      val ok = runCatching { writer.cache() }
        .onFailure { Log.i(TAG, "copy ahead of $key stopped: ${it.message}") }
        .isSuccess
      main.post {
        if (prefetchWriter !== writer) return@post
        prefetchWriter = null
        if (ok) {
          prefetchFailedAt = 0L
          Log.i(TAG, "next song ($key) copied ahead")
        } else {
          prefetchFailedAt = SystemClock.elapsedRealtime()
          prefetchKey = null
        }
      }
    }
  }

  private fun cancelPrefetch() {
    prefetchWriter?.cancel()
    prefetchWriter = null
    prefetchKey = null
  }

  /** The phone is on a network again: a song waiting to be retried is tried now. */
  fun networkBack() {
    prefetchFailedAt = 0L
    val pending = retry ?: return
    Log.i(TAG, "network back: retry song $current now")
    main.removeCallbacks(pending)
    pending.run()
  }

  private fun stateName(state: Int) = when (state) {
    Player.STATE_IDLE -> "idle"
    Player.STATE_BUFFERING -> "buffering"
    Player.STATE_READY -> "ready"
    Player.STATE_ENDED -> "ended"
    else -> "$state"
  }

  private fun pwrReason(reason: Int) = when (reason) {
    Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST -> "asked (page, notification or lock screen)"
    Player.PLAY_WHEN_READY_CHANGE_REASON_AUDIO_FOCUS_LOSS -> "audio focus lost to another app"
    Player.PLAY_WHEN_READY_CHANGE_REASON_AUDIO_BECOMING_NOISY -> "headphones or Bluetooth disconnected"
    Player.PLAY_WHEN_READY_CHANGE_REASON_REMOTE -> "remote"
    Player.PLAY_WHEN_READY_CHANGE_REASON_END_OF_MEDIA_ITEM -> "end of song"
    else -> "reason $reason"
  }

  /** HTMLMediaElement error codes, as the page expects them. */
  private fun mediaErrorCode(error: PlaybackException): Int = when (error.errorCode) {
    in 3000..3999 -> if (error.errorCode == PlaybackException.ERROR_CODE_PARSING_CONTAINER_UNSUPPORTED ||
      error.errorCode == PlaybackException.ERROR_CODE_PARSING_MANIFEST_UNSUPPORTED) 4 else 3
    PlaybackException.ERROR_CODE_DECODING_FORMAT_UNSUPPORTED, PlaybackException.ERROR_CODE_DECODING_FORMAT_EXCEEDS_CAPABILITIES -> 4
    in 4000..4999 -> 3
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
      "load" -> load(args.getLong("id"), args.getString("url"), args.text("key"))
      "play" -> if (args.optLong("id") == current) setPlaying(true)
      "pause" -> if (args.optLong("id") == current) setPlaying(false)
      "seek" -> if (args.optLong("id") == current) seek(args.optDouble("sec", 0.0))
      "volume" -> {
        volume = args.optDouble("v", 1.0).toFloat().coerceIn(0f, 1f)
        player?.volume = volume
      }
      "stop" -> {
        Log.i(TAG, "stop: asked by the page (song $current)")
        stop()
      }
      "catalog" -> setCatalog(args)
      "queue" -> setQueue(args)
      "sync" -> {
        emitQueue("sync")
        if (current != 0L) {
          Log.i(TAG, "page asked where things are: song $current ($currentKey)")
        }
      }
    }
    report()
  }

  private fun setCatalog(args: JSONObject) {
    args.text("base")?.let { streamBase = it }
    val songs = args.optJSONArray("songs") ?: return
    // The song playing now stays known even if the new list leaves it out.
    val playing = currentKey?.let { catalog[it] }
    catalog.clear()
    playing?.let { catalog[it.key] = it }
    for (i in 0 until songs.length()) {
      val row = songs.optJSONObject(i) ?: continue
      val key = row.text("k") ?: continue
      catalog[key] = Song(
        key = key,
        url = row.text("u"),
        contentId = row.text("c"),
        title = row.text("t") ?: "",
        artist = row.text("a") ?: "",
        album = row.text("l") ?: "",
        coverPath = row.text("p"),
        favorite = row.optBoolean("f"),
        duration = row.optDouble("d", 0.0),
      )
    }
    Log.i(TAG, "catalog: ${catalog.size} songs")
  }

  private fun keys(array: JSONArray?): ArrayList<String> {
    val out = ArrayList<String>()
    if (array == null) return out
    for (i in 0 until array.length()) (if (array.isNull(i)) null else array.optString(i).ifEmpty { null })?.let { out.add(it) }
    return out
  }

  private fun setQueue(args: JSONObject) {
    val seen = args.optLong("rev", -1L)
    if (seen != rev) {
      // The page had not seen the latest move made here yet. It gets that report and sends its queue again.
      Log.i(TAG, "queue from the page skipped: it saw change $seen, now at $rev; asking for it again")
      emitQueue("stale")
      return
    }
    queue.past = keys(args.optJSONArray("past"))
    queue.current = args.text("current") ?: currentKey
    queue.future = keys(args.optJSONArray("future"))
    queue.repeat = args.optString("repeat", "off")
    queue.order = args.optString("order", "manual")
    queue.contextId = args.optString("contextId", "library")
    if (args.has("pool")) queue.pool = keys(args.optJSONArray("pool"))
    if (args.has("library")) queue.library = keys(args.optJSONArray("library"))
    if (prefetchKey != null && prefetchKey != queue.future.firstOrNull { playable(it) }) cancelPrefetch()
  }

  private fun load(id: Long, url: String, key: String?) {
    val exo = exo() ?: return
    cancelRetry()
    current = id
    currentKey = key
    if (key != null) queue.current = key
    currentUrl = url
    ended = false
    errored = false
    retries = 0
    failingSince = 0L
    triedStream = false
    heardMs = 0
    heardSent = false
    exo.playWhenReady = false
    exo.setMediaItem(mediaItem(id, url, key))
    exo.prepare()
    Log.i(TAG, "load song $id ($key) from the page")
  }

  /** Also used by the notification and lock screen, so play and pause answer even before the page does. */
  fun setPlaying(play: Boolean) {
    val exo = player ?: return
    if (current == 0L) return
    if (play && (errored || exo.playbackState == Player.STATE_IDLE)) {
      errored = false
      retries = 0
      failingSince = 0L
      cancelRetry()
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
    currentKey = null
    currentUrl = null
    ended = false
    errored = false
    cancelRetry()
    main.removeCallbacks(ticker)
    player?.let {
      it.playWhenReady = false
      it.stop()
      it.clearMediaItems()
    }
  }

  fun release() {
    Log.i(TAG, "release: the player is closed")
    cancelPrefetch()
    stop()
    player?.release()
    player = null
  }

  /**
   * A press outside the page: the notification, lock screen, headset, Bluetooth or the widget.
   * Returns false when the page has to handle it (nothing is loaded here).
   */
  fun press(action: String): Boolean {
    if (current == 0L || player == null) return false
    when {
      action == "next" -> skip(1)
      action == "previous" -> skip(-1)
      action == "play" -> setPlaying(true)
      action == "pause" -> setPlaying(false)
      action == "toggle" -> setPlaying(!wantsPlay)
      action.startsWith("seek:") -> action.removePrefix("seek:").toDoubleOrNull()?.let { seek(it) } ?: return false
      else -> return false
    }
    Log.i(TAG, "press $action handled here")
    return true
  }

  private fun playable(key: String) = catalog[key]?.let { it.url != null || (it.contentId != null && streamBase.isNotEmpty()) } ?: false

  fun skip(delta: Int) {
    val exo = player ?: return
    val moved = if (delta > 0) queue.next() else queue.previous(position)
    if (!moved) {
      Log.i(TAG, "${if (delta > 0) "next" else "previous"}: start the same song over")
      ended = false
      errored = false
      exo.seekTo(0)
      exo.playWhenReady = true
      return
    }
    startQueued(if (delta > 0) "next pressed" else "previous pressed")
  }

  /** Starts the queue's current song, or stops when there is none. */
  private fun startQueued(reason: String) {
    val key = queue.current
    val song = key?.let { catalog[it] }
    val exo = player
    if (key == null || song == null || exo == null) {
      Log.i(TAG, "stop: nothing left to play after $reason")
      queue.current = currentKey
      // The song that is loaded keeps playing; only a song that already ran to its end counts as ended.
      if (exo?.playbackState == Player.STATE_ENDED) ended = true
      rev++
      emitQueue("$reason, nothing left")
      report()
      changed()
      return
    }
    cancelRetry()
    val url = song.url ?: (streamBase + song.contentId)
    current = ++nativeIds
    currentKey = key
    currentUrl = url
    ended = false
    errored = false
    retries = 0
    failingSince = 0L
    triedStream = false
    heardMs = 0
    heardSent = false
    exo.setMediaItem(mediaItem(current, url, key))
    exo.prepare()
    exo.playWhenReady = true
    Log.i(TAG, "start song $current ($key, ${if (song.url != null) "on the phone" else "remote"}): $reason")
    NowPlaying.update(
      NowPlaying.snapshot.copy(
        title = song.title,
        artist = song.artist,
        album = song.album,
        coverPath = song.coverPath,
        favorite = song.favorite,
        duration = song.duration,
        position = 0.0,
        playing = true,
        at = System.currentTimeMillis(),
      )
    )
    context?.let { NowPlayingWidget.refresh(it) }
    rev++
    emitQueue(reason)
    report()
  }

  /** The song played to its end. Start the next one now; the page takes it over when its script runs. */
  private fun finished() {
    val exo = player ?: return
    countHeard(ended = true)
    if (queue.repeat == "one" && currentKey != null) {
      Log.i(TAG, "song $current ended: repeat one")
      heardMs = 0
      heardSent = false
      exo.seekTo(0)
      exo.playWhenReady = true
      emit(JSONObject().put("t", "restarted").put("id", current))
      return
    }
    if (!queue.afterEnd()) {
      queue.current = currentKey
      Log.i(TAG, "stop: song $current ended and the queue, repeat and the library have nothing more")
      ended = true
      report()
      changed()
      return
    }
    startQueued("song ended")
  }

  private fun cancelRetry() {
    retry?.let { main.removeCallbacks(it) }
    retry = null
  }

  /** A dropped link or a device that went away is tried again; a broken file or format is not. */
  private fun failed(error: PlaybackException) {
    val exo = player ?: return
    val id = current
    val remote = currentUrl?.startsWith("http://127.0.0.1") == true
    val transient = remote && error.errorCode in 1000..2999
    Log.w(TAG, "error on song $id ($currentKey): ${error.errorCodeName} ${error.message} (cause ${error.cause?.message})")
    val now = SystemClock.elapsedRealtime()
    if (transient && failingSince == 0L) failingSince = now
    // No device has the song right now (503) or the link dropped: the link is being called again, so
    // keep trying for a few minutes. The music counts as playing meanwhile, which keeps the locks.
    if (transient && id != 0L && now - failingSince < RETRY_WINDOW_MS) {
      retries++
      val wait = (1000L shl (retries - 1).coerceAtMost(4)).coerceAtMost(RETRY_MAX_WAIT_MS)
      val at = exo.currentPosition.coerceAtLeast(0L)
      Log.i(TAG, "retry $retries for song $id at ${at / 1000.0}s in ${wait}ms (failing for ${(now - failingSince) / 1000}s)")
      cancelRetry()
      val again = Runnable {
        retry = null
        if (current != id) return@Runnable
        Log.i(TAG, "retrying song $id at ${at / 1000.0}s")
        exo.prepare()
        if (at > 0) exo.seekTo(at)
      }
      retry = again
      main.postDelayed(again, wait)
      report()
      return
    }
    val song = currentKey?.let { catalog[it] }
    if (!remote && !triedStream && id != 0L && song?.contentId != null && streamBase.isNotEmpty()) {
      triedStream = true
      val url = streamBase + song.contentId
      val at = exo.currentPosition.coerceAtLeast(0L)
      Log.i(TAG, "song $id: the file on the phone failed, trying a linked device")
      currentUrl = url
      exo.setMediaItem(mediaItem(id, url, currentKey), at)
      exo.prepare()
      return
    }
    errored = true
    emit(JSONObject().put("t", "error").put("id", id).put("code", mediaErrorCode(error)).put("msg", error.errorCodeName))
    changed()
    failedInRow++
    val wanted = exo.playWhenReady
    if (!wanted || failedInRow > MAX_FAILED_IN_ROW || currentKey == null) {
      Log.w(TAG, "stop: song $id failed (${error.errorCodeName}); ${failedInRow} failed in a row")
      return
    }
    // Like a song that ended, so a missing song does not stop the music with the screen off.
    Log.w(TAG, "song $id failed (${error.errorCodeName}), moving on ($failedInRow in a row)")
    main.postDelayed({
      if (current != id || !errored) return@postDelayed
      if (queue.afterEnd()) startQueued("previous song failed") else Log.w(TAG, "stop: nothing to move on to after a failed song")
    }, 1000)
  }

  private fun tick() {
    val now = SystemClock.elapsedRealtime()
    val delta = now - lastTick
    lastTick = now
    if (player?.isPlaying == true && delta in 1..1500) heardMs += delta
    countHeard(ended = false)
  }

  /** The page records a song in Recently Played once enough of it was heard, also when it played here alone. */
  private fun countHeard(ended: Boolean) {
    if (heardSent) return
    val key = currentKey ?: return
    val length = duration
    val needed = if (length > 0) minOf(10_000.0, length * 500.0) else 10_000.0
    if (heardMs >= needed || (ended && heardMs > 1000)) {
      heardSent = true
      emit(JSONObject().put("t", "heard").put("id", current).put("key", key))
    }
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

  private fun emitQueue(reason: String) {
    val event = JSONObject()
      .put("t", "queue")
      .put("rev", rev)
      .put("seq", seq)
      .put("reason", reason)
      .put("id", current)
      .put("past", JSONArray(queue.past))
      .put("current", currentKey ?: JSONObject.NULL)
      .put("future", JSONArray(queue.future))
      .put("repeat", queue.repeat)
      .put("order", queue.order)
      .put("contextId", queue.contextId)
      .put("playing", wantsPlay)
      .put("ended", ended)
      .put("pos", position)
    emit(event)
  }

  private fun report() {
    val exo = player ?: return
    if (current == 0L) return
    val buffered = exo.bufferedPosition.coerceAtLeast(0L) / 1000.0
    // The current song is loaded to its end: copy the next one ahead now.
    if (exo.isPlaying && duration > 0 && buffered >= duration - 1.0 && prefetchWriter == null) prefetchNext()
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

/**
 * A string field, or null when it is missing, empty or JSON null. `optString` turns a JSON null
 * into the text "null", which made every remote song (sent with `u: null`) look like a file on the
 * phone, so the next song never started.
 */
internal fun JSONObject.text(name: String): String? =
  if (isNull(name)) null else optString(name).ifEmpty { null }
