// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

/**
 * The play queue as the phone's player keeps it while the page's script is held back. The rules are
 * the same as `src/player/advance.ts` (`onEnded`, `continueLibrary`, `nextOrWrap`, `previous`), so a
 * song that ends with the screen off is followed by the same song the page would pick.
 *
 * Songs are keys (content id, or the local file id before hashing). `playable` says whether the phone's
 * player can start a key; others are passed over. No Android classes, so it is tested on the JVM.
 */
class NativeQueue(
  private val playable: (String) -> Boolean,
  private val random: java.util.Random = java.util.Random(),
) {
  var past = ArrayList<String>()
  var current: String? = null
  var future = ArrayList<String>()
  /** "off", "list" or "one". */
  var repeat = "off"
  /** "manual", "listShuffle" or "libraryRandom". */
  var order = "manual"
  var contextId = "library"
  /** The list the queue came from, to start over with (repeat all, or next at the end). */
  var pool: List<String> = emptyList()
  /** The whole library, to keep playing from when the queue runs out. */
  var library: List<String> = emptyList()

  private fun shuffled(items: List<String>): ArrayList<String> {
    val copy = ArrayList(items)
    for (index in copy.size - 1 downTo 1) {
      val swap = random.nextInt(index + 1)
      val value = copy[index]
      copy[index] = copy[swap]
      copy[swap] = value
    }
    return copy
  }

  private fun playedOrder(): List<String> {
    val seen = LinkedHashSet<String>()
    past.forEach { seen.add(it) }
    current?.let { seen.add(it) }
    return seen.toList()
  }

  /** The current song moves to `past`; `list` becomes the rest of the queue, its first entry now. */
  private fun moveTo(list: List<String>) {
    current?.let { past.add(it) }
    while (past.size > PAST_LIMIT) past.removeAt(0)
    current = list.firstOrNull()
    future = ArrayList(list.drop(1))
  }

  /** The song ended (not repeat one): what follows. False when nothing does and the music stops. */
  fun afterEnd(): Boolean {
    if (future.isNotEmpty()) {
      moveTo(ArrayList(future))
      return passOver()
    }
    if (repeat == "list") {
      val source = pool.filter(playable).ifEmpty { playedOrder() }
      if (source.isEmpty()) return false
      moveTo(if (order == "manual") source else shuffled(source))
      return passOver()
    }
    // Nothing queued: keep playing from the whole library.
    val all = library.filter(playable)
    if (all.isEmpty()) return false
    val cur = current
    val others = all.filter { it != cur }
    val list = if (order == "manual") {
      val index = all.indexOf(cur)
      val after = if (index >= 0) all.drop(index + 1) else others
      val before = if (index >= 0) all.take(index).filter { it != cur } else emptyList()
      (after + before).ifEmpty { all }
    } else shuffled(others.ifEmpty { all })
    contextId = if (order == "libraryRandom") "random" else "library"
    pool = library
    moveTo(list)
    return passOver()
  }

  /** Next on the lock screen (`nextOrWrap`). False when it only starts the same song over. */
  fun next(): Boolean {
    if (future.isNotEmpty()) {
      moveTo(ArrayList(future))
      return passOver()
    }
    val list = pool.filter(playable)
    if (list.isEmpty()) return false
    val first = list[0]
    val rest = list.drop(1)
    val cur = current
    if (cur != null && cur != first) past.add(cur)
    current = first
    future = if (order == "manual") ArrayList(rest) else shuffled(rest)
    return passOver()
  }

  /** Previous on the lock screen (`previous`). False when it only starts the same song over. */
  fun previous(positionSec: Double): Boolean {
    if (positionSec > 3) return false
    val cur = current
    if (past.isNotEmpty()) {
      val prior = past.removeAt(past.size - 1)
      cur?.let { future.add(0, it) }
      current = prior
      return true
    }
    val last = pool.lastOrNull(playable)
    if (last != null && last != cur) {
      past = ArrayList()
      current = last
      future = ArrayList(pool.filter { it != last })
      return true
    }
    return false
  }

  /** Moves past songs the phone cannot start. False when none is left. */
  private fun passOver(): Boolean {
    var guard = 0
    while (current != null && !playable(current!!) && guard++ < 10_000) {
      past.add(current!!)
      current = if (future.isNotEmpty()) future.removeAt(0) else null
    }
    return current != null
  }

  companion object {
    const val PAST_LIMIT = 200
  }
}
