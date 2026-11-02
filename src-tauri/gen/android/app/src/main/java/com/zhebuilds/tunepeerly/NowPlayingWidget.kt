// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.SystemClock
import android.view.View
import android.widget.RemoteViews

/**
 * The home-screen player: cover, title and artist, previous, play or pause, next, like, and a progress
 * bar that only shows. Redrawn when the song, the play state or the like changes, and every few
 * seconds of progress while it plays.
 */
class NowPlayingWidget : AppWidgetProvider() {
  override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
    render(context, manager, ids)
  }

  override fun onReceive(context: Context, intent: Intent) {
    val action = when (intent.action) {
      ACTION_PREVIOUS -> "previous"
      ACTION_TOGGLE -> "toggle"
      ACTION_NEXT -> "next"
      ACTION_LIKE -> "like"
      else -> null
    }
    if (action == null) {
      super.onReceive(context, intent)
      return
    }
    when (action) {
      "toggle" -> NowPlaying.assume { it.copy(playing = !it.playing && it.hasTrack, position = it.positionNow()) }
      "like" -> if (NowPlaying.snapshot.hasTrack) NowPlaying.assume { it.copy(favorite = !it.favorite) }
    }
    if (!Controls.press(action)) Controls.openApp(context)
    refresh(context, force = true)
  }

  companion object {
    private const val ACTION_PREVIOUS = "com.zhebuilds.tunepeerly.widget.PREVIOUS"
    private const val ACTION_TOGGLE = "com.zhebuilds.tunepeerly.widget.TOGGLE"
    private const val ACTION_NEXT = "com.zhebuilds.tunepeerly.widget.NEXT"
    private const val ACTION_LIKE = "com.zhebuilds.tunepeerly.widget.LIKE"
    private var drawn: Snapshot? = null
    private var drawnAt = 0L

    /** Redraws every placed widget when something it shows changed. */
    fun refresh(context: Context, force: Boolean = false) {
      val now = NowPlaying.snapshot
      val last = drawn
      val same = last != null && last.title == now.title && last.artist == now.artist && last.coverPath == now.coverPath &&
        last.playing == now.playing && last.favorite == now.favorite && last.duration == now.duration
      if (!force && same && (!now.playing || SystemClock.elapsedRealtime() - drawnAt < 5000)) return
      val manager = AppWidgetManager.getInstance(context)
      val ids = manager.getAppWidgetIds(ComponentName(context, NowPlayingWidget::class.java))
      if (ids.isEmpty()) return
      render(context, manager, ids)
    }

    private fun intent(context: Context, action: String, code: Int): PendingIntent =
      PendingIntent.getBroadcast(
        context, code,
        Intent(context, NowPlayingWidget::class.java).setAction(action),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
      )

    private fun render(context: Context, manager: AppWidgetManager, ids: IntArray) {
      val now = NowPlaying.snapshot
      drawn = now
      drawnAt = SystemClock.elapsedRealtime()
      val zh = now.language.startsWith("zh")
      val views = RemoteViews(context.packageName, R.layout.widget_now_playing)
      views.setTextViewText(R.id.widget_title, if (now.hasTrack) now.title.ifEmpty { "—" } else context.getString(R.string.app_name))
      views.setTextViewText(R.id.widget_artist, if (now.hasTrack) now.artist else if (zh) "未在播放" else "Not playing")
      val art = NowPlaying.artBitmap(now.coverPath)
      if (art != null) views.setImageViewBitmap(R.id.widget_cover, art) else views.setImageViewResource(R.id.widget_cover, R.drawable.widget_cover_empty)
      views.setImageViewResource(R.id.widget_toggle, if (now.playing) R.drawable.ic_pause else R.drawable.ic_play)
      views.setImageViewResource(R.id.widget_like, if (now.favorite) R.drawable.ic_heart_filled else R.drawable.ic_heart)
      views.setContentDescription(R.id.widget_toggle, if (now.playing) (if (zh) "暂停" else "Pause") else (if (zh) "播放" else "Play"))
      views.setContentDescription(R.id.widget_previous, if (zh) "上一首" else "Previous")
      views.setContentDescription(R.id.widget_next, if (zh) "下一首" else "Next")
      views.setContentDescription(R.id.widget_like, now.likeLabel)
      val progress = if (now.duration > 0) ((now.positionNow() / now.duration) * 1000).toInt().coerceIn(0, 1000) else 0
      views.setProgressBar(R.id.widget_progress, 1000, progress, false)
      views.setViewVisibility(R.id.widget_progress, if (now.hasTrack) View.VISIBLE else View.INVISIBLE)
      views.setOnClickPendingIntent(R.id.widget_previous, intent(context, ACTION_PREVIOUS, 1))
      views.setOnClickPendingIntent(R.id.widget_toggle, intent(context, ACTION_TOGGLE, 2))
      views.setOnClickPendingIntent(R.id.widget_next, intent(context, ACTION_NEXT, 3))
      views.setOnClickPendingIntent(R.id.widget_like, intent(context, ACTION_LIKE, 4))
      val open = PendingIntent.getActivity(
        context, 5,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
      )
      views.setOnClickPendingIntent(R.id.widget_cover, open)
      views.setOnClickPendingIntent(R.id.widget_text, open)
      manager.updateAppWidget(ids, views)
    }
  }
}
