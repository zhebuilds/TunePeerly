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
  exposed: boolean;
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

export type Grant = { id: number; kind: "directory" | "file"; path: string; exposure: Exposure };
export type Exposure = "follow" | "expose" | "hide";
export type Playlist = { id: string; name: string; exposure: Exposure; count: number };
export type Nearby = { deviceId: string; name: string; host: string; hosts: string[]; port: number };
export type LinkState = "online" | "pending" | "offline" | "failed";
export type Link = { deviceId: string; name: string; state: LinkState; path: string; detail: string; trust?: "full" | "verify" | string };
export type OverwriteAsk = { id: number; deviceId: string; name: string; feature: string; remaining: number };
export type Offer = { deviceId: string; name: string; safetyNumber: string; role?: "incoming" | "outgoing" };
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
export type Pending = { feature: Feature; source: string | null };
export type Feature = "favorites" | "appearance" | "history";
export type BlockFlags = { manual?: boolean; source?: boolean; all?: boolean };

export type Settings = {
  showUnavailable?: boolean;
  historyCap?: number;
  overwriteMode?: "silent" | "ask";
  historySync?: boolean;
  historyStartup?: boolean;
  historyPaused?: boolean;
  incognito?: boolean;
  syncSources?: Partial<Record<Feature, string>>;
  syncBlocks?: Partial<Record<Feature | "global", BlockFlags>>;
  exposeDefault?: "expose" | "hide";
  downloadFolder?: string;
  volume?: number;
  relayHost?: string;
  relayPort?: number;
  relayDeviceId?: string;
  relayToken?: string;
  discovery?: boolean;
  autoSend?: Partial<Record<Feature, boolean>>;
  syncRequestDevice?: string;
  displayName?: string;
  listenPort?: number;
  silenceSeconds?: number;
  conflictPolicy?: "ask" | "keep" | "take";
  noticeRead?: "open" | "click" | "visible";
  noticeClear?: "ask" | "unpinned" | "all";
  dirty?: Partial<Record<Feature, boolean>>;
  [key: string]: unknown;
};

export type Bootstrap = {
  deviceId: string;
  certId: string;
  deviceName: string;
  pairingText: string;
  settings: Settings;
  theme: string;
  language: string;
  grants: Grant[];
  playlists: Playlist[];
  player: Partial<QueueState> & Record<string, unknown>;
  history: string[];
  pending: Pending[];
  tracks: Track[];
  favoriteIds?: string[];
  listenPort: number;
  portFallback?: number | null;
  localAddresses?: string[];
  nearby: Nearby[];
  links: Link[];
  relay: string;
  offer: Offer | null;
  download: Download | null;
  downloadQueue?: DownloadQueueItem[];
  downloadHistory?: DownloadHistoryItem[];
  manual: string | null;
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
  mobilePickFolder: () => invoke("mobile_pick_folder"),
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
  setExposure: (scope: "grant" | "playlist" | "global", id: string, value: string) => invoke("set_exposure", { scope, id, value }),
  exposeMembers: (scope: "grant" | "playlist" | "library", id: string, expose: boolean) => invoke("expose_members", { scope, id, expose }),
  setTrackExposure: (localFileId: string, expose: boolean) => invoke("set_track_exposure", { localFileId, expose }),
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
};

export async function pickFolder(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}

/** Android hands out `content://` trees instead of paths, so the picker copies them in first. */
export function isAndroidApp(): boolean {
  return /Android/i.test(navigator.userAgent);
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
