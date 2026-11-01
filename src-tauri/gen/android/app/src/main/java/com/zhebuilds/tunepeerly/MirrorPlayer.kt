// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import android.os.Looper
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import androidx.media3.common.util.UnstableApi
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture

/**
 * A Media3 player that plays nothing itself. It shows what the web view's audio element is doing and
 * turns every press (notification, lock screen, quick settings, headset, Bluetooth) into a player
 * command, so the queue and every rule about what plays next stay in one place.
 */
@OptIn(UnstableApi::class)
class MirrorPlayer(looper: Looper) : SimpleBasePlayer(looper) {
  private val commands = Player.Commands.Builder()
    .addAll(
      Player.COMMAND_PLAY_PAUSE,
      Player.COMMAND_SEEK_TO_NEXT,
      Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM,
      Player.COMMAND_SEEK_TO_PREVIOUS,
      Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM,
      Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM,
      Player.COMMAND_GET_CURRENT_MEDIA_ITEM,
      Player.COMMAND_GET_METADATA,
      Player.COMMAND_GET_TIMELINE,
      Player.COMMAND_STOP,
    )
    .build()

  fun refresh() = invalidateState()

  override fun getState(): State {
    val now = NowPlaying.snapshot
    val state = State.Builder().setAvailableCommands(commands)
    if (!now.hasTrack) return state.setPlaybackState(Player.STATE_IDLE).build()
    val metadata = MediaMetadata.Builder()
      .setTitle(now.title)
      .setDisplayTitle(now.title)
      .setArtist(now.artist)
      .setAlbumTitle(now.album.ifEmpty { null })
      .setIsPlayable(true)
      .setIsBrowsable(false)
      .apply { NowPlaying.artBytes(now.coverPath)?.let { setArtworkData(it, MediaMetadata.PICTURE_TYPE_FRONT_COVER) } }
      .build()
    val uid = "${now.title}\u0000${now.artist}\u0000${now.album}\u0000${now.coverPath}"
    val item = MediaItemData.Builder(uid)
      .setMediaItem(MediaItem.Builder().setMediaId(uid).setMediaMetadata(metadata).build())
      .setMediaMetadata(metadata)
      .setDurationUs(if (now.duration > 0) (now.duration * 1_000_000).toLong() else C.TIME_UNSET)
      .setIsSeekable(now.duration > 0)
      .build()
    return state
      .setPlaylist(listOf(item))
      .setCurrentMediaItemIndex(0)
      // One item that repeats, so "next" and "previous" always count as available.
      .setRepeatMode(Player.REPEAT_MODE_ALL)
      .setPlaybackState(Player.STATE_READY)
      .setPlayWhenReady(now.playing, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
      .setContentPositionMs(
        PositionSupplier.getExtrapolating((now.position * 1000).toLong(), if (now.playing) 1f else 0f)
      )
      .build()
  }

  private fun send(action: String): ListenableFuture<*> {
    Controls.press(action)
    return Futures.immediateVoidFuture()
  }

  override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
    NowPlaying.assume { it.copy(playing = playWhenReady, position = it.positionNow()) }
    return send(if (playWhenReady) "play" else "pause")
  }

  override fun handleStop(): ListenableFuture<*> = handleSetPlayWhenReady(false)

  override fun handleSeek(mediaItemIndex: Int, positionMs: Long, seekCommand: Int): ListenableFuture<*> =
    when (seekCommand) {
      Player.COMMAND_SEEK_TO_NEXT, Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM -> send("next")
      Player.COMMAND_SEEK_TO_PREVIOUS, Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM -> send("previous")
      else -> {
        val seconds = positionMs.coerceAtLeast(0) / 1000.0
        NowPlaying.assume { it.copy(position = seconds) }
        send("seek:$seconds")
      }
    }
}
