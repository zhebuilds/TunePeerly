// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The phone's queue must pick the same songs as src/player/advance.ts. */
class NativeQueueTest {
  private fun queue(vararg missing: String) = NativeQueue({ it !in missing }, java.util.Random(7))

  private fun NativeQueue.plays(count: Int): List<String> {
    val out = ArrayList<String>()
    repeat(count) {
      if (repeat == "one") out.add(current!!)
      else if (afterEnd()) out.add(current!!) else return out
    }
    return out
  }

  @Test fun playsTheQueueInOrderThenStopsWithoutLibrary() {
    val q = queue()
    q.current = "a"; q.future = arrayListOf("b", "c")
    assertEquals(listOf("b", "c"), q.plays(5))
    assertEquals(listOf("a", "b"), q.past)
  }

  @Test fun repeatAllStartsTheListOverInOrder() {
    val q = queue()
    q.repeat = "list"; q.pool = listOf("a", "b", "c")
    q.current = "a"; q.future = arrayListOf("b", "c")
    assertEquals(listOf("b", "c", "a", "b", "c", "a", "b"), q.plays(7))
  }

  @Test fun repeatAllShuffledPlaysEverySongEachRound() {
    val q = queue()
    q.repeat = "list"; q.order = "listShuffle"; q.pool = listOf("a", "b", "c", "d")
    q.current = "a"; q.future = arrayListOf("c", "b", "d")
    val heard = q.plays(3 + 8)
    assertEquals(listOf("c", "b", "d"), heard.take(3))
    assertEquals(setOf("a", "b", "c", "d"), heard.subList(3, 7).toSet())
    assertEquals(setOf("a", "b", "c", "d"), heard.subList(7, 11).toSet())
  }

  @Test fun repeatOneKeepsTheSong() {
    val q = queue()
    q.repeat = "one"; q.current = "a"; q.future = arrayListOf("b")
    assertEquals(listOf("a", "a", "a"), q.plays(3))
  }

  @Test fun continuesWithTheLibraryAfterTheCurrentSong() {
    val q = queue()
    q.library = listOf("a", "b", "c", "d", "e"); q.contextId = "album:x"
    q.current = "c"; q.future = arrayListOf()
    assertEquals(listOf("d", "e", "a", "b"), q.plays(4))
    assertEquals("library", q.contextId)
  }

  @Test fun continuesWithTheLibraryShuffledInRandomMode() {
    val q = queue()
    q.order = "libraryRandom"; q.library = listOf("a", "b", "c", "d")
    q.current = "a"
    assertTrue(q.afterEnd())
    assertEquals("random", q.contextId)
    assertTrue(q.current != "a")
    assertEquals(setOf("b", "c", "d"), (listOf(q.current!!) + q.future).toSet())
  }

  @Test fun passesOverSongsItCannotPlay() {
    val q = queue("b")
    q.current = "a"; q.future = arrayListOf("b", "c")
    assertTrue(q.afterEnd())
    assertEquals("c", q.current)
  }

  @Test fun nextWrapsToTheStartOfTheList() {
    val q = queue()
    q.pool = listOf("a", "b", "c"); q.current = "c"; q.future = arrayListOf()
    assertTrue(q.next())
    assertEquals("a", q.current)
    assertEquals(listOf("b", "c"), q.future)
  }

  @Test fun previousGoesBackOrRestarts() {
    val q = queue()
    q.pool = listOf("a", "b", "c"); q.past = arrayListOf("a"); q.current = "b"; q.future = arrayListOf("c")
    assertFalse(q.previous(5.0))
    assertTrue(q.previous(1.0))
    assertEquals("a", q.current)
    assertEquals(listOf("b", "c"), q.future)
    assertTrue(q.previous(0.0))
    assertEquals("c", q.current)
  }

  @Test fun stopsWhenNothingFollows() {
    val q = queue()
    q.current = "a"
    assertFalse(q.afterEnd())
    q.current = "a"; q.future = arrayListOf("x")
    val missing = queue("x")
    missing.current = "a"; missing.future = arrayListOf("x")
    assertFalse(missing.afterEnd())
    assertNull(missing.current)
  }
}
