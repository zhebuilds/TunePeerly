// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { QueueState } from "./player/advance";

export type Badge = "local" | "remote" | "unavailable";

export type Track = {
  contentId: string | null;
  localFileId: string | null;
  title: string;
  artist: string;
  album: string;
  trackNumber: number | null;
  durationMs: number | null;
  folder: string;
  fileName: string;
  addedAt: string;
  favorite: boolean;
  badge: Badge;
  accessStatus: string;
  metadataStatus: string;
  playbackStatus: string;
  reasonKey: string | null;
  video: boolean;
  coverUrl: string | null;
  absolutePath: string | null;
  holderIds: string[];
  playable: boolean;
  dimmed: boolean;
  snippet?: string;
};

export type Grant = { id: number; kind: "directory" | "file"; path: string };
export type Playlist = { id: string; name: string; count: number };
export type Nearby = { deviceId: string; name: string; host: string; hosts: string[]; port: number };
export type LinkState = "online" | "pending" | "offline" | "failed";
export type Link = { deviceId: string; name: string; state: LinkState; path: string; detail: string; trust?: "full" | "verify" | string };
export type OverwriteAsk = { id: number; deviceId: string; name: string; feature: string; remaining: number };
/** A pairing in progress. "incoming" shows a code, "outgoing" types the code, "approve" lets a saved Ask-each-time device in. */
export type Offer = {
  deviceId: string;
  name: string;
  role?: "incoming" | "outgoing" | "approve";
  code?: string | null;
  expiresAt?: number | null;
  status?: string;
  triesLeft?: number;
  lockedUntil?: number | null;
  trust?: string;
};
export type BlockedDevice = { deviceId: string; name: string; blockedAt: string };
export type Notice = { id: number; at: string; kind: string; deviceId: string; name: string; detail: string; read?: boolean; pinned?: boolean };
export type Download = {
  seq?: number;
  contentId: string;
  state: "started" | "saved" | "failed" | "need-folder" | "busy";
  detail: string;
  title?: string;
  received?: number;
  total?: number;
};
export type DownloadQueueItem = { contentId: string; title: string };
export type DownloadHistoryItem = { contentId: string; title: string; path: string; ok: boolean; at: string };
export type DownloadFeed = { current: Download | null; queue: DownloadQueueItem[]; history: DownloadHistoryItem[] };
export type Feature = "favorites" | "appearance";
export type OverwritePolicy = "silent" | "ask" | "deny";
/** What one device did with a Send from this computer. `noResponse` and `offline` are decided here. */
export type SyncResult = {
  deviceId: string;
  name: string;
  feature: Feature;
  status: "applied" | "pending" | "rejected" | "declined" | "timeout" | "noResponse" | "offline";
  reason?: "disabled" | "busy" | "incognito" | null;
};
/** Another device's Recently Played, shown here and never stored. */
export type RemoteHistory = { deviceId: string; entries?: { contentId: string; at: string }[]; error?: "offline" | "timeout" | "denied" };

export type Settings = {
  showUnavailable?: boolean;
  historyCap?: number;
  /** What this computer does when another device presses Send, per feature. */
  overwritePolicy?: Partial<Record<Feature, OverwritePolicy>>;
  historyPaused?: boolean;
  incognito?: boolean;
  downloadFolder?: string;
  volume?: number;
  relayHost?: string;
  relayPort?: number;
  relayDeviceId?: string;
  relayToken?: string;
  discovery?: boolean;
  displayName?: string;
  silenceSeconds?: number;
  noticeRead?: "open" | "click" | "visible";
  noticeClear?: "ask" | "unpinned" | "all";
  [key: string]: unknown;
};

export type Bootstrap = {
  deviceId: string;
  certId: string;
  deviceName: string;
  settings: Settings;
  theme: string;
  language: string;
  grants: Grant[];
  playlists: Playlist[];
  player: Partial<QueueState> & Record<string, unknown>;
  history: string[];
  tracks: Track[];
  favoriteIds?: string[];
  /** The LAN port the system picked for this run; 0 while not listening. */
  listenPort: number;
  localAddresses?: string[];
  /** Read-only network facts from the backend; `discovery` is false while it is not announcing. */
  network?: { tcpPort: number; discovery: boolean; udpPort: number; multicastGroup: string; addresses: string[] };
  nearby: Nearby[];
  links: Link[];
  relay: string;
  offer: Offer | null;
  download: Download | null;
  downloadQueue?: DownloadQueueItem[];
  downloadHistory?: DownloadHistoryItem[];
  failures?: Record<string, string>;
  blocks?: BlockedDevice[];
  notices?: Notice[];
};

export type ImportSummary = { kind: string; path: string; grantId: number; added: number; updated: number; missing: number };
export type ContentMove = { localFileId: string; previousContentId: string; nextContentId: string; title: string };
export type MemberRef = { contentId: string | null; localFileId: string | null };

export type DesktopTrack = {
  title: string;
  artist: string;
  album: string;
  coverPath: string | null;
  playing: boolean;
  favorite: boolean;
  duration: number;
  position: number;
  theme: string;
  language: string;
  tips: { previous: string; play: string; pause: string; next: string; like: string };
};

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  importPaths: (paths: string[], kind: "directory" | "file" | "auto") => invoke<ImportSummary[]>("import_paths", { paths, kind }),
  rescan: () => invoke("rescan"),
  removeGrant: (id: number) => invoke("remove_grant", { id }),
  library: () => invoke<Track[]>("library"),
  search: (query: string) => invoke<Track[]>("search", { query }),
  lyrics: (localFileId: string) => invoke<string>("lyrics", { localFileId }),
  savePlayer: (stateJson: QueueState) => invoke("save_player", { stateJson }),
  saveProgress: (positionSec: number) => invoke("save_progress", { positionSec }),
  recordHistory: (contentId: string | null, localFileId: string | null) => invoke("record_history", { contentId, localFileId }),
  playbackResult: (localFileId: string | null, contentId: string | null, status: string, reason: string | null) =>
    invoke("playback_result", { localFileId, contentId, status, reason }),
  remoteFailure: (contentId: string, deviceId: string, kind: string, reason: string) =>
    invoke("remote_failure", { contentId, deviceId, class: kind, reason }),
  toggleFavorite: (contentId: string | null, localFileId: string | null) => invoke<boolean>("toggle_favorite", { contentId, localFileId }),
  createPlaylist: (name: string) => invoke<string>("create_playlist", { name }),
  renamePlaylist: (id: string, name: string) => invoke("rename_playlist", { id, name }),
  deletePlaylist: (id: string) => invoke("delete_playlist", { id }),
  addToPlaylist: (playlistId: string, contentId: string | null, localFileId: string | null) =>
    invoke("add_to_playlist", { playlistId, contentId, localFileId }),
  removePlaylistMember: (playlistId: string, contentId: string | null, localFileId: string | null) =>
    invoke("remove_playlist_member", { playlistId, contentId, localFileId }),
  movePlaylistMember: (playlistId: string, from: number, to: number) => invoke("move_playlist_member", { playlistId, from, to }),
  setLibraryOrder: (keys: string[]) => invoke("set_library_order", { keys }),
  playlistMembers: (id: string) => invoke<MemberRef[]>("playlist_members", { id }),
  updateSetting: (key: string, value: unknown) => invoke("update_setting", { key, value }),
  network: (action: string, payload: unknown = null) => invoke("network_command", { action, payload }),
  remoteUrl: (contentId: string, exclude: string[]) => invoke<{ url: string; deviceId: string }>("remote_url", { contentId, exclude }),
  moveMembership: (previous: string, next: string, accept: boolean) => invoke("move_membership", { previous, next, accept }),
  showInFolder: (localFileId: string) => invoke("show_in_folder", { localFileId }),
  revealPath: (path: string) => invoke("reveal_path", { path }),
  desktopUpdate: (track: DesktopTrack) => invoke("desktop_update", { track }),
  desktopState: () => invoke<DesktopTrack>("desktop_state"),
  desktopCommand: (action: string) => invoke("desktop_command", { action }),
  quitApp: () => invoke("quit_app"),
  hideToTray: () => invoke("hide_to_tray"),
  setTrayMenu: (show: string, quit: string, name: string) => invoke("set_tray_menu", { show, quit, name }),
  fileUrl: (path: string) => convertFileSrc(path),
  /** Android: the system picker copies the chosen songs or folder into the app, which then imports them. */
  androidImport: (folder: boolean) => invoke<ImportSummary[]>("android_import", { folder }),
};

export async function pickFolder(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}

export async function pickFiles(): Promise<string[]> {
  const selected = await open({
    directory: false,
    multiple: true,
    filters: [{ name: "Audio", extensions: ["flac", "mp3", "wav", "ogg", "opus", "m4a", "aac", "aiff", "aif", "alac", "wma", "ape", "wv", "wave", "oga", "m4b", "mp4", "dsf", "dff", "caf", "tta", "spx", "audio"] }],
  });
  if (typeof selected === "string") return [selected];
  return selected ?? [];
}

/** Backend errors arrive as plain strings; some are i18n keys. */
export function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object" && "message" in error) return String((error as { message: unknown }).message);
  return String(error);
}
