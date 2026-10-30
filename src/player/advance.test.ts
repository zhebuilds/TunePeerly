// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import assert from "node:assert/strict";
import test from "node:test";
import {
  acceptGeneration,
  addToQueue,
  fisherYates,
  jumpTo,
  moveInQueue,
  continueLibrary,
  followListOrder,
  next,
  nextOrWrap,
  onEnded,
  playNext,
  startShuffled,
  toggleShuffle,
  previous,
  queuedIndex,
  removeAt,
  selectSource,
  shouldIncrementGeneration,
  type QueueState,
  type TrackRef,
} from "./advance.ts";

function track(key: string): TrackRef {
  return { key, contentId: key, localFileId: null };
}

function state(partial: Partial<QueueState> = {}): QueueState {
  return {
    order: "manual",
    repeat: "off",
    contextId: "library",
    past: [],
    current: track("a"),
    future: [track("b"), track("c")],
    positionSec: 0,
    ...partial,
  };
}

test("manual order and independent repeat", () => {
  const ended = onEnded(state(), [track("a"), track("b")], () => 0);
  assert.equal(ended.increment, true);
  assert.equal(ended.state.current?.key, "b");
  assert.equal(ended.state.future[0]?.key, "c");
});

test("shuffle queue is a permutation", () => {
  const items = ["a", "b", "c", "d"];
  let cursor = 0;
  const values = [0.9, 0.1, 0.4];
  const shuffled = fisherYates(items, () => values[cursor++] ?? 0);
  assert.deepEqual(shuffled.slice().sort(), items.slice().sort());
});

test("previous after three seconds restarts", () => {
  const result = previous(state({ past: [track("z")], positionSec: 4 }), 4);
  assert.equal(result.current?.key, "a");
  assert.equal(result.positionSec, 0);
  assert.equal(result.past.length, 1);
});

test("previous within three seconds uses history", () => {
  const result = previous(state({ past: [track("z")] }), 2);
  assert.equal(result.current?.key, "z");
  assert.equal(result.future[0]?.key, "a");
});

test("next at the end wraps to the first song of the current list", () => {
  const playlist = [track("a"), track("b"), track("c")];
  const result = nextOrWrap(state({ current: track("c"), future: [], past: [track("a"), track("b")] }), playlist, () => 0);
  assert.equal(result.current?.key, "a");
  assert.deepEqual(result.future.map((item) => item.key), ["b", "c"]);
  assert.equal(result.positionSec, 0);
});

test("previous at the start wraps to the last song", () => {
  const playlist = [track("a"), track("b"), track("c")];
  const result = previous(state({ current: track("a"), future: [track("b"), track("c")], past: [] }), 1, playlist);
  assert.equal(result.current?.key, "c");
  assert.deepEqual(result.future.map((item) => item.key), ["a", "b"]);
});

test("an empty queue continues through the library instead of stopping", () => {
  const library = [track("a"), track("b"), track("c")];
  const finished = state({ order: "manual", repeat: "off", current: track("b"), future: [] });
  const result = continueLibrary(finished, library, () => 0);
  assert.equal(result.contextId, "library");
  assert.equal(result.current?.key, "c");
  assert.deepEqual(result.future.map((item) => item.key), ["a"]);
});

test("the last library song wraps to the first when the queue is empty", () => {
  const library = [track("a"), track("b"), track("c")];
  const result = continueLibrary(state({ current: track("c"), future: [] }), library, () => 0);
  assert.equal(result.current?.key, "a");
  assert.deepEqual(result.future.map((item) => item.key), ["b"]);
});

test("next returns to the song just left", () => {
  const left = previous(state({ past: [track("z")] }), 1);
  const back = next(left);
  assert.equal(back.current?.key, "a");
});

test("play next and add to queue", () => {
  assert.equal(playNext(state(), track("x")).future[0]?.key, "x");
  assert.equal(addToQueue(state(), track("y")).future.at(-1)?.key, "y");
});

test("removing a duplicate keeps the other copy", () => {
  const queued = addToQueue(state(), [track("b"), track("x")]);
  const result = removeAt(queued, 0);
  assert.deepEqual(result.future.map((item) => item.key), ["c", "b", "x"]);
});

test("moving a queued entry", () => {
  const result = moveInQueue(state({ future: [track("b"), track("c"), track("d")] }), 0, 2);
  assert.deepEqual(result.future.map((item) => item.key), ["c", "d", "b"]);
});

test("jumping into the queue moves skipped entries to past", () => {
  const result = jumpTo(state({ future: [track("b"), track("c"), track("d")] }), 1);
  assert.equal(result.current?.key, "c");
  assert.deepEqual(result.past.map((item) => item.key), ["a", "b"]);
  assert.deepEqual(result.future.map((item) => item.key), ["d"]);
});

test("repeat list rebuilds without keeping the first song in both places", () => {
  const result = onEnded(state({ repeat: "list", future: [] }), [track("a"), track("b"), track("c")], () => 0);
  assert.equal(result.state.current?.key, "a");
  assert.deepEqual(result.state.future.map((item) => item.key), ["b", "c"]);
});

test("shuffle keeps the current song and mixes every other song in the context", () => {
  const context = [track("a"), track("b"), track("c"), track("d")];
  const playing = state({ current: track("c"), future: [track("d")], past: [track("b")] });
  const next = toggleShuffle(playing, context, () => 0);
  assert.equal(next.order, "listShuffle");
  assert.equal(next.current?.key, "c");
  assert.deepEqual(next.future.map((item) => item.key).sort(), ["a", "b", "d"]);
});

test("shuffled playback keeps every track in the list", () => {
  const next = startShuffled([track("a"), track("b"), track("c"), track("d")], () => 0, "playlist:1", "listShuffle");
  assert.equal(next.order, "listShuffle");
  assert.deepEqual([next.current?.key, ...next.future.map((item) => item.key)].sort(), ["a", "b", "c", "d"]);
});

test("list repeat restarts from the songs already played when the context is gone", () => {
  const result = onEnded(state({ repeat: "list", future: [], past: [track("b"), track("c")], current: track("a") }), [], () => 0);
  assert.equal(result.increment, true);
  assert.equal(result.state.current?.key, "b");
  assert.deepEqual(result.state.future.map((item) => item.key), ["c", "a"]);
});

test("repeat one does not rebuild the list", () => {
  const result = onEnded(state({ repeat: "one" }), [track("nope")], () => 0);
  assert.equal(result.state.current?.key, "a");
  assert.equal(result.state.future.length, 2);
  assert.equal(result.increment, true);
});

test("stale generation events are dropped", () => {
  assert.equal(acceptGeneration(3, 3), true);
  assert.equal(acceptGeneration(4, 3), false);
});

test("transport failure tries the next device and does not skip the song", () => {
  const sources = [
    { deviceId: "a", pathId: "1", remote: true },
    { deviceId: "a", pathId: "2", remote: true },
    { deviceId: "b", pathId: "3", remote: true },
  ];
  assert.equal(selectSource(sources, "transport", sources[0], false)?.pathId, "2");
  assert.equal(selectSource(sources, "transport", sources[1], false)?.deviceId, "b");
});

test("decode or unsupported does not change device", () => {
  const sources = [
    { deviceId: "local", pathId: "file", remote: false },
    { deviceId: "peer", pathId: "copy", remote: true },
  ];
  assert.equal(selectSource(sources, "decode", sources[0], false), null);
  assert.equal(selectSource(sources, "unsupported", sources[0], false), null);
});

test("list order keeps the current song and queues whatever now follows it", () => {
  const playing = state({ repeat: "list", current: track("b"), future: [track("c"), track("d")], past: [track("a")] });
  const next = followListOrder(playing, [track("c"), track("a"), track("b"), track("d")]);
  assert.equal(next.current?.key, "b");
  assert.deepEqual(next.future.map((item) => item.key), ["d"]);
  assert.equal(followListOrder(playing, [track("z")]), playing);
});

test("a lan and relay path switch does not increment generation", () => {
  assert.equal(shouldIncrementGeneration("path"), false);
  assert.equal(shouldIncrementGeneration("device"), true);
  assert.equal(shouldIncrementGeneration("repeat"), true);
});

test("a stale queue action resolves to the song it was started on", () => {
  const future = [track("b"), track("c"), track("d")];
  assert.equal(queuedIndex(future, 1, "c"), 1);
  assert.equal(queuedIndex(future, 0, "c"), 1);
  assert.equal(queuedIndex(future, 2, "b"), 0);
  assert.equal(queuedIndex(future, 0, "gone"), -1);
  assert.equal(queuedIndex([], 0, "b"), -1);
});
