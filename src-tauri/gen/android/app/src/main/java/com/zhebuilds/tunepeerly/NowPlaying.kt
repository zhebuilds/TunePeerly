// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import java.io.File

/** What the player in the web view shows, as last reported. The service, the session and the widget read it. */
data class Snapshot(
  val title: String = "",
  val artist: String = "",
  val album: String = "",
  val coverPath: String? = null,
  val playing: Boolean = false,
  val favorite: Boolean = false,
  val duration: Double = 0.0,
  val position: Double = 0.0,
  val language: String = "en",
  val likeLabel: String = "Like",
  val at: Long = System.currentTimeMillis(),
) {
  val hasTrack get() = title.isNotEmpty() || artist.isNotEmpty()

  /** Where playback is now, counting the time since the report while it plays. */
  fun positionNow(): Double {
    if (!playing) return position
    val moved = position + (System.currentTimeMillis() - at) / 1000.0
    return if (duration > 0) moved.coerceAtMost(duration) else moved
  }
}

object NowPlaying {
  @Volatile var snapshot = Snapshot()
    private set

  private var artPath: String? = null
  private var artBytes: ByteArray? = null
  private var artBitmap: Bitmap? = null

  /** Called on the main thread after every change. */
  val listeners = mutableListOf<(Snapshot, Snapshot) -> Unit>()

  fun update(next: Snapshot) {
    val previous = snapshot
    snapshot = next
    listeners.toList().forEach { it(previous, next) }
  }

  /** The controls change the state at once; the web view's report confirms or corrects it a moment later. */
  fun assume(change: (Snapshot) -> Snapshot) = update(change(snapshot).copy(at = System.currentTimeMillis()))

  @Synchronized
  private fun loadArt(path: String?) {
    if (path == artPath) return
    artPath = path
    artBytes = null
    artBitmap = null
    if (path.isNullOrEmpty()) return
    val file = File(path)
    if (!file.isFile || file.length() > 8L * 1024 * 1024) return
    val bytes = runCatching { file.readBytes() }.getOrNull() ?: return
    artBytes = bytes
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    var sample = 1
    while (bounds.outWidth / (sample * 2) >= 320 && bounds.outHeight / (sample * 2) >= 320) sample *= 2
    artBitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })
  }

  @Synchronized
  fun artBytes(path: String?): ByteArray? {
    loadArt(path)
    return artBytes
  }

  @Synchronized
  fun artBitmap(path: String?): Bitmap? {
    loadArt(path)
    return artBitmap
  }
}
