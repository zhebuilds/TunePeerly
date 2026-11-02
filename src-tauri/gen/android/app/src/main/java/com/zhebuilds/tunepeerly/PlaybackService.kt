// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.wifi.WifiManager
import android.os.Bundle
import android.os.Looper
import android.os.PowerManager
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.CommandButton
import androidx.media3.session.DefaultMediaNotificationProvider
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionResult
import com.google.common.collect.ImmutableList
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture

/**
 * Keeps the app alive while music plays with the screen off, and owns the media session that the
 * notification, the lock screen, the quick settings media card, OEM status-bar pills and Bluetooth
 * devices read. Media3 moves it in and out of the foreground as playback starts and stops.
 */
@OptIn(UnstableApi::class)
class PlaybackService : MediaSessionService() {
  private var session: MediaSession? = null
  private var player: MirrorPlayer? = null
  private var wake: PowerManager.WakeLock? = null
  private var wifi: WifiManager.WifiLock? = null
  private val handler = android.os.Handler(Looper.getMainLooper())
  private val listener: (Snapshot, Snapshot) -> Unit = { before, now -> changed(before, now) }

  override fun onCreate() {
    super.onCreate()
    Log.i(TAG, "service starts")
    running = true
    val mirror = MirrorPlayer(Looper.getMainLooper())
    player = mirror
    val open = PendingIntent.getActivity(
      this, 0,
      Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
    session = MediaSession.Builder(this, mirror)
      .setSessionActivity(open)
      .setCallback(Callback())
      .setMediaButtonPreferences(likeButtons(NowPlaying.snapshot))
      .build()
    // Stays in the foreground for a while after a pause, like the locks below, so the app is not
    // frozen or killed while the queue waits.
    setForegroundServiceTimeoutMs(HOLD_AFTER_PAUSE_MS)
    setMediaNotificationProvider(
      DefaultMediaNotificationProvider.Builder(this).build().apply { setSmallIcon(R.drawable.ic_stat_note) }
    )
    // Nothing binds to this service (the page talks to NativeAudio, not to a MediaController), so
    // the session is added here. Without this Media3 never shows the notification and never moves
    // the service to the foreground, and Android stops it a minute after the app leaves the screen.
    addSession(session!!)
    NowPlaying.listeners.add(listener)
    locks(NowPlaying.snapshot.playing)
  }

  private fun changed(before: Snapshot, now: Snapshot) {
    player?.refresh()
    if (before.favorite != now.favorite || before.likeLabel != now.likeLabel || before.hasTrack != now.hasTrack) {
      session?.setMediaButtonPreferences(likeButtons(now))
    }
    if (before.playing != now.playing || now.playing) locks(now.playing)
  }

  private fun likeButtons(now: Snapshot): ImmutableList<CommandButton> {
    if (!now.hasTrack) return ImmutableList.of()
    val like = CommandButton.Builder(if (now.favorite) CommandButton.ICON_HEART_FILLED else CommandButton.ICON_HEART_UNFILLED)
      .setDisplayName(now.likeLabel)
      .setSessionCommand(SessionCommand(LIKE, Bundle.EMPTY))
      .setSlots(CommandButton.SLOT_FORWARD_SECONDARY, CommandButton.SLOT_OVERFLOW)
      .build()
    return ImmutableList.of(like)
  }

  /**
   * Songs stream from linked devices, so the CPU and the Wi-Fi radio stay up while the music plays,
   * while a song waits for its device to come back, and for 10 minutes after a pause, so the links
   * (kept by Rust, see `Redial` in net.rs) are still up when play is pressed on the lock screen.
   */
  private val release = Runnable { locks(false, now = true) }

  private fun locks(playing: Boolean, now: Boolean = false) {
    if (!playing && !now) {
      // A pause or the gap between two songs: keep the CPU and Wi-Fi up for a while, so the queue
      // goes on at once and the links stay up with the screen off.
      if (wake?.isHeld == true) {
        handler.removeCallbacks(release)
        handler.postDelayed(release, HOLD_AFTER_PAUSE_MS)
      }
      return
    }
    handler.removeCallbacks(release)
    if (playing) {
      if (wake?.isHeld != true) Log.i(TAG, "CPU and Wi-Fi kept awake for playback")
      if (wake == null) {
        wake = (getSystemService(Context.POWER_SERVICE) as PowerManager)
          .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "tunepeerly:playback").apply { setReferenceCounted(false) }
      }
      if (wifi == null) {
        @Suppress("DEPRECATION")
        wifi = (applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager)
          .createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "tunepeerly:playback").apply { setReferenceCounted(false) }
      }
      // A safety limit; every report while playing takes it again.
      wake?.acquire(30 * 60 * 1000L)
      runCatching { wifi?.acquire() }
    } else {
      if (wake?.isHeld == true) Log.i(TAG, "CPU and Wi-Fi locks let go (not playing)")
      wake?.let { if (it.isHeld) it.release() }
      wifi?.let { if (it.isHeld) it.release() }
    }
  }

  override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session

  override fun onTaskRemoved(rootIntent: Intent?) {
    Log.i(TAG, "app swiped away: stop the music")
    // Swiping the app away closes the page that runs the queue, so the music and the notification stop too.
    NativeAudio.stop()
    stopSelf()
  }

  override fun onDestroy() {
    Log.i(TAG, "service ends")
    running = false
    NowPlaying.listeners.remove(listener)
    locks(false, now = true)
    session?.release()
    session = null
    player?.release()
    player = null
    super.onDestroy()
  }

  private inner class Callback : MediaSession.Callback {
    override fun onConnect(session: MediaSession, controller: MediaSession.ControllerInfo): MediaSession.ConnectionResult {
      val commands = MediaSession.ConnectionResult.DEFAULT_SESSION_COMMANDS.buildUpon()
        .add(SessionCommand(LIKE, Bundle.EMPTY))
        .build()
      return MediaSession.ConnectionResult.AcceptedResultBuilder(session)
        .setAvailableSessionCommands(commands)
        .build()
    }

    override fun onCustomCommand(
      session: MediaSession,
      controller: MediaSession.ControllerInfo,
      customCommand: SessionCommand,
      args: Bundle,
    ): ListenableFuture<SessionResult> {
      if (customCommand.customAction == LIKE) {
        NowPlaying.assume { it.copy(favorite = !it.favorite) }
        Controls.press("like")
        return Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
      }
      return super.onCustomCommand(session, controller, customCommand, args)
    }
  }

  companion object {
    private const val TAG = "PlaybackService"
    const val LIKE = "com.zhebuilds.tunepeerly.LIKE"
    /** How long the service and the locks stay after the music paused. */
    const val HOLD_AFTER_PAUSE_MS = 10 * 60_000L
    @Volatile private var running = false

    /** Started while the app is on screen, before the first song plays, so Media3 may promote it later. */
    fun ensure(context: Context) {
      if (running) return
      runCatching { context.startService(Intent(context, PlaybackService::class.java)) }
        .onSuccess { refused = false }
        .onFailure {
          // Android refuses while the app is in the background; it is asked again on every update,
          // so this is said once until a start works.
          if (!refused) Log.w(TAG, "start: ${it.message}")
          refused = true
        }
    }

    @Volatile private var refused = false

    fun stop(context: Context) {
      runCatching { context.stopService(Intent(context, PlaybackService::class.java)) }
    }
  }
}
