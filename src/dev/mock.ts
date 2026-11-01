// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
/* Browser-only stand-in for the Tauri backend, used by `npm run dev` outside the app window. */
import type { Bootstrap, Link, MemberRef, Playlist, Track } from "../api";
import { version as appVersion } from "../../package.json";

type Callback = (payload: unknown) => void;
const params = new URLSearchParams(location.search);
const callbacks = new Map<number, Callback>();
const listeners = new Map<string, Set<number>>();
let nextId = 1;

const artists = ["Aurora Field", "Kite Harbor", "Mina Sol", "The Paper Lanterns", "Ochre Bloom", "陈雨", "North Window"];
const albums: Record<string, string[]> = {
  "Aurora Field": ["Lowlight", "Summer Static"],
  "Kite Harbor": ["Tidal Notes"],
  "Mina Sol": ["Velvet Hours", "Small Planets"],
  "The Paper Lanterns": ["Fold & Glow"],
  "Ochre Bloom": ["Clay Season"],
  陈雨: ["城市夜行"],
  "North Window": ["Frost Lines"],
};
const words = ["Morning", "Paper", "Echo", "Amber", "Harbor", "Glass", "River", "Velvet", "Static", "Lantern", "Signal", "Orchard", "Halo", "Drift", "Copper", "Meadow", "Neon", "Quiet", "Ember", "Satellite"];
const tails = ["Light", "Song", "Hours", "Lines", "Room", "Road", "Waves", "Dream", "Bloom", "Air", "Fever", "Garden"];
const palette = [
  ["#f4a261", "#e76f51"],
  ["#2a9d8f", "#264653"],
  ["#e9c46a", "#f4a261"],
  ["#8ab17d", "#2a9d8f"],
  ["#b56576", "#6d597a"],
  ["#457b9d", "#1d3557"],
  ["#ffb4a2", "#e5989b"],
];

function coverFor(seed: number, text: string): string {
  const [a, b] = palette[seed % palette.length];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 300"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient></defs><rect width="300" height="300" fill="url(#g)"/><circle cx="${60 + (seed * 37) % 180}" cy="${70 + (seed * 53) % 160}" r="${50 + (seed % 5) * 14}" fill="#ffffff22"/><text x="24" y="270" font-family="Segoe UI, sans-serif" font-size="34" font-weight="800" fill="#ffffffdd">${text.replace(/&/g, "&amp;")}</text></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

const empty = params.has("empty");
const tracks: Track[] = [];
if (!empty) {
  let n = 0;
  for (const artist of artists) {
    for (const album of albums[artist]) {
      const count = 4 + ((n * 7) % 5);
      const albumSeed = n;
      for (let i = 0; i < count; i += 1) {
        n += 1;
        const title = artist === "陈雨" ? ["夜行", "雨后的街", "霓虹", "慢慢", "回声", "灯塔", "远方", "旧照片"][i % 8] : `${words[(n * 3) % words.length]} ${tails[(n * 5) % tails.length]}`;
        const local = n % 9 !== 0;
        const id = `c${String(n).padStart(3, "0")}`;
        const folder = `D:\\Music\\${artist}\\${album}`;
        const missing = n === 14;
        const failed = n === 22;
        tracks.push({
          contentId: n % 11 === 0 && local ? null : id,
          localFileId: local ? `l${n}` : null,
          title,
          artist,
          album,
          trackNumber: i + 1,
          durationMs: (18 + ((n * 13) % 26)) * 1000,
          folder: local ? folder : "",
          fileName: `${String(i + 1).padStart(2, "0")} ${title}.flac`,
          addedAt: new Date(Date.now() - n * 36e5 * 7).toISOString(),
          favorite: n % 4 === 0,
          badge: !local ? (n % 18 === 0 ? "unavailable" : "remote") : missing ? "unavailable" : "local",
          accessStatus: local ? (missing ? "missing" : "available") : "missing",
          metadataStatus: "ready",
          playbackStatus: failed ? "failed" : "untried",
          reasonKey: missing ? "file.missing" : failed ? "playback.decodeFailed" : !local && n % 18 === 0 ? "notice.offlineTrack" : null,
          video: false,
          coverUrl: n % 10 === 3 ? null : coverFor(albumSeed, album),
          absolutePath: local ? `${folder}\\${String(i + 1).padStart(2, "0")} ${title}.flac` : null,
          holderIds: !local && n % 18 !== 0 ? ["dev-studio"] : n % 5 === 0 ? ["dev-studio"] : [],
          playable: local ? !missing : n % 18 !== 0,
          dimmed: missing || (!local && n % 18 === 0),
        });
      }
    }
  }
}

const key = (track: Track) => track.contentId ?? track.localFileId ?? "";
const playlists: Playlist[] = empty
  ? []
  : [
      { id: "p1", name: "Late Night Drive", count: 0 },
      { id: "p2", name: "Focus", count: 0 },
      { id: "p3", name: "周末早晨", count: 0 },
    ];
const members: Record<string, MemberRef[]> = {
  p1: tracks.filter((_, at) => at % 3 === 0).slice(0, 14).map((track) => ({ contentId: track.contentId, localFileId: track.localFileId })),
  p2: tracks.filter((_, at) => at % 5 === 1).slice(0, 9).map((track) => ({ contentId: track.contentId, localFileId: track.localFileId })),
  p3: [],
};
const sync = () => playlists.forEach((playlist) => (playlist.count = members[playlist.id]?.length ?? 0));
sync();

const links: Link[] = empty
  ? []
  : [
      { deviceId: "dev-studio", name: "Studio PC", state: "online", path: "lan", detail: "outgoing", trust: "full" },
      { deviceId: "dev-laptop", name: "Travel Laptop", state: "offline", path: "", detail: "", trust: "verify" },
    ];

const lrc = `[ti:Morning Light]
[00:00.50]Wake up, the window's open
[00:03.20]Paper planes across the floor
[00:06.00]Every sound is soft and golden
[00:09.10]Like we've heard it all before
[00:12.40]Hold the light a little longer
[00:15.30]Let the kettle sing its tune
[00:18.20]We'll be fine, we're getting stronger
[00:21.00]Morning light, and afternoon
[00:24.50]♪
[00:27.00]Wake up, the window's open`;

const settings: Record<string, unknown> = {
  showUnavailable: true,
  historyCap: 100,
  incognito: false,
  downloadFolder: "",
  volume: 0.8,
  relayHost: "",
  relayPort: 22067,
  relayDeviceId: "",
  relayToken: "",
  discovery: true,
  displayName: "Desk PC",
};

const state = {
  theme: params.has("light") ? "light" : "dark",
  language: params.get("lang") === "zh" ? "zh" : "en",
  player: {} as Record<string, unknown>,
  history: tracks.slice(0, 9).filter((track) => track.playable).map(key),
  offer: params.has("offer")
    ? { deviceId: "dev-new", name: "Living Room", role: (params.get("offer") === "enter" ? "outgoing" : "incoming") as "incoming" | "outgoing", code: params.get("offer") === "enter" ? null : "042137", expiresAt: Date.now() + 90_000, status: "", triesLeft: 3, lockedUntil: null, trust: "full" }
    : (null as Bootstrap["offer"]),
  blocks: [] as Bootstrap["blocks"],
  notices: [
    { id: 2, at: "2026-10-18T14:02:00.000Z", kind: "connected", deviceId: "dev-studio", name: "Studio PC", detail: "lan", read: false, pinned: false },
    { id: 1, at: "2026-10-18T13:40:00.000Z", kind: "paired", deviceId: "dev-laptop", name: "Travel Laptop", detail: "full", read: false, pinned: true },
  ] as NonNullable<Bootstrap["notices"]>,
  failures: {} as Bootstrap["failures"],
  download: null as Bootstrap["download"],
};

function emit(event: string, payload: unknown = {}) {
  for (const id of listeners.get(event) ?? []) callbacks.get(id)?.({ event, id, payload });
}

const audio = new Map<string, string>();
function toneUrl(seed: string, seconds: number): string {
  const cached = audio.get(seed);
  if (cached) return cached;
  const rate = 11025;
  const length = Math.floor(rate * seconds);
  const buffer = new ArrayBuffer(44 + length * 2);
  const view = new DataView(buffer);
  const write = (offset: number, text: string) => [...text].forEach((char, at) => view.setUint8(offset + at, char.charCodeAt(0)));
  write(0, "RIFF");
  view.setUint32(4, 36 + length * 2, true);
  write(8, "WAVE");
  write(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, rate, true);
  view.setUint32(28, rate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  write(36, "data");
  view.setUint32(40, length * 2, true);
  let hash = 0;
  for (const char of seed) hash = (hash * 31 + char.charCodeAt(0)) | 0;
  const scale = [0, 3, 5, 7, 10, 12, 15];
  const base = 196 * 2 ** ((Math.abs(hash) % 7) / 12);
  for (let i = 0; i < length; i += 1) {
    const t = i / rate;
    const step = Math.floor(t * 2.5);
    const note = base * 2 ** (scale[(step * 3 + Math.abs(hash)) % scale.length] / 12);
    const local = t * 2.5 - step;
    const envelope = Math.min(1, local * 20) * Math.exp(-local * 3);
    const sample = (Math.sin(2 * Math.PI * note * t) * 0.6 + Math.sin(2 * Math.PI * note * 2 * t) * 0.15) * envelope * 0.35;
    view.setInt16(44 + i * 2, Math.max(-1, Math.min(1, sample)) * 32767, true);
  }
  const url = URL.createObjectURL(new Blob([buffer], { type: "audio/wav" }));
  audio.set(seed, url);
  return url;
}

let libraryOrder: string[] | null = null;

function orderedLibrary(): Track[] {
  const copy = structuredClone(tracks);
  if (!libraryOrder) return copy;
  const rank = new Map(libraryOrder.map((key, index) => [key, index]));
  const keyOf = (track: Track) => track.contentId ?? track.localFileId ?? "";
  return copy.sort((left, right) => {
    const a = rank.get(keyOf(left));
    const b = rank.get(keyOf(right));
    if (a === undefined && b === undefined) return 0;
    if (a === undefined) return 1;
    if (b === undefined) return -1;
    return a - b;
  });
}

function bootstrap(): Bootstrap {
  sync();
  return {
    deviceId: "dev-self",
    certId: "MFZWI3D-BONSGYC-YLTMRWG-C43ENR5-QXGZDMM-FZWI3DP-BONSGYY-LTMRWAD",
    deviceName: String(settings.displayName),
    settings: structuredClone(settings),
    theme: state.theme,
    language: state.language,
    grants: empty ? [] : [{ id: 1, kind: "directory", path: "D:\\Music" }, { id: 2, kind: "file", path: "C:\\Users\\me\\Downloads\\single.mp3" }],
    playlists: structuredClone(playlists),
    player: structuredClone(state.player),
    history: [...state.history],
    tracks: orderedLibrary(),
    favoriteIds: orderedLibrary().map((track) => track.contentId).filter((id): id is string => Boolean(id && tracks.find((item) => item.contentId === id)?.favorite)),
    listenPort: 49731,
    localAddresses: ["192.168.1.20", "10.0.0.8"],
    network: { tcpPort: 49731, discovery: true, udpPort: 41220, multicastGroup: "224.0.0.168", addresses: ["192.168.1.20", "10.0.0.8"] },
    nearby: empty ? [] : [{ deviceId: "dev-living", name: "Living Room", host: "192.168.1.31", hosts: ["192.168.1.31", "10.0.0.31"], port: 22000 }],
    links: structuredClone(links),
    relay: "",
    downloadQueue: [],
    downloadHistory: [],
    offer: state.offer,
    download: state.download,
    failures: state.failures,
    blocks: state.blocks,
    notices: state.notices,
  };
}

function find(contentId: string | null, localFileId: string | null) {
  return tracks.find((track) => (contentId && track.contentId === contentId) || (localFileId && track.localFileId === localFileId));
}

async function invoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  await new Promise((resolve) => setTimeout(resolve, 8));
  switch (cmd) {
    case "plugin:event|listen": {
      const set = listeners.get(String(args.event)) ?? new Set();
      set.add(Number(args.handler));
      listeners.set(String(args.event), set);
      return Number(args.handler);
    }
    case "plugin:event|unlisten":
      return null;
    case "plugin:dialog|open":
      return null;
    case "plugin:opener|open_url":
      window.open(String(args.url), "_blank", "noopener");
      return null;
    case "plugin:app|version":
      return appVersion;
    case "bootstrap":
      return bootstrap();
    case "library":
      return orderedLibrary();
    case "set_library_order":
      libraryOrder = (args.keys as string[]) ?? [];
      return null;
    case "search": {
      const query = String(args.query).toLowerCase();
      const lyricLine = lrc
        .split("\n")
        .map((line) => line.replace(/^\[[^\]]+\]/, ""))
        .find((line) => line.toLowerCase().includes(query));
      return tracks
        .filter((track, index) => {
          const plain = `${track.title} ${track.artist} ${track.album}`.toLowerCase();
          return plain.includes(query) || (index % 3 === 0 && Boolean(lyricLine));
        })
        .map((track) => {
          const plain = `${track.title} ${track.artist} ${track.album}`.toLowerCase();
          if (plain.includes(query) || !lyricLine) return track;
          const hit = lyricLine.replace(new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "ig"), (match) => `\u0002${match}\u0003`);
          return { ...track, snippet: hit };
        });
    }
    case "lyrics": {
      const track = tracks.find((item) => item.localFileId === args.localFileId);
      return track && tracks.indexOf(track) % 3 === 0 ? lrc : "";
    }
    case "save_player":
      state.player = args.stateJson as Record<string, unknown>;
      return null;
    case "save_progress":
      state.player = { ...state.player, positionSec: args.positionSec };
      return null;
    case "record_history": {
      const id = String(args.contentId ?? args.localFileId);
      state.history = [id, ...state.history.filter((item) => item !== id)];
      return null;
    }
    case "toggle_favorite": {
      const track = find(args.contentId as string | null, args.localFileId as string | null);
      if (!track) throw "not found";
      track.favorite = !track.favorite;
      return track.favorite;
    }
    case "create_playlist": {
      const name = String(args.name).trim();
      if (!name) throw "playlist.nameEmpty";
      const id = `p${Date.now()}`;
      playlists.push({ id, name, count: 0 });
      members[id] = [];
      return id;
    }
    case "rename_playlist": {
      const playlist = playlists.find((item) => item.id === args.id);
      if (playlist) playlist.name = String(args.name);
      return null;
    }
    case "delete_playlist":
      playlists.splice(playlists.findIndex((item) => item.id === args.id), 1);
      return null;
    case "add_to_playlist": {
      const list = (members[String(args.playlistId)] ??= []);
      if (list.some((item) => (args.contentId && item.contentId === args.contentId) || (args.localFileId && item.localFileId === args.localFileId))) throw "playlist.already";
      list.push({ contentId: (args.contentId as string) ?? null, localFileId: (args.localFileId as string) ?? null });
      return null;
    }
    case "remove_playlist_member": {
      const id = String(args.playlistId);
      members[id] = (members[id] ?? []).filter((item) => !((args.contentId && item.contentId === args.contentId) || (args.localFileId && item.localFileId === args.localFileId)));
      return null;
    }
    case "move_playlist_member": {
      const list = members[String(args.playlistId)] ?? [];
      const [item] = list.splice(Number(args.from), 1);
      if (item) list.splice(Number(args.to), 0, item);
      return null;
    }
    case "playlist_members":
      return structuredClone(members[String(args.id)] ?? []);
    case "update_setting":
      if (args.key === "theme") state.theme = String(args.value);
      else if (args.key === "language") state.language = String(args.value);
      else settings[String(args.key)] = args.value;
      return null;
    case "network_command": {
      const action = String(args.action);
      if (action === "reject") state.offer = null;
      if (action === "confirm" && state.offer) {
        // The mock pairs on 042137 and treats every other code as wrong.
        const code = (args.payload as { code?: string }).code;
        state.offer = state.offer.role === "outgoing" && code !== "042137" ? { ...state.offer, status: "wrong" } : null;
      }
      if (action === "notice" || action === "clear-notices") {
        const payload = (args.payload ?? {}) as { action?: string; id?: number; ids?: number[] };
        const op = action === "clear-notices" ? "clear-all" : payload.action;
        if (op === "read") state.notices = state.notices?.map((item) => (item.id === payload.id ? { ...item, read: true } : item));
        if (op === "read-ids") state.notices = state.notices?.map((item) => (payload.ids?.includes(item.id) ? { ...item, read: true } : item));
        if (op === "read-all") state.notices = state.notices?.map((item) => ({ ...item, read: true }));
        if (op === "unread") state.notices = state.notices?.map((item) => (item.id === payload.id ? { ...item, read: false } : item));
        if (op === "pin") state.notices = state.notices?.map((item) => (item.id === payload.id ? { ...item, pinned: true } : item));
        if (op === "unpin") state.notices = state.notices?.map((item) => (item.id === payload.id ? { ...item, pinned: false } : item));
        if (op === "delete") state.notices = state.notices?.filter((item) => item.id !== payload.id);
        if (op === "clear-unpinned") state.notices = state.notices?.filter((item) => item.pinned);
        if (op === "clear-all") state.notices = [];
      }
      if (action === "view-history") {
        const deviceId = String((args.payload as { deviceId?: string }).deviceId ?? "");
        const entries = tracks.slice(3, 12).filter((track) => track.contentId).map((track, index) => ({ contentId: String(track.contentId), at: new Date(Date.now() - index * 600_000).toISOString() }));
        setTimeout(() => emit("remote-history", { deviceId, entries }), 300);
        return null;
      }
      if (action === "download") {
        state.download = { seq: Date.now(), contentId: String(args.payload), state: "started", detail: "" };
        setTimeout(() => {
          state.download = { seq: Date.now(), contentId: String(args.payload), state: "need-folder", detail: "" };
          emit("network-updated");
        }, 900);
      }
      setTimeout(() => emit("network-updated"), 20);
      return null;
    }
    case "remote_url": {
      const track = tracks.find((item) => item.contentId === args.contentId);
      return { url: toneUrl(String(args.contentId), (track?.durationMs ?? 30000) / 1000), deviceId: "dev-studio" };
    }
    case "import_paths":
      return [{ kind: "directory", path: "x", grantId: 1, added: 0, updated: 0, missing: 0 }];
    case "rescan":
    case "remove_grant":
    case "playback_result":
    case "remote_failure":
    case "reveal_path":
    case "move_membership":
    case "show_in_folder":
    case "quit_app":
    case "hide_to_tray":
    case "set_tray_menu":
      return null;
    case "plugin:window|close":
      // The real shell intercepts the close request and asks the page what to do.
      emit("close-requested");
      return null;
    default:
      console.warn("mock: unhandled", cmd, args);
      return null;
  }
}

const durations = new Map(tracks.map((track) => [track.absolutePath ?? "", (track.durationMs ?? 30000) / 1000]));

Object.assign(window, {
  __TAURI_INTERNALS__: {
    invoke,
    transformCallback: (callback: Callback) => {
      const id = nextId++;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback: (id: number) => callbacks.delete(id),
    convertFileSrc: (path: string) => (path.startsWith("data:") ? path : toneUrl(path, durations.get(path) ?? 30)),
    metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
  },
  __TAURI_EVENT_PLUGIN_INTERNALS__: {
    unregisterListener: (event: string, id: number) => listeners.get(event)?.delete(id),
  },
});
