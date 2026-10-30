// Original work 漏 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { listen } from "@tauri-apps/api/event";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { api, errorText, pickFiles, pickFolder, type Bootstrap, type ContentMove, type DownloadFeed, type MemberRef, type OverwriteAsk, type Settings, type Track } from "../api";
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
  queuedIndex,
  removeAt,
  startLibraryRandom,
  startShuffled,
  toggleShuffle,
  type QueueState,
  type TrackRef,
} from "../player/advance";
import { PlaybackEngine, type EngineEvents } from "../player/engine";

export type PlaybackMirror = {
  deviceId: string;
  contentId: string;
  seconds: number;
  playing: boolean;
  duration: number;
  title: string;
  artist: string;
  album: string;
  at: number;
};

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

/** Playback position of a followed device, advanced from the last report. */
export function mirrorPosition(mirror: PlaybackMirror, now = performance.now()): number {
  const elapsed = mirror.playing ? Math.max(0, (now - mirror.at) / 1000) : 0;
  const raw = Math.max(0, mirror.seconds + elapsed);
  return mirror.duration > 0 ? Math.min(raw, mirror.duration) : raw;
}

function followedTrack(mirror: PlaybackMirror, tracks: Track[], favoriteIds: ReadonlySet<string>): Track {
  const found = tracks.find((track) => track.contentId && track.contentId === mirror.contentId);
  if (found) return found;
  return {
    contentId: mirror.contentId,
    localFileId: null,
    title: mirror.title,
    artist: mirror.artist,
    album: mirror.album,
    trackNumber: null,
    durationMs: mirror.duration > 0 ? Math.round(mirror.duration * 1000) : null,
    folder: "",
    fileName: "",
    addedAt: "",
    favorite: favoriteIds.has(mirror.contentId),
    exposed: false,
    badge: "remote",
    accessStatus: "",
    metadataStatus: "",
    playbackStatus: "",
    reasonKey: null,
    video: false,
    coverUrl: null,
    absolutePath: null,
    holderIds: [mirror.deviceId],
    playable: false,
    dimmed: false,
  };
}
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
  const [mirror, setMirror] = useState<PlaybackMirror | null>(null);
  const [downloadFeed, setDownloadFeed] = useState<DownloadFeed | null>(null);
  const [search, setSearch] = useState("");

  const queueRef = useRef(queue);
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
    setAlerts((items) => [...items, { id, text }]);
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
    document.title = lang === "zh" ? "澹版浮" : "TunePeerly";
    if (i18n.language !== lang) void i18n.changeLanguage(lang);
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
      listen<Omit<PlaybackMirror, "at">>("playback-mirror", (event) => {
        if (disposed) return;
        const selected = String(bootRef.current?.settings.positionSyncDevice ?? "");
        const row = event.payload;
        if (!selected || row.deviceId !== selected || bootRef.current?.settings.incognito) return;
        setMirror({ ...row, at: performance.now() });
        if (engine.playing) engine.pause();
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

  useEffect(() => {
    const selected = String(settings.positionSyncDevice ?? "");
    if (!selected || settings.incognito) {
      setMirror(null);
      return;
    }
    if (engine.playing) engine.pause();
  }, [settings.positionSyncDevice, settings.incognito, engine]);

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
  }, [engine]);

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

  const publishPlayback = useCallback((playing?: boolean, track = engine.current, seconds = engine.position) => {
    if (!track?.contentId || bootRef.current?.settings.incognito) return;
    // While this computer is following someone, its own queue must not be published as what it is playing.
    if (String(bootRef.current?.settings.positionSyncDevice ?? "")) return;
    void api.network("position", {
      contentId: track.contentId,
      seconds,
      playing: playing ?? engine.playing,
      duration: (track.durationMs ?? 0) / 1000,
      title: track.title,
      artist: track.artist,
      album: track.album,
    }).catch(() => {});
  }, [engine]);

  useEffect(() => {
    const timer = window.setInterval(() => publishPlayback(), 1000);
    return () => window.clearInterval(timer);
  }, [publishPlayback]);

  const startCurrent = useCallback((state: QueueState) => {
    const track = indexRef.current.get(state.current);
    setFailure(null);
    if (!track) {
      engine.cue(null, 0);
      setLyrics(null);
      return;
    }
    if (String(bootRef.current?.settings.positionSyncDevice ?? "") && !bootRef.current?.settings.incognito) {
      engine.cue(track, state.positionSec);
      loadLyrics(track);
      return;
    }
    void engine.start(track, state.positionSec, true);
    loadLyrics(track);
  }, [engine, loadLyrics]);

  const playList = useCallback((list: Track[], at: number, contextId: string, shuffle = false) => {
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

  const playAll = useCallback((list: Track[], contextId: string, shuffle = false) => {
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
    playList(list, list.indexOf(playable[0]), contextId, false);
  }, [commit, playList, setNotice, startCurrent, t]);

  const shuffleLibrary = useCallback(() => {
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

  const next = useCallback(() => {
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

  const previous = useCallback(() => {
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

  const toggle = useCallback(() => {
    if (String(bootRef.current?.settings.positionSyncDevice ?? "") && !bootRef.current?.settings.incognito) return;
    const state = queueRef.current;
    if (!state.current) {
      if (state.future.length > 0) next();
      else playAll(library, "library");
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
  }, [engine, library, next, playAll]);

  const jump = useCallback((at: number, expected?: Track | null) => {
    const index = expected ? queuedIndex(queueRef.current.future, at, refOf(expected).key) : at;
    if (index < 0) return;
    const target = jumpTo(queueRef.current, index);
    commit(target, 0);
    startCurrent(target);
  }, [commit, startCurrent]);

  const queueActions = useMemo(() => ({
    playNext: (items: Track[]) => {
      commit(playNext(queueRef.current, items.map(refOf)));
      toast(items.length === 1 ? t("toast.playNext") : t("toast.playNextMany", { count: items.length }));
    },
    add: (items: Track[]) => {
      commit(addToQueue(queueRef.current, items.map(refOf)));
      toast(items.length === 1 ? t("toast.queued") : t("toast.queuedMany", { count: items.length }));
    },
    insert: (at: number, items: Track[]) => {
      const state = queueRef.current;
      const position = Math.max(0, Math.min(at, state.future.length));
      commit({ ...state, future: [...state.future.slice(0, position), ...items.map(refOf), ...state.future.slice(position)] });
      toast(items.length === 1 ? t("toast.queued") : t("toast.queuedMany", { count: items.length }));
    },
    remove: (at: number, expected?: Track | null) => {
      const index = expected ? queuedIndex(queueRef.current.future, at, refOf(expected).key) : at;
      if (index < 0) return;
      commit(removeAt(queueRef.current, index));
    },
    move: (from: number, to: number, expected?: Track | null) => {
      const index = expected ? queuedIndex(queueRef.current.future, from, refOf(expected).key) : from;
      if (index < 0) return;
      commit(moveInQueue(queueRef.current, index, to));
    },
    clear: () => commit({ ...queueRef.current, future: [] }),
    jump,
  }), [commit, jump, t, toast]);

  const toggleShuffleMode = useCallback(() => {
    const state = queueRef.current;
    const pool = context.current.id === state.contextId && context.current.tracks ? context.current.tracks : contextTracks(state.contextId);
    const next = toggleShuffle(state, pool.filter((track) => track.playable).map(refOf), Math.random);
    commit(next, state.current ? engine.position : 0);
    if (!state.current && next.current) {
        startCurrent(next);
    }
  }, [commit, contextTracks, engine, startCurrent]);

  const cycleRepeatMode = useCallback(() => {
    commit({ ...queueRef.current, repeat: cycleRepeat(queueRef.current.repeat) });
  }, [commit]);

  const setVolume = useCallback((value: number) => {
    const level = Math.min(1, Math.max(0, value));
    setVolumeState(level);
    setMuted(false);
    engine.setVolume(level, false);
    window.clearTimeout(volumeTimer.current);
    volumeTimer.current = window.setTimeout(() => void api.updateSetting("volume", level).catch(() => {}), 400);
  }, [engine]);

  const toggleMute = useCallback(() => {
    setMuted((value) => {
      engine.setVolume(volume, !value);
      return !value;
    });
  }, [engine, volume]);

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
    if (key === "incognito" && value === true && engine.remoteDevice) {
      engine.halt();
      setNotice(t("notice.incognitoStopped"));
    }
    try {
      await api.updateSetting(key, value);
      if (key === "incognito") void api.network("close-route").catch(() => {});
      if (key === "showUnavailable" || key === "historyCap" || key === "incognito") scheduleRefresh();
    } catch (error) {
      setBoot(before);
      applyLook(before.theme, before.language);
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
      const paths = kind === "directory" ? [await pickFolder()].filter((item): item is string => Boolean(item)) : await pickFiles();
      await importPaths(paths, kind);
    } catch (error) {
      say(error);
    }
  }, [importPaths, say]);

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
    const [item] = list.splice(from, 1);
    if (!item) return;
    list.splice(to, 0, item);
    const nextMembers = { ...membersRef.current, [playlistId]: list };
    membersRef.current = nextMembers;
    setMembers(nextMembers);
    try {
      await api.movePlaylistMember(playlistId, from, to);
      applyPlayingOrder(`playlist:${playlistId}`, indexRef.current.members(list));
    } catch (error) {
      say(error);
    }
    scheduleRefresh();
  }, [applyPlayingOrder, say, scheduleRefresh]);

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
    paused: (track, position) => {
      void api.saveProgress(position).catch(() => {});
      publishPlayback(false, track, position);
    },
    seeked: (track, position, paused) => {
      void api.saveProgress(position).catch(() => {});
      publishPlayback(!paused, track, position);
    },
    progress: (position) => void api.saveProgress(position).catch(() => {}),
    activity: (playing, track, bufferEnough) => {
      if (playing && String(bootRef.current?.settings.positionSyncDevice ?? "") && !bootRef.current?.settings.incognito) {
        engine.pause();
        return;
      }
      void api.network("activity", { playing, localFileId: track?.localFileId ?? null, bufferEnough }).catch(() => {});
      if (track?.contentId) publishPlayback(playing, track, engine.position);
    },
    playable: (track) => {
      if (track.playbackStatus !== "playable") void api.playbackResult(track.localFileId, track.contentId, "playable", null).catch(() => {});
    },
  };

  useEffect(() => {
    const flush = () => {
      void api.saveProgress(engine.position).catch(() => {});
      publishPlayback(false);
    };
    window.addEventListener("beforeunload", flush);
    return () => window.removeEventListener("beforeunload", flush);
  }, [engine, publishPlayback]);

  const followId = String(settings.positionSyncDevice ?? "");
  const following = Boolean(followId && mirror?.deviceId === followId && !settings.incognito);
  const favoriteIds = useMemo(() => new Set(boot?.favoriteIds ?? []), [boot?.favoriteIds]);
  const shown = useMemo(
    () => (following && mirror ? followedTrack(mirror, tracks, favoriteIds) : current ?? null),
    [following, mirror, tracks, current, favoriteIds],
  );

  useEffect(() => {
    loadLyrics(shown ?? undefined);
  }, [shown, loadLyrics, lyricsEpoch]);

  const findTracks = useCallback((text: string, scope: "all" | "queue" | string) => {
    if (scope === "queue") {
      const state = queueRef.current;
      return filterText([state.current, ...state.future].map((item) => indexRef.current.get(item)).filter((item): item is Track => Boolean(item)), text);
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
    queue,
    current,
    shown,
    following,
    engine,
    volume,
    muted,
    lyrics,
    failure,
    mirror,
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
    next,
    previous,
    queueActions,
    toggleShuffle: toggleShuffleMode,
    cycleRepeat: cycleRepeatMode,
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
