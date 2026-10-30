// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import android.content.Context
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Bundle
import android.provider.DocumentsContract
import android.util.Log
import android.widget.Toast
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.Keep
import java.io.File
import java.util.concurrent.Executors

class MainActivity : TauriActivity() {
  private var multicastLock: WifiManager.MulticastLock? = null
  private val importer = Executors.newSingleThreadExecutor()

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    holdMulticastLock()
  }

  override fun onDestroy() {
    importer.shutdownNow()
    multicastLock?.let { if (it.isHeld) it.release() }
    multicastLock = null
    super.onDestroy()
  }

  /**
   * Android only hands multicast packets to apps that hold a MulticastLock, so nearby-device
   * discovery (UDP multicast on the LAN) stays silent without it. Held for the activity lifetime.
   */
  private fun holdMulticastLock() {
    try {
      val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
      if (wifi == null) {
        Log.w(TAG, "no WifiManager, discovery may not work")
        return
      }
      multicastLock = wifi.createMulticastLock("tunepeerly-discovery").also {
        it.setReferenceCounted(false)
        it.acquire()
      }
      Log.i(TAG, "multicast lock held")
    } catch (err: Exception) {
      Log.w(TAG, "multicast lock failed: ${err.message}")
    }
  }

  /**
   * Called from Rust over JNI, so release builds must keep this exact name. Android hands out
   * `content://` URIs that the Rust side cannot read, so the chosen tree is copied into the private
   * music folder and scanned from there once the copy is done.
   */
  @Keep
  @Suppress("unused")
  fun openMusicFolderPicker() {
    runOnUiThread {
      try {
        picker.launch(null)
      } catch (err: Exception) {
        Log.w(TAG, "no folder picker: ${err.message}")
        toast(getString(R.string.import_no_picker))
      }
    }
  }

  private val picker = registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { tree ->
    if (tree == null) {
      // Rust waits for this marker, so a cancel has to be reported too, or the UI sits out its
      // whole timeout with nothing on screen.
      writeMarker(CANCELLED)
    } else {
      startImport(tree)
    }
  }

  private fun startImport(tree: Uri) {
    // Must be `dataDir`, not `filesDir`: Rust scans `app.path().data_dir()/music`, which on Android
    // is `Context.getDataDir()`. `filesDir` puts the songs one level deeper and the scan then always
    // finds an empty folder.
    val music = File(applicationContext.dataDir, "music")
    deleteMarker()
    importer.execute {
      val copied = try {
        copyTree(tree, music)
      } catch (err: Exception) {
        Log.w(TAG, "import failed: ${err.message}")
        -1
      }
      if (copied < 0) {
        runOnUiThread { toast(getString(R.string.import_failed)) }
      } else if (copied == 0) {
        runOnUiThread { toast(getString(R.string.import_nothing_new)) }
      }
      writeMarker(if (copied < 0) FAILED else copied)
    }
  }

  /**
   * Walks the picked document tree and copies every audio file, keeping folder names. Files that
   * are already there with the same size are skipped, so picking the same folder twice is cheap.
   *
   * Each folder has to carry its own document id: `getTreeDocumentId` always returns the id of the
   * picked root, so building child URIs from it lists the root again, never descends, and keeps
   * copying the same files into deeper and deeper folders.
   */
  private fun copyTree(tree: Uri, root: File): Int {
    val resolver = contentResolver
    var copied = 0
    val visited = HashSet<String>()
    val pending = ArrayDeque<Triple<Uri, File, String>>()
    pending.addLast(Triple(tree, root, DocumentsContract.getTreeDocumentId(tree)))
    while (pending.isNotEmpty()) {
      val (treeUri, dirFile, documentId) = pending.removeLast()
      if (!visited.add(documentId)) continue
      val children = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, documentId)
      resolver.query(
        children,
        arrayOf(
          DocumentsContract.Document.COLUMN_DOCUMENT_ID,
          DocumentsContract.Document.COLUMN_DISPLAY_NAME,
          DocumentsContract.Document.COLUMN_MIME_TYPE,
          DocumentsContract.Document.COLUMN_SIZE,
        ),
        null,
        null,
        null,
      )?.use { cursor ->
        while (cursor.moveToNext()) {
          val id = cursor.getString(0) ?: continue
          val name = safeName(cursor.getString(1) ?: continue)
          val mime = cursor.getString(2)
          val size = if (cursor.isNull(3)) -1L else cursor.getLong(3)
          val childUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, id)
          if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
            pending.addLast(Triple(treeUri, File(dirFile, name), id))
            continue
          }
          if (!isAudio(name)) continue
          val target = File(dirFile, name)
          if (size >= 0 && target.isFile && target.length() == size) continue
          target.parentFile?.mkdirs()
          // Copy under a temporary name first, so an interrupted copy is never taken for a track.
          val partial = File(target.parentFile, "$name.part")
          val stream = resolver.openInputStream(childUri) ?: continue
          stream.use { input -> partial.outputStream().use { output -> input.copyTo(output) } }
          if (!partial.renameTo(target)) {
            partial.delete()
            continue
          }
          copied += 1
        }
      }
    }
    return copied
  }

  /** Writes the marker through a temporary file, so Rust never reads a half-written number. */
  private fun writeMarker(value: Int) {
    try {
      val music = File(applicationContext.dataDir, "music")
      music.mkdirs()
      val marker = File(music, MARKER)
      val partial = File(music, "$MARKER.part")
      partial.writeText(value.toString())
      if (!partial.renameTo(marker)) {
        marker.delete()
        partial.renameTo(marker)
      }
    } catch (err: Exception) {
      Log.w(TAG, "could not write the import marker: ${err.message}")
    }
  }

  private fun deleteMarker() {
    val music = File(applicationContext.dataDir, "music")
    File(music, MARKER).delete()
    File(music, "$MARKER.part").delete()
  }

  private fun isAudio(name: String): Boolean =
    name.substringAfterLast('.', "").lowercase() in AUDIO_EXTENSIONS

  private fun safeName(name: String): String =
    name.replace('/', '_').replace('\u0000', '_').ifEmpty { "unnamed" }

  private fun toast(text: String) {
    Toast.makeText(this, text, Toast.LENGTH_LONG).show()
  }

  private companion object {
    const val TAG = "TunePeerly"
    const val MARKER = ".import-done"
    const val FAILED = -1
    const val CANCELLED = -2
    val AUDIO_EXTENSIONS = setOf(
      "mp3", "flac", "m4a", "m4b", "aac", "ogg", "oga", "opus", "wav", "wave",
      "wma", "aiff", "aif", "alac", "ape", "wv", "mp4", "caf", "dsf", "dff", "tta", "spx",
    )
  }
}
