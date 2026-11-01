// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import android.content.Context
import android.content.Intent

/** Where every control press outside the app window goes. */
object Controls {
  /** Returns false when the app's web view is not running, so the caller can open the app instead. */
  fun press(action: String): Boolean {
    // Next, previous, play and pause go to the phone's player first, which needs no page script, so
    // they work with the screen locked. The page hears about it from the player's reports.
    if (NativeAudio.press(action)) return true
    return TunePlugin.instance?.action(action) ?: false
  }

  fun openApp(context: Context) {
    val intent = Intent(context, MainActivity::class.java)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
    context.startActivity(intent)
  }
}
