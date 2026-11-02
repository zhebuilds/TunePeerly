// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import assert from "node:assert/strict";
import test from "node:test";
import { followedFuture, nativeQueueable } from "./nativePlan.ts";

const song = (over: Partial<{ playable: boolean; video: boolean; badge: string; contentId: string | null; reasonKey: string | null }>) => ({
  playable: true,
  video: false,
  badge: "local",
  contentId: "c1",
  ...over,
});

test("a linked device's song stays queueable while the device is offline", () => {
  assert.equal(nativeQueueable(song({ badge: "remote", playable: false })), true);
  assert.equal(nativeQueueable(song({ badge: "remote", playable: true })), true);
  // How the library lists a paired device's song while that device is offline.
  assert.equal(nativeQueueable(song({ badge: "unavailable", playable: false, reasonKey: "notice.offlineTrack" })), true);
});

test("songs that can never stream are left out", () => {
  assert.equal(nativeQueueable(song({ badge: "remote", playable: false, contentId: null })), false);
  assert.equal(nativeQueueable(song({ badge: "local", playable: false })), false);
  assert.equal(nativeQueueable(song({ badge: "unavailable", playable: false, reasonKey: "file.missing" })), false);
  assert.equal(nativeQueueable(song({ badge: "unavailable", playable: false, reasonKey: "notice.offlineTrack", contentId: null })), false);
  assert.equal(nativeQueueable(song({ video: true })), false);
  assert.equal(nativeQueueable(song({ badge: "remote", video: true })), false);
});

test("an end-of-queue report never empties the page's upcoming songs", () => {
  assert.deepEqual(followedFuture(["b", "c"], [], true), ["b", "c"]);
});

test("other reports are taken over as they are", () => {
  assert.deepEqual(followedFuture(["b", "c"], ["c"], false), ["c"]);
  assert.deepEqual(followedFuture(["b", "c"], [], false), []);
  assert.deepEqual(followedFuture([], [], true), []);
  assert.deepEqual(followedFuture(["b"], ["x"], true), ["x"]);
});
