// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { listen } from "@tauri-apps/api/event";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { api, errorText, isAndroidApp, pickFiles, pickFolder, type Bootstrap, type ContentMove, type DownloadFeed, type MemberRef, type OverwriteAsk, type Settings, type Track } from "../api";
import i18n from "../i18n";
import { albumParts, filterText, refOf, reorderVisible, shufflable, trackKey, TrackIndex } from "../lib/library";
import {
  addToQueue,
  continueLibrary,
  followListOrder,
  cycleRepeat,
  jumpTo,
  moveInQueue,
  nextOrWrap,
  onEnded,
  playFromList,
  playNext,
  previous as previousState,
  removeAt,
  startLibraryRandom,
  startShuffled,
  toggleShuffle,
  type QueueState,
  type TrackRef,
} from "../player/advance";
import {
  ACK_TIMEOUT_MS,
  CONNECT_VERSION,
  HEARTBEAT_PAUSED_MS,
  HEARTBEAT_PLAYING_MS,
  MAX_PAST,
  MAX_REFS,
  MAX_STATE_QUEUE,
  acceptsInbound,
  addOverlay,
  commandId,
  contextIdFrom,
  contextKindOf,
  extrapolate,
  forgetDevice,
  isContentId,
  isFresh,
  isPortableContext,
  nextClaim,
  pickAction,
  pickActive,
  receiveState,
  safeName,
  settleOverlay,
  throttle,
  viewOf,
  windowAround,
  type ConnectAck,
  type ConnectCommand,
  type ConnectMeta,
  type ConnectState,
  type ConnectTransfer,
  type Overlay,
  type RemoteEntry,
  type RemoteView,
} from "../player/connect";
import { clock, transport } from "../player/clock";
import { PlaybackEngine, type EngineEvents } from "../player/engine";

export type RouteName =
  | "home"
  | "search"
  | "library"
  | "liked"
  | "recent"
  | "artists"
  | "albums"
  | "folders"
  | "playlist"
  | "artist"
  | "album"
  | "folder"
  | "device"
  | "devices"
  | "settings"
  | "problems"
  | "now";
export type Route = { name: RouteName; id?: string };

export type MenuState = { x: number; y: number; tracks: Track[]; playlistId?: string; queueIndex?: number; playlistMenu?: string; play?: () => void } | null;

export type DialogState =
  | { kind: "confirm"; title: string; body: string; action: string; danger?: boolean; run: () => void | Promise<void> }
  | { kind: "prompt"; title: string; label: string; value: string; action: string; run: (value: string) => void | Promise<void> }
  | null;

export type Toast = { id: number; text: string };

const emptyQueue: QueueState = { order: "manual", repeat: "off", contextId: "library", past: [], current: null, future: [], positionSec: 0 };

/** A song another device reported, shown with what this computer knows about it. */
function metaTrack(meta: ConnectMeta, deviceId: string, favoriteIds: ReadonlySet<string>): Track {
  return {
    contentId: meta.contentId,
    localFileId: null,
    title: meta.title,
    artist: meta.artist,
    album: meta.album,
    trackNumber: null,
    durationMs: meta.durationMs,
    folder: "",
    fileName: "",
    addedAt: "",
    favorite: favoriteIds.has(meta.contentId),
    exposed: false,
    badge: "remote",
    accessStatus: "",
    metadataStatus: "",
    playbackStatus: "",
    reasonKey: null,
    video: false,
    coverUrl: null,
    absolutePath: null,
    holderIds: [deviceId],
    playable: false,
    dimmed: false,
  };
}

function metaOf(track: Track | null | undefined): ConnectMeta | null {
  if (!track?.contentId) return null;
  return { contentId: track.contentId, title: track.title, artist: track.artist, album: track.album, durationMs: track.durationMs ?? null };
}

function idRef(contentId: string): TrackRef {
  return { key: contentId, contentId, localFileId: null };
}

/** This computer's claim on being the active device, kept across restarts. */
function savedClaim(): number {
  try {
    const value = Number(window.localStorage.getItem("connectClaim"));
    return Number.isSafeInteger(value) && value > 0 ? value : 0;
  } catch {
    return 0;
  }
}

function saveClaim(value: number) {
  try {
    window.localStorage.setItem("connectClaim", String(value));
  } catch {
    /* storage unavailable: the claim lasts until restart */
  }
}

/** A row of the device picker. `detached`: the device plays on its own, outside device control. */
export type ConnectDevice = { deviceId: string; name: string; self: boolean; active: boolean; detached: boolean };
export type RemotePlayback = RemoteView & { name: string };

const PAST_LIMIT = 200;

function asRef(value: unknown): TrackRef | null {
  if (!value || typeof value !== "object") return null;
  const item = value as Record<string, unknown>;
  const contentId = typeof item.contentId === "string" ? item.contentId : null;
  const localFileId = typeof item.localFileId === "string" ? item.localFileId : null;
  const key = typeof item.key === "string" ? item.key : contentId ?? localFileId;
  return key ? { key, contentId, localFileId } : null;
}

function refs(value: unknown): TrackRef[] {
  return Array.isArray(value) ? value.map(asRef).filter((item): item is TrackRef => item !== null) : [];
}

function normalizeQueue(raw: Record<string, unknown> | undefined): QueueState {
  if (!raw) return emptyQueue;
  const order = raw.order === "listShuffle" || raw.order === "libraryRandom" ? raw.order : "manual";
  const repeat = raw.repeat === "list" || raw.repeat === "one" ? raw.repeat : "off";
  return {
    order,
    repeat,
    contextId: typeof raw.contextId === "string" ? raw.contextId : "library",
    past: refs(raw.past).slice(-PAST_LIMIT),
    current: asRef(raw.current),
    future: refs(raw.future),
    positionSec: Number(raw.positionSec) > 0 ? Number(raw.positionSec) : 0,
  };
}

function sameRoute(left: Route, right: Route): boolean {
  return left.name === right.name && (left.id ?? "") === (right.id ?? "");
}

function useModel() {
  const { t } = useTranslation();
  const [boot, setBoot] = useState<Bootstrap | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [members, setMembers] = useState<Record<string, MemberRef[]>>({});
  const [queue, setQueueState] = useState<QueueState>(emptyQueue);
  const [route, setRoute] = useState<Route>({ name: "home" });
  const [navigation, setNavigation] = useState({ canBack: false, canForward: false });
  // The queue starts open so it is easy to discover; closing it is remembered. Narrow windows start closed
  // because the panel overlays the page there.
  const [queueOpen, setQueueState_] = useState(() => {
    try {
      if (window.localStorage.getItem("queueOpen") === "0") return false;
    } catch {
      /* storage unavailable: use the default */
    }
    return window.innerWidth > 1040;
  });
  const [noticesOpen, setNoticesOpenState] = useState(false);
  const setQueueOpen = useCallback((open: boolean) => {
    setQueueState_(open);
    if (open) setNoticesOpenState(false);
    try {
      window.localStorage.setItem("queueOpen", open ? "1" : "0");
    } catch {
      /* ignore */
    }
  }, []);
  const setNoticesOpen = useCallback((open: boolean) => {
    setNoticesOpenState(open);
    if (open) {
      setQueueState_(false);
      try {
        window.localStorage.setItem("queueOpen", "0");
      } catch {
        /* ignore */
      }
    }
  }, []);
  const [menu, setMenu] = useState<MenuState>(null);
  const [dialog, setDialog] = useState<DialogState>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [alerts, setAlerts] = useState<{ id: number; text: string }[]>([]);
  const [moves, setMoves] = useState<ContentMove[]>([]);
  const [overwriteAsks, setOverwriteAsks] = useState<OverwriteAsk[]>([]);
  const [volume, setVolumeState] = useState(1);
  const [muted, setMuted] = useState(false);
  const [lyrics, setLyrics] = useState<{ key: string; text: string } | null>(null);
  const [failure, setFailure] = useState<{ key: string; reason: string } | null>(null);
  const [remotes, setRemotes] = useState<Record<string, RemoteEntry>>({});
  const [activeId, setActiveId] = useState("");
  const [overlay, setOverlay] = useState<Overlay | null>(null);
  /** This computer left device control to play on its own. Not saved: every start joins again. */
  const [detached, setDetached] = useState(false);
  /** Other devices that said they play on their own, until they report again. */
  const [detachedPeers, setDetachedPeers] = useState<ReadonlySet<string>>(() => new Set());
  const [downloadFeed, setDownloadFeed] = useState<DownloadFeed | null>(null);
  const [search, setSearch] = useState("");

  const queueRef = useRef(queue);
  const volumeRef = useRef(1);
  const mutedRef = useRef(false);
  const remotesRef = useRef(remotes);
  const activeRef = useRef("");
  const overlayRef = useRef(overlay);
  overlayRef.current = overlay;
  const detachedRef = useRef(false);
  /** Rejoined with music playing here: it becomes the active one once the others' claims are known. */
  const claimOnJoinRef = useRef<number | undefined>(undefined);
  const detachedPeersRef = useRef<ReadonlySet<string>>(detachedPeers);
  const claimRef = useRef(savedClaim());
  /** The highest claim seen from any device, kept when a device that left is forgotten. */
  const claimFloorRef = useRef(0);
  /** Ids of the latest commands this computer carried out for other devices. */
  const doneRef = useRef<string[]>([]);
  const pendingRef = useRef(new Map<string, { timer: number; transfer: boolean }>());
  const connectIn = useRef<((from: string, kind: string, body: unknown) => void) | null>(null);
  const publishTimer = useRef<number | undefined>(undefined);
  const lastPublish = useRef({ at: 0, signature: "" });
  const bootRef = useRef(boot);
  bootRef.current = boot;
  const membersRef = useRef(members);
  membersRef.current = members;
  const restored = useRef(false);
  const context = useRef<{ id: string; tracks: Track[] | null }>({ id: "library", tracks: null });
  const nav = useRef<{ stack: Route[]; index: number }>({ stack: [{ name: "home" }], index: 0 });
  const toastId = useRef(0);
  const alertId = useRef(0);
  const volumeTimer = useRef<number | undefined>(undefined);
  const refreshTimer = useRef<number | undefined>(undefined);
  const refreshRequest = useRef(0);
  const membersRequest = useRef(0);
  const lyricsRequest = useRef({ id: 0, key: null as string | null });
  const scanning = useRef(false);
  const [lyricsEpoch, setLyricsEpoch] = useState(0);
  /** Lyrics are kept per song. After a scan the song on screen may have a new `.lrc`, so ask again. */
  const reloadLyrics = useCallback(() => {
    lyricsRequest.current = { id: lyricsRequest.current.id + 1, key: null };
    setLyricsEpoch((value) => value + 1);
  }, []);

  const tracks = useMemo(() => boot?.tracks ?? [], [boot]);
  const index = useMemo(() => new TrackIndex(tracks), [tracks]);
  const indexRef = useRef(index);
  indexRef.current = index;
  const settings: Settings = boot?.settings ?? {};
  const showUnavailable = settings.showUnavailable !== false;

  const library = useMemo(() => (showUnavailable ? tracks : tracks.filter((track) => track.badge !== "unavailable")), [tracks, showUnavailable]);
  const liked = useMemo(() => tracks.filter((track) => track.favorite), [tracks]);
  const recent = useMemo(() => {
    const seen = new Set<string>();
    const out: Track[] = [];
    for (const key of boot?.history ?? []) {
      const track = index.get({ key });
      if (track && !seen.has(trackKey(track))) {
        seen.add(trackKey(track));
        out.push(track);
      }
    }
    return out;
  }, [boot?.history, index]);
  const current = index.get(queue.current);

  const handlers = useRef<EngineEvents | null>(null);
  const engineRef = useRef<PlaybackEngine | null>(null);
  if (!engineRef.current) {
    engineRef.current = new PlaybackEngine({
      ended: () => handlers.current?.ended(),
      failed: (track, info) => handlers.current?.failed(track, info),
      heard: (track) => handlers.current?.heard(track),
      paused: (track, position) => handlers.current?.paused(track, position),
      seeked: (track, position, paused) => handlers.current?.seeked(track, position, paused),
      progress: (position) => handlers.current?.progress(position),
      activity: (playing, track, enough) => handlers.current?.activity(playing, track, enough),
      playable: (track) => handlers.current?.playable(track),
    });
  }
  const engine = engineRef.current;

  const toast = useCallback((text: string) => {
    const id = ++toastId.current;
    setToasts((items) => [...items.slice(-2), { id, text }]);
    window.setTimeout(() => setToasts((items) => items.filter((item) => item.id !== id)), 2600);
  }, []);

  const setNotice = useCallback((text: string | null) => {
    if (!text) {
      setAlerts([]);
      return;
    }
    const id = ++alertId.current;
    // The same notice again moves to the end instead of stacking up; only the latest few stay.
    setAlerts((items) => [...items.filter((item) => item.text !== text), { id, text }].slice(-5));
  }, []);
  const dismissAlert = useCallback((id: number) => {
    setAlerts((items) => items.filter((item) => item.id !== id));
  }, []);

  const say = useCallback((error: unknown) => {
    const text = errorText(error);
    setNotice(i18n.exists(text) ? t(text) : text);
  }, [setNotice, t]);

  const applyLook = useCallback((theme: string, language: string) => {
    document.documentElement.dataset.theme = theme === "light" ? "light" : "dark";
    const lang = language === "zh" ? "zh" : "en";
    document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
    document.title = lang === "zh" ? "声渡" : "TunePeerly";
    if (i18n.language !== lang) void i18n.changeLanguage(lang);
    try {
      window.localStorage.setItem("language", lang);
    } catch {
      // Only used to pick the first language on the next start.
    }
    if ("__TAURI_INTERNALS__" in window) {
      const title = document.title;
      void import("@tauri-apps/api/window")
        .then(({ getCurrentWindow }) => getCurrentWindow().setTitle(title))
        .catch(() => {});
    }
  }, []);

  const loadMembers = useCallback(async (playlists: Bootstrap["playlists"]) => {
    const request = ++membersRequest.current;
    const rows = await Promise.all(
      playlists.map((playlist) => api.playlistMembers(playlist.id).then((items) => [playlist.id, items] as const).catch(() => [playlist.id, []] as const)),
    );
    if (request === membersRequest.current) setMembers(Object.fromEntries(rows));
  }, []);

  const refresh = useCallback(async () => {
    const request = ++refreshRequest.current;
    try {
      const next = await api.bootstrap();
      if (request !== refreshRequest.current) return;
      setBoot(next);
      setDownloadFeed({ current: next.download, queue: next.downloadQueue ?? [], history: next.downloadHistory ?? [] });
      setLoadError(null);
      applyLook(next.theme, next.language);
      void loadMembers(next.playlists);
      if (!restored.current) {
        restored.current = true;
        const state = normalizeQueue(next.player);
        queueRef.current = state;
        setQueueState(state);
        const saved = Number(next.settings.volume);
        const level = Number.isFinite(saved) ? Math.min(1, Math.max(0, saved)) : 1;
        setVolumeState(level);
        volumeRef.current = level;
        engine.setVolume(level, false);
        const index = new TrackIndex(next.tracks);
        const keys = (state.contextId === "search" || state.contextId === "problems") && Array.isArray(next.player.contextKeys) ? next.player.contextKeys : null;
        const rebuilt = keys ? keys.map((key) => index.get({ key: String(key) })).filter((track): track is Track => !!track) : null;
        context.current = { id: state.contextId, tracks: rebuilt && rebuilt.length > 0 ? rebuilt : null };
        engine.cue(index.get(state.current) ?? null, state.positionSec);
      }
    } catch (error) {
      if (request === refreshRequest.current) setLoadError(errorText(error));
    }
  }, [applyLook, engine, loadMembers]);

  const scheduleRefresh = useCallback(() => {
    window.clearTimeout(refreshTimer.current);
    refreshTimer.current = window.setTimeout(() => void refresh(), 150);
  }, [refresh]);

  useEffect(() => {
    void refresh();
    let disposed = false;
    const stops = Promise.all([
      listen("library-updated", () => {
        if (!disposed) scheduleRefresh();
      }),
      listen("network-updated", () => {
        if (!disposed) scheduleRefresh();
      }),
      listen<ContentMove>("content-moved", (event) => {
        if (!disposed) setMoves((items) => [...items, event.payload]);
      }),
      listen<OverwriteAsk[]>("overwrite-asks", (event) => {
        if (!disposed) setOverwriteAsks(Array.isArray(event.payload) ? event.payload.slice(0, 3) : []);
      }),
      listen<{ feature?: string; name?: string }>("sync-denied", (event) => {
        if (disposed) return;
        const feature = event.payload.feature ? t(`feature.${event.payload.feature}`, { defaultValue: event.payload.feature }) : "";
        toast(t("toast.syncDenied", { name: event.payload.name || t("sync.thatDevice"), feature }));
      }),
      listen<{ current: DownloadFeed["current"]; queue: DownloadFeed["queue"]; history: DownloadFeed["history"] | null }>("download-updated", (event) => {
        if (disposed) return;
        const row = event.payload;
        setDownloadFeed((prev) => ({
          current: row.current,
          queue: row.queue ?? [],
          history: row.history ?? prev?.history ?? [],
        }));
      }),
      listen<{ from: string; kind: string; body: unknown }>("connect", (event) => {
        if (disposed || !event.payload || typeof event.payload.from !== "string") return;
        connectIn.current?.(event.payload.from, String(event.payload.kind ?? ""), event.payload.body);
      }),
    ]).catch(() => [] as (() => void)[]);
    return () => {
      disposed = true;
      window.clearTimeout(refreshTimer.current);
      refreshRequest.current += 1;
      membersRequest.current += 1;
      lyricsRequest.current = { id: lyricsRequest.current.id + 1, key: null };
      void stops.then((items) => items.forEach((stop) => stop()));
    };
  }, [refresh, scheduleRefresh, t, toast, engine]);

  const publishRef = useRef<() => void>(() => {});
  /** Sends this computer's playback to the other devices soon, once a burst of changes settles. */
  const schedulePublish = useCallback(() => {
    window.clearTimeout(publishTimer.current);
    publishTimer.current = window.setTimeout(() => publishRef.current(), 60);
  }, []);

  const commit = useCallback((state: QueueState, position: number = engine.position) => {
    const trimmed = state.past.length > PAST_LIMIT ? { ...state, past: state.past.slice(-PAST_LIMIT) } : state;
    queueRef.current = trimmed;
    setQueueState(trimmed);
    // A search or problem list cannot be rebuilt from its id, so keep its track keys for the next launch.
    const saved: Record<string, unknown> = { ...trimmed, positionSec: position };
    if ((trimmed.contextId === "search" || trimmed.contextId === "problems") && context.current.id === trimmed.contextId && context.current.tracks) {
      saved.contextKeys = context.current.tracks.slice(0, 2000).map(trackKey);
    }
    void api.savePlayer(saved as unknown as QueueState).catch(() => {});
    schedulePublish();
  }, [engine, schedulePublish]);

  const contextTracks = useCallback((id: string): Track[] => {
    const all = bootRef.current?.tracks ?? [];
    const visible = bootRef.current?.settings.showUnavailable !== false ? all : all.filter((track) => track.badge !== "unavailable");
    const [kind, ...rest] = id.split(":");
    const value = rest.join(":");
    switch (kind) {
      case "liked":
        return all.filter((track) => track.favorite);
      case "playlist":
        return indexRef.current.members(membersRef.current[value] ?? []);
      case "artist":
        return visible.filter((track) => track.artist === value);
      case "album": {
        const { artist, album } = albumParts(value);
        return visible
          .filter((track) => track.album === album && track.artist === artist)
          .sort((a, b) => (a.trackNumber ?? Number.MAX_SAFE_INTEGER) - (b.trackNumber ?? Number.MAX_SAFE_INTEGER) || a.title.localeCompare(b.title));
      }
      case "folder":
        return visible.filter((track) => track.folder === value);
      case "device":
        return all.filter((track) => track.holderIds.includes(value));
      case "random":
        return all.filter(shufflable);
      case "recent": {
        const seen = new Set<string>();
        const out: Track[] = [];
        for (const key of bootRef.current?.history ?? []) {
          const track = indexRef.current.get({ key });
          if (!track || seen.has(trackKey(track))) continue;
          seen.add(trackKey(track));
          out.push(track);
        }
        return out;
      }
      case "added":
        return visible.slice().sort((a, b) => b.addedAt.localeCompare(a.addedAt) || a.title.localeCompare(b.title));
      case "search":
      case "problems":
        return [];
      default:
        return visible;
    }
  }, []);

  const rebuildPool = useCallback((state: QueueState): Track[] => {
    if (state.order === "libraryRandom") return (bootRef.current?.tracks ?? []).filter(shufflable);
    const list = context.current.id === state.contextId && context.current.tracks ? context.current.tracks : contextTracks(state.contextId);
    return list.filter((track) => track.playable);
  }, [contextTracks]);

  const loadLyrics = useCallback((track: Track | undefined) => {
    const key = track?.localFileId ? trackKey(track) : null;
    if (lyricsRequest.current.key === key) return;
    const request = lyricsRequest.current.id + 1;
    lyricsRequest.current = { id: request, key };
    if (!track?.localFileId) {
      setLyrics(null);
      return;
    }
    const lyricKey = trackKey(track);
    setLyrics((value) => (value?.key === lyricKey ? value : null));
    void api.lyrics(track.localFileId).then(
      (text) => {
        if (lyricsRequest.current.id === request) setLyrics({ key: lyricKey, text });
      },
      () => {
        if (lyricsRequest.current.id === request) setLyrics({ key: lyricKey, text: "" });
      },
    );
  }, []);

  const startCurrent = useCallback((state: QueueState) => {
    const track = indexRef.current.get(state.current);
    setFailure(null);
    if (!track) {
      engine.cue(null, 0);
      setLyrics(null);
      return;
    }
    void engine.start(track, state.positionSec, true);
    loadLyrics(track);
  }, [engine, loadLyrics]);

  const playListLocal = useCallback((list: Track[], at: number, contextId: string, shuffle = false) => {
    const chosen = list[at];
    if (!chosen) return;
    if (!chosen.playable) {
      setNotice(t(chosen.reasonKey ?? "notice.offlineTrack"));
      return;
    }
    const pool = list.filter((track, position) => position === at || track.playable);
    context.current = { id: contextId, tracks: pool };
    const state = playFromList({ ...queueRef.current, contextId }, pool.map(refOf), pool.indexOf(chosen), shuffle, Math.random);
    commit(state, 0);
    startCurrent(state);
  }, [commit, setNotice, startCurrent, t]);

  const playAllLocal = useCallback((list: Track[], contextId: string, shuffle = false) => {
    const playable = list.filter((track) => track.playable);
    if (playable.length === 0) {
      setNotice(t("notice.nothingPlayable"));
      return;
    }
    if (shuffle) {
      context.current = { id: contextId, tracks: playable };
        const state = { ...startShuffled(playable.map(refOf), Math.random, contextId, "listShuffle"), repeat: queueRef.current.repeat };
      commit(state, 0);
      startCurrent(state);
      return;
    }
    playListLocal(list, list.indexOf(playable[0]), contextId, false);
  }, [commit, playListLocal, setNotice, startCurrent, t]);

  const shuffleLibraryLocal = useCallback(() => {
    const pool = (bootRef.current?.tracks ?? []).filter(shufflable);
    if (pool.length === 0) {
      setNotice(t("notice.nothingPlayable"));
      return;
    }
    context.current = { id: "random", tracks: pool };
    const state = { ...startLibraryRandom(pool.map(refOf), Math.random, "random"), repeat: queueRef.current.repeat };
    commit(state, 0);
    startCurrent(state);
  }, [commit, setNotice, startCurrent, t]);

  const nextLocal = useCallback(() => {
    const state = queueRef.current;
    const target = nextOrWrap(state, rebuildPool(state).map(refOf), Math.random);
    if (!target.current) return;
    const sameSong = target.current.key === state.current?.key && target.past.length === state.past.length && target.future.length === state.future.length;
    if (sameSong) {
      const restarted = { ...state, positionSec: 0 };
      commit(restarted, 0);
      startCurrent(restarted);
      return;
    }
    commit(target, 0);
    startCurrent(target);
  }, [commit, rebuildPool, startCurrent]);

  const previousLocal = useCallback(() => {
    const state = queueRef.current;
    if (!state.current) return;
    const target = previousState(state, engine.position, rebuildPool(state).map(refOf));
    if (target.current?.key === state.current.key && target.past.length === state.past.length && target.future.length === state.future.length) {
      engine.seek(0);
      return;
    }
    commit(target, 0);
    startCurrent(target);
  }, [commit, engine, rebuildPool, startCurrent]);

  const toggleLocal = useCallback(() => {
    const state = queueRef.current;
    if (!state.current) {
      if (state.future.length > 0) nextLocal();
      else playAllLocal(library, "library");
      return;
    }
    const track = indexRef.current.get(state.current);
    if (!engine.hasSource && track) {
      // Retry with the freshest row: the file may have come back or another device may hold it now.
      const resume = engine.current && trackKey(engine.current) === trackKey(track) ? engine.position : state.positionSec;
      setFailure(null);
      engine.cue(track, resume);
    }
    engine.toggle();
  }, [engine, library, nextLocal, playAllLocal]);

  const jumpLocal = useCallback((at: number) => {
    const target = jumpTo(queueRef.current, at);
    commit(target, 0);
    startCurrent(target);
  }, [commit, startCurrent]);

  /** Queue changes without messages; the device that asked shows them. */
  const localQueue = useMemo(() => ({
    playNext: (items: Track[]) => commit(playNext(queueRef.current, items.map(refOf))),
    add: (items: Track[]) => commit(addToQueue(queueRef.current, items.map(refOf))),
    insert: (at: number, items: Track[]) => {
      const state = queueRef.current;
      const position = Math.max(0, Math.min(at, state.future.length));
      commit({ ...state, future: [...state.future.slice(0, position), ...items.map(refOf), ...state.future.slice(position)] });
    },
    remove: (at: number) => commit(removeAt(queueRef.current, at)),
    move: (from: number, to: number) => commit(moveInQueue(queueRef.current, from, to)),
    clear: () => commit({ ...queueRef.current, future: [] }),
  }), [commit]);

  const shuffleLocal = useCallback(() => {
    const state = queueRef.current;
    const pool = context.current.id === state.contextId && context.current.tracks ? context.current.tracks : contextTracks(state.contextId);
    const next = toggleShuffle(state, pool.filter((track) => track.playable).map(refOf), Math.random);
    commit(next, state.current ? engine.position : 0);
    if (!state.current && next.current) {
        startCurrent(next);
    }
  }, [commit, contextTracks, engine, startCurrent]);

  const setVolumeLocal = useCallback((value: number) => {
    const level = Math.min(1, Math.max(0, value));
    setVolumeState(level);
    setMuted(false);
    volumeRef.current = level;
    mutedRef.current = false;
    engine.setVolume(level, false);
    window.clearTimeout(volumeTimer.current);
    volumeTimer.current = window.setTimeout(() => void api.updateSetting("volume", level).catch(() => {}), 400);
    schedulePublish();
  }, [engine, schedulePublish]);

  const muteLocal = useCallback((value: boolean) => {
    setMuted(value);
    mutedRef.current = value;
    engine.setVolume(volumeRef.current, value);
    schedulePublish();
  }, [engine, schedulePublish]);

  const patchBoot = useCallback((patch: (value: Bootstrap) => Bootstrap) => {
    setBoot((value) => (value ? patch(value) : value));
  }, []);

  const setSetting = useCallback(async (key: string, value: unknown) => {
    const before = bootRef.current;
    if (!before) return;
    if (key === "theme" || key === "language") {
      patchBoot((item) => ({ ...item, [key]: value }));
      applyLook(key === "theme" ? String(value) : before.theme, key === "language" ? String(value) : before.language);
    } else {
      patchBoot((item) => ({ ...item, settings: { ...item.settings, [key]: value } }));
    }
    if (key === "incognito" && value === true) {
      // Like a private session: the other devices stop seeing this one before nothing more is shared.
      publishRef.current();
      leaveConnect.current();
      if (engine.remoteDevice) {
        engine.halt();
        setNotice(t("notice.incognitoStopped"));
      }
    }
    try {
      await api.updateSetting(key, value);
      if (key === "incognito") void api.network("close-route").catch(() => {});
      if (key === "incognito" && value === false) rejoinConnect.current();
      if (key === "showUnavailable" || key === "historyCap" || key === "incognito") scheduleRefresh();
    } catch (error) {
      // Undo only this setting, so a refresh that landed in the meantime is kept.
      if (key === "theme" || key === "language") {
        patchBoot((item) => ({ ...item, [key]: before[key] }));
        const now = bootRef.current ?? before;
        applyLook(key === "theme" ? before.theme : now.theme, key === "language" ? before.language : now.language);
      } else {
        patchBoot((item) => ({ ...item, settings: { ...item.settings, [key]: before.settings[key as keyof typeof before.settings] } }));
      }
      say(error);
    }
  }, [applyLook, engine, patchBoot, say, scheduleRefresh, setNotice, t]);

  const importPaths = useCallback(async (paths: string[], kind: "directory" | "file" | "auto") => {
    if (paths.length === 0) return;
    try {
      const summaries = await api.importPaths(paths, kind);
      const added = summaries.reduce((sum, item) => sum + item.added, 0);
      toast(added > 0 ? t("toast.imported", { count: added }) : t("toast.importedNone"));
      reloadLyrics();
      await refresh();
    } catch (error) {
      say(error);
    }
  }, [refresh, reloadLyrics, say, t, toast]);

  /** Scans every added folder and file again. Tags and lyrics files that changed are read again. */
  const rescan = useCallback(async () => {
    if (scanning.current) return;
    scanning.current = true;
    toast(t("toast.rescanning"));
    try {
      await api.rescan();
      reloadLyrics();
      await refresh();
      toast(t("toast.rescanned"));
    } catch (error) {
      say(error);
    } finally {
      scanning.current = false;
    }
  }, [refresh, reloadLyrics, say, t, toast]);

  const addMusic = useCallback(async (kind: "directory" | "file") => {
    try {
      if (isAndroidApp()) {
        // The system picker copies the chosen tree into app storage in the background; Rust
        // rescans when the copy finishes and tells us here, so nothing polls on this side.
        let unlisten: (() => void) | undefined;
        const done = new Promise<{ added: number; status: string }>((resolve) => {
          const timer = window.setTimeout(() => resolve({ added: 0, status: "timeout" }), 15 * 60 * 1000);
          listen<{ added: number; status: string }>("mobile-import-finished", (event) => {
            window.clearTimeout(timer);
            resolve({ added: event.payload.added, status: event.payload.status });
          })
            .then((stop) => {
              unlisten = stop;
            })
            .catch(() => resolve({ added: 0, status: "failed" }));
        });
        try {
          await api.mobilePickFolder();
          const result = await done;
          if (result.status === "cancelled") return;
          if (result.status !== "ok") {
            // A failed or timed-out import used to look exactly like "nothing happened".
            toast(t("toast.importFailed"));
            return;
          }
          if (result.added > 0) {
            toast(t("toast.imported", { count: result.added }));
            reloadLyrics();
            await refresh();
          }
        } finally {
          unlisten?.();
        }
        return;
      }
      const paths = kind === "directory" ? [await pickFolder()].filter((item): item is string => Boolean(item)) : await pickFiles();
      await importPaths(paths, kind);
    } catch (error) {
      say(error);
    }
  }, [importPaths, refresh, reloadLyrics, say, t, toast]);

  const like = useCallback(async (track: Track) => {
    const key = trackKey(track);
    const remember = (value: boolean) => (ids: string[] | undefined) => {
      const next = new Set(ids ?? []);
      if (track.contentId) {
        if (value) next.add(track.contentId);
        else next.delete(track.contentId);
      }
      return [...next];
    };
    const flip = (value: boolean) =>
      patchBoot((item) => ({
        ...item,
        favoriteIds: remember(value)(item.favoriteIds),
        tracks: item.tracks.map((row) => (trackKey(row) === key ? { ...row, favorite: value } : row)),
      }));
    flip(!track.favorite);
    try {
      const added = await api.toggleFavorite(track.contentId, track.localFileId);
      flip(added);
      toast(added ? t("toast.liked") : t("toast.unliked"));
    } catch (error) {
      flip(track.favorite);
      say(error);
    }
  }, [patchBoot, say, t, toast]);

  const addToPlaylist = useCallback(async (playlistId: string, items: Track[]) => {
    const playlist = bootRef.current?.playlists.find((item) => item.id === playlistId);
    let added = 0;
    let already = 0;
    for (const track of items) {
      try {
        await api.addToPlaylist(playlistId, track.contentId, track.localFileId);
        added += 1;
      } catch (error) {
        if (errorText(error) === "playlist.already") already += 1;
        else {
          say(error);
          break;
        }
      }
    }
    if (added > 0) toast(t("toast.added", { name: playlist?.name ?? "" }));
    else if (already > 0) toast(t("toast.already", { name: playlist?.name ?? "" }));
    scheduleRefresh();
  }, [say, scheduleRefresh, t, toast]);

  const navigate = useCallback((next: Route) => {
    const state = nav.current;
    if (sameRoute(state.stack[state.index], next)) return;
    state.stack = [...state.stack.slice(0, state.index + 1), next].slice(-60);
    state.index = state.stack.length - 1;
    setRoute(next);
    setMenu(null);
    setNavigation({ canBack: state.index > 0, canForward: false });
  }, []);

  const step = useCallback((delta: number) => {
    const state = nav.current;
    const target = state.index + delta;
    if (target < 0 || target >= state.stack.length) return;
    state.index = target;
    setRoute(state.stack[target]);
    setMenu(null);
    setNavigation({ canBack: target > 0, canForward: target < state.stack.length - 1 });
  }, []);

  const createPlaylist = useCallback((items: Track[] = [], open = false) => {
    setDialog({
      kind: "prompt",
      title: t("playlist.create"),
      label: t("playlist.name"),
      value: t("playlist.defaultName", { count: (bootRef.current?.playlists.length ?? 0) + 1 }),
      action: t("common.create"),
      run: async (name) => {
        try {
          const id = await api.createPlaylist(name);
          let added = 0;
          for (const track of items) {
            try {
              await api.addToPlaylist(id, track.contentId, track.localFileId);
              added += 1;
            } catch (error) {
              if (errorText(error) !== "playlist.already") throw error;
            }
          }
          toast(added > 0 ? t("toast.added", { name }) : t("toast.created", { name }));
          await refresh();
          if (open) navigate({ name: "playlist", id });
        } catch (error) {
          say(error);
        }
      },
    });
  }, [navigate, refresh, say, t, toast]);

  const renamePlaylist = useCallback((id: string) => {
    const playlist = bootRef.current?.playlists.find((item) => item.id === id);
    if (!playlist) return;
    setDialog({
      kind: "prompt",
      title: t("playlist.rename"),
      label: t("playlist.name"),
      value: playlist.name,
      action: t("common.save"),
      run: async (name) => {
        try {
          await api.renamePlaylist(id, name);
          await refresh();
        } catch (error) {
          say(error);
        }
      },
    });
  }, [refresh, say, t]);

  const deletePlaylist = useCallback((id: string) => {
    const playlist = bootRef.current?.playlists.find((item) => item.id === id);
    if (!playlist) return;
    setDialog({
      kind: "confirm",
      title: t("playlist.deleteTitle", { name: playlist.name }),
      body: t("playlist.deleteBody"),
      action: t("common.delete"),
      danger: true,
      run: async () => {
        try {
          await api.deletePlaylist(id);
          if (route.name === "playlist" && route.id === id) navigate({ name: "home" });
          toast(t("toast.deleted", { name: playlist.name }));
          await refresh();
        } catch (error) {
          say(error);
        }
      },
    });
  }, [navigate, refresh, route, say, t, toast]);

  const removeFromPlaylist = useCallback(async (playlistId: string, items: Track[]) => {
    try {
      for (const track of items) await api.removePlaylistMember(playlistId, track.contentId, track.localFileId);
      const playlist = bootRef.current?.playlists.find((item) => item.id === playlistId);
      toast(t("toast.removedFrom", { name: playlist?.name ?? "" }));
      await refresh();
    } catch (error) {
      say(error);
    }
  }, [refresh, say, t, toast]);

  const applyPlayingOrder = useCallback((contextId: string, ordered: Track[]) => {
    const playable = ordered.filter((track) => track.playable);
    if (context.current.id === contextId) context.current = { id: contextId, tracks: playable };
    const state = queueRef.current;
    if (state.contextId !== contextId || state.order !== "manual") return;
    const followed = followListOrder(state, playable.map(refOf));
    const same = followed.future.length === state.future.length && followed.future.every((item, index) => item.key === state.future[index]?.key);
    if (!same) commit(followed, engine.position);
  }, [commit, engine]);

  const moveLibrary = useCallback(async (from: number, to: number) => {
    const boot = bootRef.current;
    if (!boot || from === to) return;
    const visible = (track: Track) => boot.settings.showUnavailable !== false || track.badge !== "unavailable";
    const tracks = reorderVisible(boot.tracks, visible, from, to);
    if (tracks === boot.tracks) return;
    const next = { ...boot, tracks };
    bootRef.current = next;
    setBoot(next);
    try {
      await api.setLibraryOrder(tracks.map((track) => trackKey(track)).filter(Boolean));
      applyPlayingOrder("library", tracks.filter(visible));
    } catch (error) {
      say(error);
    }
    scheduleRefresh();
  }, [applyPlayingOrder, say, scheduleRefresh]);

  const movePlaylistMember = useCallback(async (playlistId: string, from: number, to: number) => {
    const list = (membersRef.current[playlistId] ?? []).slice();
    // The page lists only members whose song is known, so `from` and `to` count those rows.
    // Map them onto the full member list, which is what the database numbers.
    const shown: number[] = [];
    list.forEach((ref, at) => {
      if (indexRef.current.get(ref)) shown.push(at);
    });
    const fromMember = shown[from];
    const toMember = shown[to];
    if (fromMember === undefined || toMember === undefined || fromMember === toMember) return;
    const [item] = list.splice(fromMember, 1);
    if (!item) return;
    list.splice(toMember, 0, item);
    const nextMembers = { ...membersRef.current, [playlistId]: list };
    membersRef.current = nextMembers;
    setMembers(nextMembers);
    try {
      await api.movePlaylistMember(playlistId, fromMember, toMember);
      applyPlayingOrder(`playlist:${playlistId}`, indexRef.current.members(list));
    } catch (error) {
      say(error);
    }
    scheduleRefresh();
  }, [applyPlayingOrder, say, scheduleRefresh]);


  // ---- Devices: one active playback device across paired computers, as in Spotify Connect.
  const selfId = useCallback(() => bootRef.current?.deviceId ?? "", []);
  const incognito = useCallback(() => Boolean(bootRef.current?.settings.incognito), []);
  const onlineIds = useCallback(
    () => new Set((bootRef.current?.links ?? []).filter((link) => link.state === "online").map((link) => link.deviceId)),
    [],
  );
  const deviceName = useCallback((deviceId: string) => bootRef.current?.links.find((link) => link.deviceId === deviceId)?.name || t("connect.otherDevice"), [t]);
  /** The active device when it is another one; `null` while this computer plays. */
  const remoteTarget = useCallback((): string | null => {
    const active = activeRef.current;
    const self = selfId();
    return active && self && active !== self && !incognito() && !detachedRef.current ? active : null;
  }, [incognito, selfId]);

  const contextNameOf = useCallback((contextId: string): string => {
    const [kind, ...rest] = contextId.split(":");
    const value = rest.join(":");
    switch (kind) {
      case "playlist":
        return bootRef.current?.playlists.find((item) => item.id === value)?.name ?? "";
      case "album":
        return albumParts(value).album;
      case "artist":
      case "remote":
        return value;
      case "folder":
        return safeName(value);
      case "device":
        return bootRef.current?.links.find((link) => link.deviceId === value)?.name ?? "";
      default:
        return "";
    }
  }, []);

  const publishNow = useCallback(() => {
    window.clearTimeout(publishTimer.current);
    if (incognito() || detachedRef.current || !selfId()) return;
    const state = queueRef.current;
    const track = engine.current ?? indexRef.current.get(state.current) ?? null;
    // Queue positions are counted the same on both sides, so the list stops at the first song without an id.
    const future: ConnectMeta[] = [];
    for (const ref of state.future) {
      if (future.length >= MAX_STATE_QUEUE) break;
      const meta = metaOf(indexRef.current.get(ref));
      if (!meta) break;
      future.push(meta);
    }
    const body: ConnectState = {
      v: CONNECT_VERSION,
      claim: claimRef.current,
      playing: engine.playing,
      positionSec: Math.max(0, engine.position || 0),
      volume: volumeRef.current,
      muted: mutedRef.current,
      track: metaOf(track),
      queue: { future, order: state.order, repeat: state.repeat, contextKind: contextKindOf(state.contextId), contextName: contextNameOf(state.contextId) },
      done: doneRef.current.slice(-16),
    };
    lastPublish.current = { at: performance.now(), signature: publishSignature() };
    void api.network("connect", { kind: "connect-state", body }).catch(() => {});
  }, [contextNameOf, engine, incognito, selfId]);
  publishRef.current = publishNow;

  /** What other devices see; a change is sent at once instead of waiting for the next report. */
  const publishSignature = () => {
    const state = queueRef.current;
    return [
      engine.current?.contentId ?? state.current?.contentId ?? "",
      engine.playing,
      claimRef.current,
      volumeRef.current,
      mutedRef.current,
      state.order,
      state.repeat,
      state.contextId,
      state.future.slice(0, MAX_STATE_QUEUE).map((item) => item.key).join(","),
    ].join("|");
  };

  const sendConnect = useCallback((kind: string, to: string | null, body: unknown) => {
    void api.network("connect", { kind, to, body }).catch(() => {});
  }, []);

  const reevaluateRef = useRef<() => void>(() => {});

  const claim = useCallback(() => {
    claimRef.current = nextClaim(claimRef.current, remotesRef.current, claimFloorRef.current);
    saveClaim(claimRef.current);
    reevaluateRef.current();
    publishNow();
  }, [publishNow]);

  /** The active device went away: its song and queue continue here, paused where it was. */
  const takeOver = useCallback((entry: RemoteEntry, now: number) => {
    const position = extrapolate(entry, now);
    const resolve = (contentId: string) => indexRef.current.get(idRef(contentId));
    const track = entry.track ? resolve(entry.track.contentId) : undefined;
    if (track) {
      const futureTracks = entry.queue.future.map((item) => resolve(item.contentId)).filter((item): item is Track => Boolean(item));
      const contextId = contextIdFrom(entry.queue.contextKind, entry.queue.contextName);
      context.current = { id: contextId, tracks: isPortableContext(entry.queue.contextKind) ? null : [track, ...futureTracks] };
      const state: QueueState = { ...queueRef.current, order: entry.queue.order, repeat: entry.queue.repeat, contextId, current: refOf(track), future: futureTracks.map(refOf), positionSec: position };
      setFailure(null);
      commit(state, position);
      engine.cue(track, position);
      loadLyrics(track);
    }
    claimRef.current = nextClaim(claimRef.current, remotesRef.current, claimFloorRef.current);
    saveClaim(claimRef.current);
    toast(t("connect.fallback", { name: deviceName(entry.deviceId) }));
    publishNow();
  }, [commit, deviceName, engine, loadLyrics, publishNow, t, toast]);

  const reevaluate = useCallback(() => {
    const self = selfId();
    if (!self) return;
    const now = performance.now();
    const online = incognito() || detachedRef.current ? new Set<string>() : onlineIds();
    const next = pickActive(self, claimRef.current, remotesRef.current, online, now);
    const before = activeRef.current || self;
    if (next === before) {
      if (!activeRef.current) {
        activeRef.current = next;
        setActiveId(next);
      }
      return;
    }
    activeRef.current = next;
    setActiveId(next);
    setOverlay(null);
    if (next !== self) {
      // Only one device plays: another one took over.
      if (engine.playing) engine.pause();
      return;
    }
    const gone = remotesRef.current[before];
    if (gone && !incognito() && !detachedRef.current && (!online.has(before) || !isFresh(gone, now))) takeOver(gone, now);
  }, [engine, incognito, onlineIds, selfId, takeOver]);
  reevaluateRef.current = reevaluate;

  const setRemoteEntries = useCallback((next: Record<string, RemoteEntry>) => {
    remotesRef.current = next;
    setRemotes(next);
  }, []);

  /** Incognito: forget the other devices without taking over their playback. */
  const leaveConnect = useRef<() => void>(() => {});
  leaveConnect.current = () => {
    setRemoteEntries({});
    activeRef.current = selfId();
    setActiveId(selfId());
    setOverlay(null);
  };
  const rejoinConnect = useRef<() => void>(() => {});
  rejoinConnect.current = () => {
    if (detachedRef.current) return;
    if (engine.playing) claim();
    else publishNow();
  };

  const finishPending = useCallback((id: string) => {
    const pending = pendingRef.current.get(id);
    if (pending) window.clearTimeout(pending.timer);
    pendingRef.current.delete(id);
    return pending;
  }, []);

  /** Sends a control to the active device and shows its effect until the device confirms it. */
  const sendCommand = useCallback((target: string, command: ConnectCommand, patch?: Omit<Overlay, "ids" | "at">) => {
    const id = commandId();
    sendConnect("connect-cmd", target, { v: CONNECT_VERSION, id, ...command });
    if (patch) {
      setOverlay((value) => addOverlay(value, id, patch, performance.now()));
      window.setTimeout(() => setOverlay((value) => settleOverlay(value, [], performance.now())), ACK_TIMEOUT_MS + 50);
    }
    const timer = window.setTimeout(() => {
      const pending = finishPending(id);
      if (pending?.transfer) setNotice(t("connect.noResponse", { name: deviceName(target) }));
    }, ACK_TIMEOUT_MS);
    pendingRef.current.set(id, { timer, transfer: command.op === "transfer" });
  }, [deviceName, finishPending, sendConnect, setNotice, t]);

  /** Moves this computer's playback to another device, which becomes the active one. */
  const transferTo = useCallback((target: string) => {
    if (target === selfId() || incognito() || detachedRef.current) return;
    if (detachedPeersRef.current.has(target)) {
      setNotice(t("connect.detachedThere"));
      return;
    }
    const state = queueRef.current;
    const ids = (items: TrackRef[]) => items.map((item) => indexRef.current.get(item)?.contentId ?? item.contentId).filter(isContentId);
    const currentTrack = indexRef.current.get(state.current);
    const id = commandId();
    const body: ConnectTransfer = {
      v: CONNECT_VERSION,
      id,
      positionSec: Math.max(0, engine.position || state.positionSec || 0),
      playing: engine.playing,
      queue: {
        past: ids(state.past).slice(-MAX_PAST),
        current: currentTrack?.contentId ?? null,
        future: ids(state.future).slice(0, MAX_REFS),
        order: state.order,
        repeat: state.repeat,
        contextKind: contextKindOf(state.contextId),
        contextName: contextNameOf(state.contextId),
      },
    };
    sendConnect("connect-transfer", target, body);
    const timer = window.setTimeout(() => {
      if (finishPending(id)) setNotice(t("connect.noResponse", { name: deviceName(target) }));
    }, ACK_TIMEOUT_MS * 2);
    pendingRef.current.set(id, { timer, transfer: true });
  }, [contextNameOf, deviceName, engine, finishPending, incognito, selfId, sendConnect, setNotice, t]);

  const markDetachedPeers = useCallback((update: (set: Set<string>) => void) => {
    const next = new Set(detachedPeersRef.current);
    update(next);
    if (next.size === detachedPeersRef.current.size && [...next].every((id) => detachedPeersRef.current.has(id))) return;
    detachedPeersRef.current = next;
    setDetachedPeers(next);
  }, []);

  /**
   * Leaves device control: this computer keeps its own song and queue and plays on its own, and the
   * others forget it. Used both by a computer that was only a remote and by the active one ("play
   * only here"); the device that is playing elsewhere keeps playing.
   */
  const detach = useCallback(() => {
    const self = selfId();
    if (!self || incognito() || detachedRef.current) return;
    const wasRemote = remoteTarget() !== null;
    window.clearTimeout(claimOnJoinRef.current);
    claimOnJoinRef.current = undefined;
    detachedRef.current = true;
    setDetached(true);
    // The backend says "connect-leave" and from now on refuses controls and drops reports.
    void api.network("connect-mode", { detached: true }).catch(() => {});
    for (const pending of pendingRef.current.values()) window.clearTimeout(pending.timer);
    pendingRef.current.clear();
    setRemoteEntries({});
    activeRef.current = self;
    setActiveId(self);
    setOverlay(null);
    toast(t(wasRemote ? "connect.detachedRemote" : "connect.detachedSolo"));
  }, [incognito, remoteTarget, selfId, setRemoteEntries, t, toast]);

  /**
   * Joins device control again. Music playing here becomes the group's playback when `claimHere`;
   * otherwise this computer reports itself and follows whichever device is active.
   */
  const rejoin = useCallback(async (claimHere: boolean) => {
    if (!detachedRef.current) return;
    detachedRef.current = false;
    setDetached(false);
    // Waits until the backend lets reports out again, so the first one is not dropped.
    await api.network("connect-mode", { detached: false }).catch(() => {});
    publishNow();
    if (!claimHere || !engine.playing) return;
    // The claims of the others were forgotten while detached, so this one is taken after their
    // answers to the report above arrive, or after a moment when nobody answers.
    window.clearTimeout(claimOnJoinRef.current);
    claimOnJoinRef.current = window.setTimeout(() => {
      claimOnJoinRef.current = undefined;
      if (engine.playing && !detachedRef.current) claim();
    }, 1500);
  }, [claim, engine, publishNow]);

  /** The device picker: choosing a device moves playback there, or joins again while detached. */
  const pickDevice = useCallback((target: string) => {
    const action = pickAction({
      self: selfId(),
      target,
      active: activeRef.current || selfId(),
      detached: detachedRef.current,
      detachedPeers: detachedPeersRef.current,
      hasTrack: Boolean(queueRef.current.current),
      playing: engine.playing,
    });
    switch (action.kind) {
      case "unavailable":
        setNotice(t("connect.detachedThere"));
        return;
      case "transfer":
        transferTo(action.to);
        return;
      case "remoteTransfer":
        sendCommand(action.via, { op: "transfer", to: action.to });
        return;
      case "rejoin":
        void rejoin(action.claim);
        return;
      case "rejoinTransfer":
        void (async () => {
          detachedRef.current = false;
          setDetached(false);
          await api.network("connect-mode", { detached: false }).catch(() => {});
          transferTo(action.to);
        })();
        return;
      default:
        return;
    }
  }, [engine, rejoin, selfId, sendCommand, setNotice, t, transferTo]);

  const resolveIds = useCallback((ids: string[]) => ids.map((id) => indexRef.current.get(idRef(id))).filter((item): item is Track => Boolean(item)), []);

  /** Carries out a control another device sent. The answer is `null` when it worked, or a message key. */
  const runCommand = useCallback((from: string, body: Record<string, unknown>): string | null => {
    const op = String(body.op ?? "");
    if (op === "transfer") {
      const to = String(body.to ?? "");
      if (to !== selfId()) transferTo(to);
      return null;
    }
    // Whoever sent this sees this computer as the active device; make that true here too.
    if (activeRef.current !== selfId()) claim();
    const at = Number(body.at ?? 0);
    const sameAt = (index: number) => queueRef.current.future[index] && indexRef.current.get(queueRef.current.future[index])?.contentId === body.contentId;
    switch (op) {
      case "toggle":
        toggleLocal();
        return null;
      case "play":
        if (!engine.playing) toggleLocal();
        return null;
      case "pause":
        engine.pause();
        return null;
      case "next":
        nextLocal();
        return null;
      case "prev":
        previousLocal();
        return null;
      case "seek":
        engine.seek(Number(body.sec) || 0);
        return null;
      case "skip":
        engine.skip(Number(body.delta) || 0);
        return null;
      case "volume":
        setVolumeLocal(Number(body.level));
        return null;
      case "mute":
        muteLocal(Boolean(body.muted));
        return null;
      case "repeat":
        commit({ ...queueRef.current, repeat: body.mode === "list" || body.mode === "one" ? body.mode : "off" });
        return null;
      case "shuffle":
        shuffleLocal();
        return null;
      case "shuffleLibrary":
        shuffleLibraryLocal();
        return null;
      case "clear":
        localQueue.clear();
        return null;
      case "playList": {
        const ids = Array.isArray(body.refs) ? body.refs.filter(isContentId) : [];
        const chosenId = ids[Number(body.index) || 0];
        const list = resolveIds(ids);
        const chosen = list.findIndex((track) => track.contentId === chosenId);
        if (chosen < 0 || !list[chosen].playable) return list[chosen]?.reasonKey ?? "notice.offlineTrack";
        playListLocal(list, chosen, contextIdFrom(String(body.contextKind ?? ""), String(body.contextName ?? "")), Boolean(body.shuffle));
        return null;
      }
      case "add":
      case "playNext":
      case "insert": {
        const list = resolveIds(Array.isArray(body.refs) ? body.refs.filter(isContentId) : []);
        if (list.length === 0) return "connect.notHere";
        if (op === "add") localQueue.add(list);
        else if (op === "playNext") localQueue.playNext(list);
        else localQueue.insert(at, list);
        return null;
      }
      case "remove":
        if (!sameAt(at)) return "connect.queueChanged";
        localQueue.remove(at);
        return null;
      case "jump":
        if (!sameAt(at)) return "connect.queueChanged";
        jumpLocal(at);
        return null;
      case "move": {
        const fromAt = Number(body.from ?? 0);
        if (!sameAt(fromAt)) return "connect.queueChanged";
        localQueue.move(fromAt, Number(body.to ?? 0));
        return null;
      }
      default:
        void from;
        return "connect.failed";
    }
  }, [claim, commit, engine, jumpLocal, localQueue, muteLocal, nextLocal, playListLocal, previousLocal, resolveIds, selfId, setVolumeLocal, shuffleLibraryLocal, shuffleLocal, toggleLocal, transferTo]);

  /** Playback moved here from another device. */
  const receiveTransfer = useCallback((body: ConnectTransfer): string | null => {
    const queueIn = body.queue;
    const track = queueIn.current ? indexRef.current.get(idRef(queueIn.current)) : undefined;
    if (queueIn.current && (!track || !track.playable)) return track?.reasonKey ?? "notice.offlineTrack";
    claimRef.current = nextClaim(claimRef.current, remotesRef.current, claimFloorRef.current);
    saveClaim(claimRef.current);
    reevaluateRef.current();
    if (track) {
      const past = resolveIds(queueIn.past);
      const future = resolveIds(queueIn.future);
      const contextId = contextIdFrom(queueIn.contextKind, queueIn.contextName);
      context.current = { id: contextId, tracks: isPortableContext(queueIn.contextKind) ? null : [...past, track, ...future] };
      const state: QueueState = { order: queueIn.order, repeat: queueIn.repeat, contextId, past: past.map(refOf), current: refOf(track), future: future.map(refOf), positionSec: body.positionSec };
      setFailure(null);
      commit(state, body.positionSec);
      if (body.playing) void engine.start(track, body.positionSec, true);
      else engine.cue(track, body.positionSec);
      loadLyrics(track);
    }
    publishNow();
    return null;
  }, [commit, engine, loadLyrics, publishNow, resolveIds]);

  const rememberDone = useCallback((id: string) => {
    doneRef.current = [...doneRef.current.filter((item) => item !== id), id].slice(-16);
  }, []);

  connectIn.current = (from, kind, raw) => {
    if (incognito() || !raw || typeof raw !== "object" || from === selfId()) return;
    if (!acceptsInbound(detachedRef.current, kind)) return;
    const body = raw as Record<string, unknown>;
    if (kind === "connect-leave") {
      // That device plays on its own now: it is no longer active here and cannot be controlled.
      markDetachedPeers((set) => set.add(from));
      if (from in remotesRef.current) {
        setRemoteEntries(forgetDevice(remotesRef.current, from));
        if (activeRef.current === from) setOverlay(null);
        reevaluate();
      }
      return;
    }
    if (kind === "connect-state") {
      const state = body as unknown as ConnectState;
      const now = performance.now();
      const known = from in remotesRef.current;
      claimFloorRef.current = Math.max(claimFloorRef.current, Number(state.claim) || 0);
      const entry = receiveState(remotesRef.current[from], from, state, now);
      if (entry === remotesRef.current[from]) return;
      markDetachedPeers((set) => set.delete(from));
      setRemoteEntries({ ...remotesRef.current, [from]: entry });
      if (claimOnJoinRef.current !== undefined) {
        window.clearTimeout(claimOnJoinRef.current);
        claimOnJoinRef.current = undefined;
        // Taken before the active device is worked out, so the music playing here is not paused first.
        if (engine.playing) {
          claim();
          return;
        }
      }
      // A device that just joined (or rejoined) learns who is playing without waiting for the next report.
      if (!known) schedulePublish();
      if (activeRef.current === from && overlayRef.current) setOverlay(settleOverlay(overlayRef.current, state.done ?? [], now));
      reevaluate();
      return;
    }
    if (kind === "connect-ack") {
      const ack = body as unknown as ConnectAck;
      const pending = finishPending(ack.id);
      if (ack.ok) {
        // The other device took over; stop here at once rather than when its report arrives.
        if (pending?.transfer && !remoteTarget() && engine.playing) engine.pause();
        return;
      }
      setOverlay(null);
      if (ack.reason === "connect.detachedThere") {
        markDetachedPeers((set) => set.add(from));
        if (from in remotesRef.current) setRemoteEntries(forgetDevice(remotesRef.current, from));
        reevaluate();
      }
      setNotice(t(ack.reason ?? "connect.failed", { defaultValue: t("connect.failed") }));
      return;
    }
    const id = typeof body.id === "string" ? body.id : "";
    if (!id) return;
    const reason = kind === "connect-cmd" ? runCommand(from, body) : kind === "connect-transfer" ? receiveTransfer(body as unknown as ConnectTransfer) : "connect.failed";
    rememberDone(id);
    const ack: ConnectAck = reason ? { v: CONNECT_VERSION, id, ok: false, reason } : { v: CONNECT_VERSION, id, ok: true };
    sendConnect("connect-ack", from, ack);
    schedulePublish();
  };

  // Reports: changes go out within a second, and a quiet device repeats itself so it is not taken for gone.
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (incognito() || detachedRef.current || onlineIds().size === 0) return;
      const now = performance.now();
      const signature = publishSignature();
      const quiet = now - lastPublish.current.at >= (engine.playing ? HEARTBEAT_PLAYING_MS : HEARTBEAT_PAUSED_MS);
      if (quiet || signature !== lastPublish.current.signature) publishNow();
      reevaluate();
    }, 1000);
    return () => window.clearInterval(timer);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engine, incognito, onlineIds, publishNow, reevaluate]);

  // Detaching is not saved: a reloaded window makes sure the backend takes part again too.
  useEffect(() => {
    void api.network("connect-mode", { detached: false }).catch(() => {});
  }, []);

  // A device that just connected learns at once who is playing; the second report covers a slow handshake.
  const onlineKey = (boot?.links ?? []).filter((link) => link.state === "online").map((link) => link.deviceId).sort().join(",");
  useEffect(() => {
    reevaluate();
    if (!onlineKey) return;
    const first = window.setTimeout(() => publishNow(), 2000);
    const second = window.setTimeout(() => publishNow(), 6000);
    return () => {
      window.clearTimeout(first);
      window.clearTimeout(second);
    };
  }, [onlineKey, publishNow, reevaluate]);

  // Controls go to the active device. Each one acts here while this computer is the active device.
  const throttledSeek = useMemo(() => throttle<{ target: string; sec: number }>(({ target, sec }) => sendCommand(target, { op: "seek", sec }, { positionSec: sec })), [sendCommand]);
  const throttledVolume = useMemo(() => throttle<{ target: string; level: number }>(({ target, level }) => sendCommand(target, { op: "volume", level }, { volume: level, muted: false })), [sendCommand]);
  const remoteEntryNow = () => {
    const target = remoteTarget();
    const entry = target ? remotesRef.current[target] : undefined;
    return target && entry ? { target, view: viewOf(entry, overlayRef.current) } : null;
  };

  const toggle = useCallback(() => {
    const remote = remoteEntryNow();
    if (remote) sendCommand(remote.target, { op: "toggle" }, { playing: !remote.view.playing });
    else toggleLocal();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sendCommand, toggleLocal]);
  const play = useCallback(() => {
    const remote = remoteEntryNow();
    if (remote) sendCommand(remote.target, { op: "play" }, { playing: true });
    else if (!engine.playing) toggleLocal();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engine, sendCommand, toggleLocal]);
  const pause = useCallback(() => {
    const remote = remoteEntryNow();
    if (remote) sendCommand(remote.target, { op: "pause" }, { playing: false });
    else engine.pause();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engine, sendCommand]);
  const seek = useCallback((sec: number) => {
    const target = remoteTarget();
    if (target) throttledSeek({ target, sec: Math.max(0, sec) });
    else engine.seek(sec);
  }, [engine, remoteTarget, throttledSeek]);
  const skip = useCallback((delta: number) => {
    const remote = remoteEntryNow();
    if (remote) sendCommand(remote.target, { op: "skip", delta }, { positionSec: Math.max(0, extrapolate(remote.view, performance.now()) + delta) });
    else engine.skip(delta);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engine, sendCommand]);
  const next = useCallback(() => {
    const target = remoteTarget();
    if (target) sendCommand(target, { op: "next" });
    else nextLocal();
  }, [nextLocal, remoteTarget, sendCommand]);
  const previous = useCallback(() => {
    const target = remoteTarget();
    if (target) sendCommand(target, { op: "prev" });
    else previousLocal();
  }, [previousLocal, remoteTarget, sendCommand]);
  const toggleShuffleRouted = useCallback(() => {
    const target = remoteTarget();
    if (target) sendCommand(target, { op: "shuffle" });
    else shuffleLocal();
  }, [remoteTarget, sendCommand, shuffleLocal]);
  const cycleRepeat_ = useCallback(() => {
    const remote = remoteEntryNow();
    if (remote) sendCommand(remote.target, { op: "repeat", mode: cycleRepeat(remote.view.queue.repeat) });
    else commit({ ...queueRef.current, repeat: cycleRepeat(queueRef.current.repeat) });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [commit, sendCommand]);
  const setVolume = useCallback((value: number) => {
    const level = Math.min(1, Math.max(0, value));
    const target = remoteTarget();
    if (target) throttledVolume({ target, level });
    else setVolumeLocal(level);
  }, [remoteTarget, setVolumeLocal, throttledVolume]);
  const toggleMute = useCallback(() => {
    const remote = remoteEntryNow();
    if (remote) sendCommand(remote.target, { op: "mute", muted: !remote.view.muted }, { muted: !remote.view.muted });
    else muteLocal(!mutedRef.current);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [muteLocal, sendCommand]);

  const sendList = useCallback((target: string, list: Track[], at: number, contextId: string, shuffle: boolean) => {
    const chosen = list[at];
    if (!chosen?.contentId) {
      setNotice(t(chosen?.reasonKey ?? "notice.offlineTrack"));
      return;
    }
    const withIds = list.filter((track, position) => position === at || Boolean(track.contentId));
    const cut = windowAround(withIds, withIds.indexOf(chosen), MAX_REFS);
    sendCommand(target, {
      op: "playList",
      refs: cut.items.map((track) => track.contentId as string),
      index: cut.index,
      shuffle,
      contextKind: contextKindOf(contextId),
      contextName: contextNameOf(contextId),
    }, { playing: true });
  }, [contextNameOf, sendCommand, setNotice, t]);

  const playList = useCallback((list: Track[], at: number, contextId: string, shuffle = false) => {
    const target = remoteTarget();
    if (target) sendList(target, list, at, contextId, shuffle);
    else playListLocal(list, at, contextId, shuffle);
  }, [playListLocal, remoteTarget, sendList]);

  const playAll = useCallback((list: Track[], contextId: string, shuffle = false) => {
    const target = remoteTarget();
    if (!target) {
      playAllLocal(list, contextId, shuffle);
      return;
    }
    const playable = list.filter((track) => track.playable && track.contentId);
    if (playable.length === 0) {
      setNotice(t("notice.nothingPlayable"));
      return;
    }
    sendList(target, playable, shuffle ? Math.floor(Math.random() * playable.length) : 0, contextId, shuffle);
  }, [playAllLocal, remoteTarget, sendList, setNotice, t]);

  const shuffleLibrary = useCallback(() => {
    const target = remoteTarget();
    if (target) sendCommand(target, { op: "shuffleLibrary" }, { playing: true });
    else shuffleLibraryLocal();
  }, [remoteTarget, sendCommand, shuffleLibraryLocal]);

  const jump = useCallback((at: number) => {
    const remote = remoteEntryNow();
    if (!remote) {
      jumpLocal(at);
      return;
    }
    const item = remote.view.queue.future[at];
    if (item) sendCommand(remote.target, { op: "jump", at, contentId: item.contentId }, { playing: true });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jumpLocal, sendCommand]);

  const queueActions = useMemo(() => {
    const ids = (items: Track[]) => items.map((track) => track.contentId).filter(isContentId).slice(0, MAX_REFS);
    const routed = (items: Track[], send: (target: string, refs: string[]) => void, local: () => void) => {
      const target = remoteTarget();
      if (!target) {
        local();
        return true;
      }
      const refs = ids(items);
      if (refs.length === 0) {
        setNotice(t("notice.offlineTrack"));
        return false;
      }
      send(target, refs);
      return true;
    };
    const queuedToast = (items: Track[]) => toast(items.length === 1 ? t("toast.queued") : t("toast.queuedMany", { count: items.length }));
    const byIndex = (at: number, send: (target: string, contentId: string) => void, local: () => void) => {
      const remote = remoteEntryNow();
      if (!remote) {
        local();
        return;
      }
      const item = remote.view.queue.future[at];
      if (item) send(remote.target, item.contentId);
    };
    return {
      playNext: (items: Track[]) => {
        if (routed(items, (target, refs) => sendCommand(target, { op: "playNext", refs }), () => localQueue.playNext(items))) {
          toast(items.length === 1 ? t("toast.playNext") : t("toast.playNextMany", { count: items.length }));
        }
      },
      add: (items: Track[]) => {
        if (routed(items, (target, refs) => sendCommand(target, { op: "add", refs }), () => localQueue.add(items))) queuedToast(items);
      },
      insert: (at: number, items: Track[]) => {
        if (routed(items, (target, refs) => sendCommand(target, { op: "insert", at: Math.max(0, at), refs }), () => localQueue.insert(at, items))) queuedToast(items);
      },
      remove: (at: number) => byIndex(at, (target, contentId) => sendCommand(target, { op: "remove", at, contentId }), () => localQueue.remove(at)),
      move: (from: number, to: number) => byIndex(from, (target, contentId) => sendCommand(target, { op: "move", from, to: Math.max(0, to), contentId }), () => localQueue.move(from, to)),
      clear: () => {
        const target = remoteTarget();
        if (target) sendCommand(target, { op: "clear" });
        else localQueue.clear();
      },
      jump,
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jump, localQueue, remoteTarget, sendCommand, setNotice, t, toast]);

  handlers.current = {
    ended: () => {
      const state = queueRef.current;
      const track = indexRef.current.get(state.current);
      if (state.repeat === "one" && track) {
        // Persist the restart immediately.  Calling the engine alone leaves
        // the saved queue at the old end position when the app is closed
        // during the next loop.
        const restarted = { ...state, positionSec: 0 };
        commit(restarted, 0);
        startCurrent(restarted);
        return;
      }
      const result = onEnded(state, rebuildPool(state).map(refOf), Math.random);
      if (result.increment && result.state.current) {
        commit(result.state, 0);
        startCurrent(result.state);
        return;
      }
      const tracks = contextTracks("library").filter((item) => item.playable);
      const continued = continueLibrary(state, tracks.map(refOf), Math.random);
      if (!continued.current) {
        commit({ ...state, positionSec: 0 }, 0);
        engine.cue(track ?? null, 0);
        return;
      }
      context.current = { id: continued.contextId, tracks };
      commit(continued, 0);
      startCurrent(continued);
    },
    failed: (track, info) => {
      setFailure({ key: trackKey(track), reason: info.reason });
      setNotice(t(info.reason, { defaultValue: t("notice.playbackFailed") }));
      if (track.localFileId && info.kind !== "offline" && info.kind !== "transport") {
        void api.playbackResult(track.localFileId, track.contentId, "failed", info.reason).then(scheduleRefresh).catch(() => {});
      }
    },
    // Counted once the song was really heard, not only when it plays to the end.
    heard: (track) => {
      if (bootRef.current?.settings.incognito || bootRef.current?.settings.historyPaused) return;
      const key = trackKey(track);
      patchBoot((item) => ({ ...item, history: [key, ...item.history.filter((value) => value !== key)] }));
      void api.recordHistory(track.contentId, track.localFileId).catch(() => {});
    },
    paused: (_track, position) => {
      void api.saveProgress(position).catch(() => {});
      schedulePublish();
    },
    seeked: (_track, position) => {
      void api.saveProgress(position).catch(() => {});
      schedulePublish();
    },
    progress: (position) => void api.saveProgress(position).catch(() => {}),
    activity: (playing, track, bufferEnough) => {
      void api.network("activity", { playing, localFileId: track?.localFileId ?? null, bufferEnough }).catch(() => {});
      // Playback that starts here makes this the active device, and every other device pauses.
      if (playing && !bootRef.current?.settings.incognito && !detachedRef.current && (claimRef.current === 0 || activeRef.current !== selfId())) claim();
      schedulePublish();
    },
    playable: (track) => {
      if (track.playbackStatus !== "playable") void api.playbackResult(track.localFileId, track.contentId, "playable", null).catch(() => {});
    },
  };

  useEffect(() => {
    const flush = () => {
      void api.saveProgress(engine.position).catch(() => {});
      schedulePublish();
    };
    window.addEventListener("beforeunload", flush);
    return () => window.removeEventListener("beforeunload", flush);
  }, [engine, schedulePublish]);

  const favoriteIds = useMemo(() => new Set(boot?.favoriteIds ?? []), [boot?.favoriteIds]);
  const selfDevice = boot?.deviceId ?? "";
  const remoteEntry = activeId && activeId !== selfDevice && !settings.incognito && !detached ? remotes[activeId] : undefined;
  const remoteName = remoteEntry ? (boot?.links.find((link) => link.deviceId === remoteEntry.deviceId)?.name || t("connect.otherDevice")) : "";
  const remote: RemotePlayback | null = useMemo(
    () => (remoteEntry ? { ...viewOf(remoteEntry, overlay), name: remoteName } : null),
    [remoteEntry, overlay, remoteName],
  );
  /** Songs of the active device's queue, shown with this computer's copy when there is one. */
  const remoteTracks = useMemo(() => {
    const map = new Map<string, Track>();
    if (!remote) return map;
    for (const meta of [remote.track, ...remote.queue.future]) {
      if (!meta || map.has(meta.contentId)) continue;
      map.set(meta.contentId, index.get(idRef(meta.contentId)) ?? metaTrack(meta, remote.deviceId, favoriteIds));
    }
    return map;
  }, [remote, index, favoriteIds]);
  const lookup = useCallback((ref: TrackRef | null | undefined): Track | undefined => {
    if (!ref) return undefined;
    return index.get(ref) ?? (ref.contentId ? remoteTracks.get(ref.contentId) : undefined);
  }, [index, remoteTracks]);
  const lookupRef = useRef(lookup);
  lookupRef.current = lookup;
  const shown = remote ? (remote.track ? remoteTracks.get(remote.track.contentId) ?? null : null) : current ?? null;
  const shownQueue: QueueState = useMemo(() => {
    if (!remote) return queue;
    return {
      order: remote.queue.order,
      repeat: remote.queue.repeat,
      contextId: contextIdFrom(remote.queue.contextKind, remote.queue.contextName),
      past: [],
      current: remote.track ? idRef(remote.track.contentId) : null,
      future: remote.queue.future.map((item) => idRef(item.contentId)),
      positionSec: 0,
    };
  }, [remote, queue]);
  const shownQueueRef = useRef(shownQueue);
  shownQueueRef.current = shownQueue;

  // Every progress bar and play button shows the active device while another device plays.
  useEffect(() => {
    if (!remote) {
      clock.show(null);
      transport.show(null);
      return;
    }
    const duration = remote.track?.durationMs ? remote.track.durationMs / 1000 : 0;
    const tick = () => clock.show({ position: extrapolate(remote, performance.now()), duration });
    tick();
    transport.show({ playing: remote.playing });
    if (!remote.playing) return;
    const timer = window.setInterval(tick, 250);
    return () => window.clearInterval(timer);
  }, [remote]);
  useEffect(() => () => {
    clock.show(null);
    transport.show(null);
  }, []);

  const devices: ConnectDevice[] = useMemo(() => {
    const active = remote ? remote.deviceId : selfDevice;
    const list: ConnectDevice[] = [{ deviceId: selfDevice, name: t("connect.thisComputer"), self: true, active: active === selfDevice, detached }];
    if (settings.incognito) return list;
    for (const link of boot?.links ?? []) {
      if (link.state !== "online" || link.deviceId === selfDevice) continue;
      const apart = detachedPeers.has(link.deviceId);
      list.push({ deviceId: link.deviceId, name: link.name || t("connect.otherDevice"), self: false, active: !detached && !apart && link.deviceId === active, detached: apart });
    }
    return list;
  }, [boot?.links, detached, detachedPeers, remote, selfDevice, settings.incognito, t]);

  useEffect(() => {
    loadLyrics(shown ?? undefined);
  }, [shown, loadLyrics, lyricsEpoch]);

  const findTracks = useCallback((text: string, scope: "all" | "queue" | string) => {
    if (scope === "queue") {
      const state = shownQueueRef.current;
      return filterText([state.current, ...state.future].map((item) => lookupRef.current(item)).filter((item): item is Track => Boolean(item)), text);
    }
    if (scope.startsWith("playlist:")) return filterText(indexRef.current.members(membersRef.current[scope.slice(9)] ?? []), text);
    return null;
  }, []);

  return {
    boot,
    loadError,
    settings,
    tracks,
    library,
    liked,
    recent,
    index,
    members,
    queue: shownQueue,
    current: shown,
    shown,
    lookup,
    engine,
    volume: remote ? remote.volume : volume,
    muted: remote ? remote.muted : muted,
    remote,
    devices,
    activeId: remote ? remote.deviceId : selfId(),
    transferTo: pickDevice,
    detached,
    detach,
    rejoin,
    lyrics,
    failure,
    downloadFeed,
    route,
    navigation,
    queueOpen,
    noticesOpen,
    unreadNotices: (boot?.notices ?? []).filter((item) => !item.read).length,
    menu,
    dialog,
    toasts,
    alerts,
    dismissAlert,
    moves,
    overwriteAsks,
    search,
    setSearch,
    setQueueOpen,
    setNoticesOpen,
    setMenu,
    setDialog,
    setNotice,
    setMoves,
    toast,
    say,
    refresh,
    scheduleRefresh,
    navigate,
    back: () => step(-1),
    forward: () => step(1),
    playList,
    playAll,
    shuffleLibrary,
    toggle,
    play,
    pause,
    seek,
    skip,
    next,
    previous,
    queueActions,
    toggleShuffle: toggleShuffleRouted,
    cycleRepeat: cycleRepeat_,
    setVolume,
    toggleMute,
    setSetting,
    importPaths,
    rescan,
    addMusic,
    like,
    addToPlaylist,
    createPlaylist,
    renamePlaylist,
    deletePlaylist,
    removeFromPlaylist,
    movePlaylistMember,
    moveLibrary,
    contextTracks,
    findTracks,
  };
}

export type AppModel = ReturnType<typeof useModel>;

const AppContext = createContext<AppModel | null>(null);

export function AppProvider({ children }: { children: ReactNode }) {
  const model = useModel();
  return <AppContext.Provider value={model}>{children}</AppContext.Provider>;
}

export function useApp(): AppModel {
  const value = useContext(AppContext);
  if (!value) throw new Error("AppProvider missing");
  return value;
}
