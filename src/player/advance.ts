// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
export type OrderMode = "manual" | "listShuffle" | "libraryRandom";
export type RepeatMode = "off" | "list" | "one";

export type TrackRef = {
  key: string;
  contentId: string | null;
  localFileId: string | null;
};

export type QueueState = {
  order: OrderMode;
  repeat: RepeatMode;
  contextId: string;
  past: TrackRef[];
  current: TrackRef | null;
  future: TrackRef[];
  positionSec: number;
};

export type FailureClass = "transport" | "decode" | "unsupported";

export type PlaySource = {
  deviceId: string;
  pathId: string;
  remote: boolean;
};

export function fisherYates<T>(items: T[], random: () => number): T[] {
  const copy = items.slice();
  for (let index = copy.length - 1; index > 0; index -= 1) {
    const swap = Math.floor(random() * (index + 1));
    const current = copy[index];
    copy[index] = copy[swap];
    copy[swap] = current;
  }
  return copy;
}

export function sameTrack(left: TrackRef | null, right: TrackRef | null): boolean {
  if (!left || !right) return false;
  return left.key === right.key;
}

export function playFromList(
  state: QueueState,
  tracks: TrackRef[],
  index: number,
  shuffle: boolean,
  random: () => number,
): QueueState {
  const current = tracks[index] ?? null;
  const after = tracks.slice(index + 1);
  const future = shuffle || state.order === "listShuffle" ? fisherYates(after, random) : after;
  return {
    ...state,
    order: shuffle ? "listShuffle" : state.order === "libraryRandom" ? "manual" : state.order,
    past: state.current ? [...state.past, state.current] : state.past,
    current,
    future,
    positionSec: 0,
  };
}

export function startShuffled(
  tracks: TrackRef[],
  random: () => number,
  contextId: string,
  order: "listShuffle" | "libraryRandom",
): QueueState {
  const shuffled = fisherYates(tracks, random);
  return {
    order,
    repeat: "off",
    contextId,
    past: [],
    current: shuffled[0] ?? null,
    future: shuffled.slice(1),
    positionSec: 0,
  };
}

export function startLibraryRandom(tracks: TrackRef[], random: () => number, contextId: string): QueueState {
  return startShuffled(tracks, random, contextId, "libraryRandom");
}

export function onEnded(
  state: QueueState,
  rebuild: TrackRef[],
  random: () => number,
): { state: QueueState; increment: boolean } {
  if (state.repeat === "one" && state.current) {
    return { state: { ...state, positionSec: 0 }, increment: true };
  }
  if (state.future.length > 0) {
    const [current, ...future] = state.future;
    return {
      increment: true,
      state: {
        ...state,
        past: state.current ? [...state.past, state.current] : state.past,
        current,
        future,
        positionSec: 0,
      },
    };
  }
  if (state.repeat === "list") {
    const source = rebuild.length > 0 ? rebuild : playedOrder(state);
    if (source.length === 0) return { state, increment: false };
    const pool = state.order === "manual" ? source : fisherYates(source, random);
    const [current, ...future] = pool;
    return {
      increment: Boolean(current),
      state: {
        ...state,
        past: state.current ? [...state.past, state.current] : state.past,
        current: current ?? null,
        future,
        positionSec: 0,
      },
    };
  }
  return { state, increment: false };
}

/** The song finished and nothing is queued. Keep playing from the whole library. */
export function continueLibrary(state: QueueState, library: TrackRef[], random: () => number): QueueState {
  const others = library.filter((track) => !sameTrack(track, state.current));
  if (library.length === 0) return { ...state, positionSec: 0 };
  const past = state.current ? [...state.past, state.current] : state.past;
  if (state.order === "manual") {
    const index = library.findIndex((track) => sameTrack(track, state.current));
    const after = index >= 0 ? library.slice(index + 1) : others;
    const before = index >= 0 ? library.slice(0, index).filter((track) => !sameTrack(track, state.current)) : [];
    const sequence = [...after, ...before];
    const list = sequence.length > 0 ? sequence : library;
    const [current, ...future] = list;
    return { ...state, contextId: "library", past, current: current ?? null, future, positionSec: 0 };
  }
  const source = others.length > 0 ? others : library;
  const [current, ...future] = fisherYates(source, random);
  return {
    ...state,
    contextId: state.order === "libraryRandom" ? "random" : "library",
    past,
    current: current ?? null,
    future,
    positionSec: 0,
  };
}

export function previous(state: QueueState, positionSec: number, playlist: TrackRef[] = []): QueueState {
  if (positionSec > 3) {
    return { ...state, positionSec: 0 };
  }
  const prior = state.past[state.past.length - 1];
  if (prior) {
    return {
      ...state,
      past: state.past.slice(0, -1),
      current: prior,
      future: state.current ? [state.current, ...state.future] : state.future,
      positionSec: 0,
    };
  }
  const last = playlist[playlist.length - 1];
  if (last && !sameTrack(last, state.current)) {
    return {
      ...state,
      past: [],
      current: last,
      future: playlist.filter((track) => !sameTrack(track, last)),
      positionSec: 0,
    };
  }
  return { ...state, positionSec: 0 };
}

export function next(state: QueueState): QueueState {
  if (state.future.length === 0) return state;
  const [current, ...future] = state.future;
  return {
    ...state,
    past: state.current ? [...state.past, state.current] : state.past,
    current,
    future,
    positionSec: 0,
  };
}

/** Next plays the queue. With nothing left, it restarts at the first song of the current list. */
export function nextOrWrap(state: QueueState, playlist: TrackRef[], random: () => number): QueueState {
  if (state.future.length > 0) return next(state);
  if (playlist.length === 0) return state.current ? { ...state, positionSec: 0 } : state;
  const [current, ...rest] = playlist;
  const future = state.order === "manual" ? rest : fisherYates(rest, random);
  const past = state.current && !sameTrack(state.current, current) ? [...state.past, state.current] : state.past;
  return { ...state, past, current, future, positionSec: 0 };
}

export function playNext(state: QueueState, tracks: TrackRef | TrackRef[]): QueueState {
  const items = Array.isArray(tracks) ? tracks : [tracks];
  return { ...state, future: [...items, ...state.future] };
}

export function addToQueue(state: QueueState, tracks: TrackRef | TrackRef[]): QueueState {
  const items = Array.isArray(tracks) ? tracks : [tracks];
  return { ...state, future: [...state.future, ...items] };
}

/** Removes one queue entry by position, so other copies of the same song stay. */
export function removeAt(state: QueueState, index: number): QueueState {
  if (index < 0 || index >= state.future.length) return state;
  return { ...state, future: state.future.filter((_, at) => at !== index) };
}

export function moveInQueue(state: QueueState, from: number, to: number): QueueState {
  if (from === to || from < 0 || from >= state.future.length) return state;
  const future = state.future.slice();
  const [item] = future.splice(from, 1);
  future.splice(Math.max(0, Math.min(to, future.length)), 0, item);
  return { ...state, future };
}

/**
 * Finds the entry a queue action was started on. The queue can move under an
 * open menu or a drag (the current song ends), so trust the position only when
 * it still holds the same song; otherwise fall back to the entry's new spot.
 * Returns -1 when the song is no longer queued.
 */
export function queuedIndex(future: TrackRef[], at: number, key: string): number {
  if (future[at]?.key === key) return at;
  return future.findIndex((ref) => ref.key === key);
}

/** Jumps to a queued entry; everything before it moves to `past`. */
export function jumpTo(state: QueueState, index: number): QueueState {
  if (index < 0 || index >= state.future.length) return state;
  const skipped = state.future.slice(0, index);
  return {
    ...state,
    past: [...state.past, ...(state.current ? [state.current] : []), ...skipped],
    current: state.future[index],
    future: state.future.slice(index + 1),
    positionSec: 0,
  };
}

function playedOrder(state: QueueState): TrackRef[] {
  const seen = new Set<string>();
  const items: TrackRef[] = [];
  for (const item of [...state.past, ...(state.current ? [state.current] : [])]) {
    if (seen.has(item.key)) continue;
    seen.add(item.key);
    items.push(item);
  }
  return items;
}

/** Keeps the current song and lines up the songs after it in the list. */
export function followListOrder(state: QueueState, playlist: TrackRef[]): QueueState {
  if (!state.current || state.order !== "manual") return state;
  const index = playlist.findIndex((track) => sameTrack(track, state.current));
  if (index < 0) return state;
  return { ...state, future: playlist.slice(index + 1) };
}

export function toggleShuffle(state: QueueState, context: TrackRef[], random: () => number): QueueState {
  if (state.order === "manual") {
    const rest = state.current ? context.filter((track) => !sameTrack(track, state.current)) : context;
    const shuffled = fisherYates(rest, random);
    if (!state.current) {
      const [current, ...future] = shuffled;
      return { ...state, order: "listShuffle", past: [], current: current ?? null, future, positionSec: 0 };
    }
    return { ...state, order: "listShuffle", future: shuffled };
  }
  const index = context.findIndex((track) => sameTrack(track, state.current));
  const future = index >= 0 ? context.slice(index + 1) : context;
  return { ...state, order: "manual", future };
}

export function cycleRepeat(mode: RepeatMode): RepeatMode {
  if (mode === "off") return "list";
  if (mode === "list") return "one";
  return "off";
}

export function acceptGeneration(current: number, eventGeneration: number): boolean {
  return current === eventGeneration;
}

export function shouldIncrementGeneration(reason: "track" | "device" | "repeat" | "path"): boolean {
  return reason !== "path";
}

export function selectSource(
  sources: PlaySource[],
  failure: FailureClass,
  failed: PlaySource | null,
  incognito: boolean,
): PlaySource | null {
  if (!failed) return sources[0] ?? null;
  if (failure === "decode" || failure === "unsupported") return null;
  if (incognito && failed.remote) return null;
  const start = sources.findIndex(
    (source) => source.deviceId === failed.deviceId && source.pathId === failed.pathId,
  );
  const rest = sources.slice(start >= 0 ? start + 1 : 0);
  const sameDevice = rest.find((source) => source.deviceId === failed.deviceId);
  if (sameDevice) return sameDevice;
  if (incognito) return null;
  return rest.find((source) => source.deviceId !== failed.deviceId) ?? null;
}
