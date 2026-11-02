package com.zhebuilds.tunepeerly

import android.os.Bundle
import android.view.View
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  private var dark: Boolean? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Android 15 and newer always draw the app under the status and navigation bars. Pad the page
    // by the bars (and the keyboard) so nothing sits behind them.
    val root = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(root) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val keyboard = insets.getInsets(WindowInsetsCompat.Type.ime())
      view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, keyboard.bottom))
      WindowInsetsCompat.CONSUMED
    }
    applyTheme(true)
    // Back closes what is open in the page, then goes back a page; at the start it sends the app to
    // the background instead of closing it, so the music keeps playing.
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        val asked = TunePlugin.instance?.back { handled -> if (!handled) moveTaskToBack(true) } ?: false
        if (!asked) moveTaskToBack(true)
      }
    })
  }

  /** Colours the strips behind the status and navigation bars like the page. */
  fun applyTheme(dark: Boolean) {
    if (this.dark == dark) return
    this.dark = dark
    val colour = if (dark) 0xFF0F0C0A.toInt() else 0xFFECE4D7.toInt()
    window.decorView.setBackgroundColor(colour)
    findViewById<View>(android.R.id.content)?.setBackgroundColor(colour)
    WindowCompat.getInsetsController(window, window.decorView).apply {
      isAppearanceLightStatusBars = !dark
      isAppearanceLightNavigationBars = !dark
    }
  }
}
