// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.LinkProperties
import android.net.Network
import android.net.NetworkCapabilities
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.DocumentsContract
import android.provider.Settings
import android.provider.OpenableColumns
import android.util.Base64
import android.util.Log
import android.webkit.WebView
import androidx.activity.result.ActivityResult
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject
import java.io.File
import java.util.concurrent.Executors

@InvokeArg
class BlobArgs {
  var data: String = ""
}

@InvokeArg
class PickArgs {
  var folder: Boolean = false
  var dest: String = ""
}

@InvokeArg
class NetworkWatchArgs {
  lateinit var onEvent: Channel
}

@InvokeArg
class TipArgs {
  var like: String = "Like"
}

@InvokeArg
class TrackArgs {
  var title: String = ""
  var artist: String = ""
  var album: String = ""
  var coverPath: String? = null
  var playing: Boolean = false
  var favorite: Boolean = false
  var duration: Double = 0.0
  var position: Double = 0.0
  var theme: String = "dark"
  var language: String = "en"
  var tips: TipArgs? = null
}

/**
 * The Android half of the app: the Keystore for the database key, the Wi-Fi multicast lock that lets
 * discovery packets in, the system picker for adding songs, and the media session mirror. Actions from
 * the notification, the lock screen, headsets and the widget go back to the web view as
 * `desktop_command`, the same path the Windows media controls use.
 */
@TauriPlugin
class TunePlugin(private val activity: Activity) : Plugin(activity) {
  private var webView: WebView? = null
  private var multicast: WifiManager.MulticastLock? = null
  private val main = Handler(Looper.getMainLooper())
  private val worker = Executors.newSingleThreadExecutor()
  private var pickDest: File? = null
  private var pickFolder = false
  private var askedNotifications = false
  private var pausedAt = 0L

  override fun load(webView: WebView) {
    this.webView = webView
    instance = this
    NativeAudio.attach(activity)
    holdMulticast()
    watchNetwork()
  }

  override fun onPause(activity: AppCompatActivity) {
    pausedAt = SystemClock.elapsedRealtime()
    Log.i(TAG, "app in the background (playing ${NowPlaying.snapshot.playing}, phone player song ${NativeAudio.current})")
    // WryActivity pauses the web view, and with it the audio element. A music app keeps playing.
    if (NowPlaying.snapshot.playing) keepRunning()
  }

  override fun onResume(activity: AppCompatActivity) {
    Log.i(TAG, "app in front again (playing ${NowPlaying.snapshot.playing}, phone player song ${NativeAudio.current})")
    // After a long time away (Doze, a night in the pocket) the relay link is likely stale.
    if (pausedAt != 0L && SystemClock.elapsedRealtime() - pausedAt > 60_000) networkChanged()
    pausedAt = 0L
  }

  override fun onDestroy(activity: AppCompatActivity) {
    instance = null
    multicast?.let { if (it.isHeld) it.release() }
    multicast = null
    worker.shutdown()
    if (activity.isFinishing) {
      // The app was closed: the music ends with it.
      Log.i(TAG, "app closed: stop the music")
      NativeAudio.release()
      PlaybackService.stop(activity)
    } else {
      // The system took the window away (e.g. to save memory) but the app goes on: the phone's
      // player keeps playing the queue and the notification stays.
      Log.i(TAG, "window closed by the system; the music keeps playing (song ${NativeAudio.current})")
    }
  }

  /** Resumes the web view right after the activity paused it, so the audio element keeps going. */
  fun keepRunning() {
    val view = webView ?: return
    main.post { view.onResume() }
    main.postDelayed({ if (NowPlaying.snapshot.playing) view.onResume() }, 150)
  }

  private fun holdMulticast() {
    // Without it many phones drop the UDP broadcast and multicast that Nearby discovery uses.
    val wifi = activity.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager ?: return
    multicast = wifi.createMulticastLock("tunepeerly-discovery").apply {
      setReferenceCounted(false)
      runCatching { acquire() }.onFailure { Log.w(TAG, "multicast lock: ${it.message}") }
    }
  }

  private fun connectivity() = activity.applicationContext.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager

  /**
   * Network changes go to Rust, which calls paired devices again at once (`Cmd::NetworkChanged`).
   * Registered once per process, so it keeps working while the window is gone and the music plays on.
   */
  private fun watchNetwork() {
    if (watching) return
    watching = true
    var first = true
    // What the last reports said. Signal strength alone changes the capabilities every few seconds;
    // only a change in how the phone is connected, or in its addresses, calls the links again.
    var lastCaps: String? = null
    var lastAddresses: String? = null
    val debounce = Runnable { networkChanged("changed") }
    val callback = object : ConnectivityManager.NetworkCallback() {
      override fun onAvailable(net: Network) {
        // A new network's first capabilities and addresses come right after; they are not a change.
        lastCaps = null
        lastAddresses = null
        // The first call only reports the network the app started on.
        if (first) { first = false; return }
        main.removeCallbacks(debounce)
        main.post {
          networkChanged("available")
          NativeAudio.networkBack()
        }
      }
      override fun onLost(net: Network) {
        main.removeCallbacks(debounce)
        main.post { networkChanged("lost") }
      }
      override fun onCapabilitiesChanged(net: Network, caps: NetworkCapabilities) {
        val key = listOf(
          NetworkCapabilities.TRANSPORT_WIFI, NetworkCapabilities.TRANSPORT_CELLULAR,
          NetworkCapabilities.TRANSPORT_ETHERNET, NetworkCapabilities.TRANSPORT_VPN,
        ).joinToString("") { if (caps.hasTransport(it)) "1" else "0" } +
          (if (caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)) "v" else "-")
        val before = lastCaps
        lastCaps = key
        if (first || before == null || before == key) return
        main.removeCallbacks(debounce)
        main.postDelayed(debounce, 1500)
      }
      override fun onLinkPropertiesChanged(net: Network, props: LinkProperties) {
        val key = props.linkAddresses.map { it.address.hostAddress.orEmpty() }.sorted().joinToString(",")
        val before = lastAddresses
        lastAddresses = key
        if (first || before == null || before == key) return
        main.removeCallbacks(debounce)
        main.postDelayed(debounce, 1500)
      }
    }
    runCatching { connectivity().registerDefaultNetworkCallback(callback) }
      .onFailure {
        watching = false
        Log.w(TAG, "network callback: ${it.message}")
      }
  }

  /** Straight to Rust when it asked for these (`networkWatch`), so a held-back page does not delay it. */
  private fun networkChanged(why: String = "resumed") {
    Log.i(TAG, "network $why: links are called again")
    val channel = rustNetwork
    if (channel != null && runCatching { channel.send(JSObject().put("reason", why)) }.isSuccess) return
    (instance ?: this).invokeRust("network_command", JSONObject().put("action", "network-changed").put("payload", JSONObject.NULL))
  }

  private fun invokeRust(command: String, args: JSONObject) {
    val script = "window.__TAURI_INTERNALS__&&window.__TAURI_INTERNALS__.invoke(${JSONObject.quote(command)},$args).catch(function(){})"
    main.post { webView?.evaluateJavascript(script, null) }
  }

  /** Asks the page to handle the back button. [done] gets false when there was nothing left to close. */
  fun back(done: (Boolean) -> Unit): Boolean {
    val view = webView ?: return false
    val script = "(function(){try{return !!(window.__tpBack&&window.__tpBack())}catch(e){return false}})()"
    view.evaluateJavascript(script) { done(it == "true") }
    return true
  }

  /** Runs a control press in the player. Returns false when the app is not running. */
  fun action(name: String): Boolean {
    if (webView == null) return false
    keepRunning()
    invokeRust("desktop_command", JSONObject().put("action", name))
    return true
  }

  /** Sends what the phone's player is doing to the page (`src/player/native.ts`). */
  fun audioEvent(json: String) {
    val script = "window.__tpAudio&&window.__tpAudio($json)"
    main.post { webView?.evaluateJavascript(script, null) }
  }

  /** Rust asks to hear about network changes directly (see `android::watch_network`). */
  @Command
  fun networkWatch(invoke: Invoke) {
    val args = invoke.parseArgs(NetworkWatchArgs::class.java)
    rustNetwork = args.onEvent
    invoke.resolve(JSObject())
  }

  /** The name the user gave the phone in its settings, else its model; never "localhost". */
  @Command
  fun deviceName(invoke: Invoke) {
    val set = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N_MR1) {
      runCatching { Settings.Global.getString(activity.contentResolver, Settings.Global.DEVICE_NAME) }.getOrNull()
    } else null
    val name = set?.trim().orEmpty().ifEmpty { Build.MODEL?.trim().orEmpty() }
    invoke.resolve(JSObject().put("name", name))
  }

  /** A command from the page for the phone's player. Applied in order on the main thread. */
  @Command
  fun audio(invoke: Invoke) {
    val args = invoke.getArgs()
    val op = args.getString("op") ?: ""
    val body = JSONObject(args.toString())
    main.post { runCatching { NativeAudio.command(op, body) }.onFailure { Log.w(TAG, "audio $op: ${it.message}") } }
    invoke.resolve()
  }

  @Command
  fun seal(invoke: Invoke) {
    val args = invoke.parseArgs(BlobArgs::class.java)
    runCatching { KeyVault.seal(Base64.decode(args.data, Base64.NO_WRAP)) }
      .onSuccess { invoke.resolve(JSObject().put("data", Base64.encodeToString(it, Base64.NO_WRAP))) }
      .onFailure { invoke.reject("keystore: ${it.message}") }
  }

  @Command
  fun unseal(invoke: Invoke) {
    val args = invoke.parseArgs(BlobArgs::class.java)
    runCatching { KeyVault.unseal(Base64.decode(args.data, Base64.NO_WRAP)) }
      .onSuccess { invoke.resolve(JSObject().put("data", Base64.encodeToString(it, Base64.NO_WRAP))) }
      .onFailure { invoke.reject("keystore: ${it.message}") }
  }

  @Command
  fun pick(invoke: Invoke) {
    val args = invoke.parseArgs(PickArgs::class.java)
    pickDest = File(args.dest)
    pickFolder = args.folder
    val intent = if (args.folder) {
      Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
    } else {
      Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE)
        type = "audio/*"
        putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
      }
    }
    startActivityForResult(invoke, intent, "picked")
  }

  @ActivityCallback
  fun picked(invoke: Invoke, result: ActivityResult) {
    val dest = pickDest
    val data = result.data
    if (result.resultCode != Activity.RESULT_OK || data == null || dest == null) {
      invoke.resolve(JSObject().put("count", 0))
      return
    }
    val uris = mutableListOf<Uri>()
    data.clipData?.let { clip -> for (i in 0 until clip.itemCount) uris.add(clip.getItemAt(i).uri) }
    data.data?.let { if (uris.isEmpty()) uris.add(it) }
    val tree = pickFolder && uris.size == 1
    worker.execute {
      runCatching {
        val copier = Copier(activity.contentResolver)
        if (tree) copier.copyTree(uris[0], dest) else uris.forEach { copier.copyFile(it, dest) }
        copier.count
      }.onSuccess { invoke.resolve(JSObject().put("count", it)) }
        .onFailure { invoke.reject("import: ${it.message}") }
    }
  }

  @Command
  fun playback(invoke: Invoke) {
    val args = invoke.parseArgs(TrackArgs::class.java)
    val draft = Snapshot(
      title = args.title,
      artist = args.artist,
      album = args.album,
      coverPath = args.coverPath,
      playing = args.playing,
      favorite = args.favorite,
      duration = args.duration,
      position = args.position,
      language = args.language,
      likeLabel = args.tips?.like ?: "Like",
    )
    val dark = args.theme != "light"
    main.post {
      // While the phone's player has a song, it is the one that knows whether sound is coming and
      // where. The page briefly reports "not playing" between songs, which must not stop the service.
      // After songs changed in the background, the page may report the old song until it caught up.
      val next = if (NativeAudio.current != 0L) {
        val song = NativeAudio.currentSong
        val base = if (song != null && song.title != draft.title) {
          draft.copy(title = song.title, artist = song.artist, album = song.album, coverPath = song.coverPath, favorite = song.favorite)
        } else draft
        base.copy(playing = NativeAudio.wantsPlay, position = NativeAudio.position, duration = NativeAudio.duration.takeIf { it > 0 } ?: song?.duration?.takeIf { it > 0 } ?: draft.duration)
      } else draft
      (activity as? MainActivity)?.applyTheme(dark)
      NowPlaying.update(next)
      if (next.hasTrack) PlaybackService.ensure(activity)
      if (next.playing) askNotifications()
      NowPlayingWidget.refresh(activity.applicationContext)
    }
    invoke.resolve()
  }

  /** Android 13 and newer show the playback notification only after the user allows notifications. */
  private fun askNotifications() {
    if (askedNotifications || Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
    askedNotifications = true
    if (ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
      runCatching { activity.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 41) }
    }
  }

  companion object {
    private const val TAG = "TunePlugin"
    @Volatile var instance: TunePlugin? = null
    /** Rust's listener for network changes; kept for the whole process, like the callback that feeds it. */
    @Volatile private var rustNetwork: Channel? = null
    @Volatile private var watching = false
  }
}

/** Copies picked documents into the app's own music folder, which the importer then reads like any folder. */
private class Copier(private val resolver: android.content.ContentResolver) {
  var count = 0

  private val audio = setOf(
    "flac", "mp3", "wav", "ogg", "opus", "m4a", "aac", "aiff", "aif", "alac", "wma", "ape", "wv", "wave",
    "oga", "m4b", "mp4", "dsf", "dff", "caf", "tta", "spx", "lrc",
  )

  /** A file or folder name from the provider, which is not trusted: never a path, never `.` or `..`. */
  private fun safe(name: String): String {
    val cleaned = name.replace(Regex("[/\\\\:*?\"<>|\\u0000-\\u001f]"), "_").trim()
    return (if (cleaned == "." || cleaned == "..") "_" else cleaned).ifEmpty { "song" }.take(180)
  }

  private fun wanted(name: String, mime: String?) =
    (mime?.startsWith("audio/") == true) || name.substringAfterLast('.', "").lowercase() in audio

  fun copyFile(uri: Uri, dir: File) {
    var name = uri.lastPathSegment?.substringAfterLast('/') ?: "song"
    resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
      if (cursor.moveToFirst()) cursor.getString(0)?.let { name = it }
    }
    copy(uri, File(dir, safe(name)))
  }

  private fun copy(uri: Uri, target: File) {
    target.parentFile?.mkdirs()
    val partial = File(target.parentFile, ".${target.name}.partial")
    resolver.openInputStream(uri)?.use { input -> partial.outputStream().use { input.copyTo(it, 256 * 1024) } }
      ?: throw IllegalStateException("cannot read $uri")
    if (!partial.renameTo(target)) {
      target.delete()
      if (!partial.renameTo(target)) throw IllegalStateException("cannot save ${target.name}")
    }
    if (!target.name.endsWith(".lrc", true)) count++
  }

  fun copyTree(tree: Uri, dest: File) {
    val root = DocumentsContract.getTreeDocumentId(tree)
    var rootName = "Folder"
    resolver.query(DocumentsContract.buildDocumentUriUsingTree(tree, root), arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME), null, null, null)?.use {
      if (it.moveToFirst()) it.getString(0)?.let { name -> rootName = name }
    }
    walk(tree, root, File(dest, safe(rootName)), 0)
  }

  private fun walk(tree: Uri, parent: String, dir: File, depth: Int) {
    if (depth > 12) return
    val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, parent)
    val columns = arrayOf(
      DocumentsContract.Document.COLUMN_DOCUMENT_ID,
      DocumentsContract.Document.COLUMN_DISPLAY_NAME,
      DocumentsContract.Document.COLUMN_MIME_TYPE,
    )
    resolver.query(children, columns, null, null, null)?.use { cursor ->
      while (cursor.moveToNext()) {
        val id = cursor.getString(0) ?: continue
        val name = cursor.getString(1) ?: continue
        val mime = cursor.getString(2)
        if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
          walk(tree, id, File(dir, safe(name)), depth + 1)
        } else if (wanted(name, mime)) {
          copy(DocumentsContract.buildDocumentUriUsingTree(tree, id), File(dir, safe(name)))
        }
      }
    }
  }
}
