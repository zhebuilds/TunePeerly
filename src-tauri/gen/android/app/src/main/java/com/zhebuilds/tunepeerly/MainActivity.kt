// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

/**
 * The whole app is the Tauri web view, so the activity only has to hand the window to Tauri and
 * let the system draw edge to edge. Anything Android-specific the Rust side needs later
 * (multicast, the folder picker) is added here as it is built.
 */
class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }
}
