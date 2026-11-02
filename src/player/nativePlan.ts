// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
// Rules for what the page hands the phone's player (NativeAudio.kt) and takes back from it. No imports
// besides types, so `npm test` runs them directly.

/** The parts of a track these rules read. */
export type QueueableTrack = { playable: boolean; video: boolean; badge: string; contentId: string | null; reasonKey?: string | null };

/**
 * Whether the phone's player may hold this song in its catalog and queue. A linked device's song stays
 * even while that device is offline for a moment: the player waits for the link instead of dropping the
 * song (and with it the rest of the queue). While its device is offline such a song is listed as
 * "unavailable" with the offline notice, so that case counts too. Videos are left to the page.
 */
export function nativeQueueable(track: QueueableTrack): boolean {
  if (track.video) return false;
  if (track.playable) return true;
  if (!track.contentId) return false;
  return track.badge === "remote" || (track.badge === "unavailable" && track.reasonKey === "notice.offlineTrack");
}

/**
 * The upcoming songs the page shows after a report from the phone's player. At the end of the queue
 * ("nothing left") the report may come with no upcoming songs although the page still has some, e.g.
 * songs whose device was offline. Those are kept, so the Queue tab never empties by itself.
 */
export function followedFuture<T>(pageFuture: T[], reported: T[], ended: boolean): T[] {
  if (ended && reported.length === 0 && pageFuture.length > 0) return pageFuture;
  return reported;
}
