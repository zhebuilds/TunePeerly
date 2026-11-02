// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { api, type RemoteHistory, type Track } from "../api";
import { useApp, type Route } from "../app/model";
import { Cover, LikedCover } from "../components/Cover";
import { Select } from "../components/controls";
import { ActionBar, PageHeader } from "../components/PageHeader";
import { SyncButton } from "../components/SyncButton";
import { TrackTable, type Columns } from "../components/TrackTable";
import { tintFor, totalText } from "../lib/format";
import { albumParts, covers, filterText, folderName, sortTracks, trackKey, type Sort } from "../lib/library";
import { transport } from "../player/clock";
import { Icons } from "../ui/icons";

const sorts = new Map<string, Sort>();

type PageInfo = {
  kind: string;
  title: string;
  cover: ReactNode;
  tracks: Track[];
  contextId: string;
  defaultSort: Sort;
  columns?: Columns;
  playlistId?: string;
  reorderId?: string;
  empty: ReactNode;
  extraMeta?: ReactNode;
  shuffleAll?: boolean;
};

export function TrackListPage({ route }: { route: Route }) {
  const { t } = useTranslation();
  const app = useApp();
  const routeKey = `${route.name}:${route.id ?? ""}`;
  const [filter, setFilter] = useState("");
  const [filterOpen, setFilterOpen] = useState(false);
  const filterInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setFilter("");
    setFilterOpen(false);
  }, [routeKey]);

  const online = (app.boot?.links ?? []).filter((link) => link.state === "online" && link.deviceId !== app.boot?.deviceId);
  const [viewing, setViewing] = useState("");
  const viewed = route.name === "recent" ? viewing : "";
  const remote = useRemoteHistory(viewed, () => setViewing(""));
  const local = usePageInfo(route);
  const info: PageInfo = viewed
    ? {
        ...local,
        tracks: remote.tracks,
        contextId: `recent-device:${viewed}`,
        extraMeta: remote.missing > 0 ? <span className="muted">{t("recent.notHere", { count: remote.missing })}</span> : undefined,
        empty: remote.loading ? t("recent.loading") : <EmptyHint icon={<Icons.clock />} title={t("empty.recentTitle")} body={t("recent.remoteEmpty")} />,
      }
    : local;
  const [sort, setSortState] = useState<Sort>(() => sorts.get(routeKey) ?? info.defaultSort);
  useEffect(() => setSortState(sorts.get(routeKey) ?? info.defaultSort), [routeKey]);
  const setSort = (value: Sort) => {
    sorts.set(routeKey, value);
    setSortState(value);
  };

  const sortChoices = sortOptions(info.defaultSort, t, route.name === "recent");
  const shown = useMemo(() => sortTracks(filterText(info.tracks, filter), sort), [info.tracks, filter, sort]);
  const total = info.tracks.reduce((sum, track) => sum + (track.durationMs ?? 0), 0);
  const playlist = info.playlistId ? app.boot?.playlists.find((item) => item.id === info.playlistId) : undefined;
  const reorderId = info.reorderId ?? info.playlistId;
  const reorderable = Boolean(reorderId) && sort.key === "order" && sort.dir === 1 && !filter;

  return (
    <div className="page list-page">
      <PageHeader
        kind={info.kind}
        title={info.title}
        cover={info.cover}
        tint={route.name === "liked" ? "#d36c7c" : tintFor(info.title)}
        meta={
          <>
            {info.extraMeta}
            <span>{t("list.songCount", { count: info.tracks.length })}</span>
            {total > 0 && <span>{totalText(total, t)}</span>}
          </>
        }
      />
      <ActionBar>
        <PlayFab
          active={app.queue.contextId === info.contextId && Boolean(app.current)}
          disabled={info.tracks.length === 0}
          onPlay={() => app.playAll(shown, info.contextId)}
        />
        <button
          type="button"
          className="icon-btn lg"
          aria-label={info.shuffleAll ? t("list.shuffleLibrary") : t("list.shuffle")}
          title={info.shuffleAll ? t("list.shuffleLibrary") : t("list.shuffle")}
          disabled={info.tracks.length === 0}
          onClick={() => (info.shuffleAll ? app.shuffleLibrary() : app.playAll(shown, info.contextId, true))}
        >
          <Icons.shuffle />
        </button>
        {route.name === "library" && (
          <>
            <button type="button" className="btn ghost" title={t("library.addFolder")} aria-label={t("library.addFolder")} onClick={() => void app.addMusic("directory")}>
              <Icons.folder /> <span className="btn-label">{t("library.addFolder")}</span>
            </button>
            <button type="button" className="btn ghost" title={t("library.addFiles")} aria-label={t("library.addFiles")} onClick={() => void app.addMusic("file")}>
              <Icons.music /> <span className="btn-label">{t("library.addFiles")}</span>
            </button>
            <button type="button" className="btn ghost" title={t("library.scanHint")} aria-label={t("library.scanHint")} onClick={() => void app.rescan()}>
              <Icons.sync /> <span className="btn-label">{t("library.scan")}</span>
            </button>
          </>
        )}
        {route.name === "liked" && <SyncButton feature="favorites" />}
        {route.name === "recent" && (online.length > 0 || viewed) && (
          <span className={`recent-source${remote.loading ? " loading" : ""}`} title={t("recent.sourceHelp")}>
            <Select
              label={t("recent.source")}
              value={viewed}
              onChange={setViewing}
              options={[
                { value: "", label: t("recent.thisDevice") },
                ...online.map((link) => ({ value: link.deviceId, label: link.name || link.deviceId.slice(0, 7) })),
              ]}
            />
          </span>
        )}
        {playlist && (
          <>
            <button type="button" className="icon-btn lg" aria-label={t("playlist.rename")} title={t("playlist.rename")} onClick={() => app.renamePlaylist(playlist.id)}>
              <Icons.pencil />
            </button>
            <button type="button" className="icon-btn lg" aria-label={t("playlist.delete")} title={t("playlist.delete")} onClick={() => app.deletePlaylist(playlist.id)}>
              <Icons.trash />
            </button>
          </>
        )}
        <div className="action-spacer" />
        <div className={`list-filter${filterOpen || filter ? " open" : ""}`}>
          <button
            type="button"
            className="icon-btn"
            aria-label={t("list.filter")}
            onClick={() => {
              setFilterOpen(true);
              window.setTimeout(() => filterInput.current?.focus(), 0);
            }}
          >
            <Icons.filter />
          </button>
          <input
            ref={filterInput}
            className="input"
            value={filter}
            placeholder={t("list.filterPlaceholder")}
            aria-label={t("list.filter")}
            onChange={(event) => setFilter(event.target.value)}
            onBlur={() => {
              if (!filter) setFilterOpen(false);
            }}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                setFilter("");
                (event.target as HTMLInputElement).blur();
              }
            }}
          />
        </div>
        <Select
          label={t("list.sortBy")}
          value={sortChoices.find((option) => option.value.startsWith(`${sort.key}:`))?.value ?? sortChoices[0].value}
          onChange={(value) => {
            const [key, dir] = value.split(":");
            setSort({ key: key as Sort["key"], dir: Number(dir) === -1 ? -1 : 1 });
          }}
          options={sortChoices}
        />
        <button
          type="button"
          className="icon-btn sort-dir"
          aria-label={sort.dir === 1 ? t("list.sortAsc") : t("list.sortDesc")}
          title={sort.dir === 1 ? t("list.sortAsc") : t("list.sortDesc")}
          onClick={() => setSort({ key: sort.key, dir: sort.dir === 1 ? -1 : 1 })}
        >
          <Icons.chevronDown className={sort.dir === 1 ? "up" : ""} />
        </button>
      </ActionBar>
      <TrackTable
        tracks={shown}
        contextId={info.contextId}
        sort={sort}
        onSort={setSort}
        defaultSort={info.defaultSort}
        playlistId={info.playlistId}
        reorderId={reorderId}
        reorderable={reorderable}
        columns={info.columns}
        empty={filter ? t("list.noMatch", { text: filter }) : info.empty}
      />
    </div>
  );
}

export function PlayFab({ active, disabled, onPlay }: { active: boolean; disabled?: boolean; onPlay: () => void }) {
  const { t } = useTranslation();
  const app = useApp();
  const { playing } = useSyncExternalStore(transport.subscribe, transport.get);
  const pausing = active && playing;
  return (
    <button type="button" className="play-fab" aria-label={pausing ? t("player.pause") : t("list.play")} disabled={disabled} onClick={() => (active ? app.toggle() : onPlay())}>
      {pausing ? <Icons.pause /> : <Icons.play />}
    </button>
  );
}

function sortOptions(defaultSort: Sort, t: (key: string) => string, history: boolean) {
  const options = [
    { value: "title:1", label: t("sort.title") },
    { value: "artist:1", label: t("sort.artist") },
    { value: "album:1", label: t("sort.album") },
    { value: "folder:1", label: t("sort.folder") },
    { value: "addedAt:-1", label: t("sort.added") },
    { value: "favorite:-1", label: t("sort.liked") },
    { value: "duration:-1", label: t("sort.duration") },
  ];
  if (defaultSort.key === "order") options.unshift({ value: "order:1", label: history ? t("sort.recent") : t("sort.custom") });
  if (defaultSort.key === "trackNumber") options.unshift({ value: "trackNumber:1", label: t("sort.trackNumber") });
  return options;
}

/** Another connected device's Recently Played, asked for when chosen and shown read-only. Nothing is
 * stored or merged here. If that device goes offline, the page goes back to this computer's list. */
function useRemoteHistory(deviceId: string, back: () => void) {
  const { t } = useTranslation();
  const app = useApp();
  const [reply, setReply] = useState<RemoteHistory | null>(null);
  const link = app.boot?.links.find((item) => item.deviceId === deviceId);
  const name = link?.name || deviceId.slice(0, 7);
  const gone = Boolean(deviceId) && link?.state !== "online";
  const backRef = useRef(back);
  backRef.current = back;

  useEffect(() => {
    setReply(null);
    if (!deviceId) return;
    let live = true;
    const stop = listen<RemoteHistory>("remote-history", (event) => {
      if (live && event.payload.deviceId === deviceId) setReply(event.payload);
    });
    void stop.then(() => (live ? api.network("view-history", { deviceId }) : null)).catch(app.say);
    return () => {
      live = false;
      void stop.then((off) => off()).catch(() => {});
    };
  }, [deviceId]);

  useEffect(() => {
    if (!deviceId) return;
    const error = reply?.error;
    if (!gone && !error) return;
    app.toast(error === "denied" ? t("recent.denied", { name }) : error === "timeout" ? t("recent.noAnswer", { name }) : t("recent.offline", { name }));
    backRef.current();
  }, [deviceId, gone, reply?.error]);

  return useMemo(() => {
    const seen = new Set<string>();
    const tracks: Track[] = [];
    let missing = 0;
    for (const entry of reply?.entries ?? []) {
      const track = app.index.get({ contentId: entry.contentId });
      if (!track) {
        missing += 1;
        continue;
      }
      if (seen.has(trackKey(track))) continue;
      seen.add(trackKey(track));
      tracks.push(track);
    }
    return { tracks, missing, loading: Boolean(deviceId) && !reply };
  }, [reply, app.index, deviceId]);
}

function usePageInfo(route: Route): PageInfo {
  const { t } = useTranslation();
  const app = useApp();
  const id = route.id ?? "";
  return useMemo((): PageInfo => {
    const base = { defaultSort: { key: "title", dir: 1 } as Sort };
    switch (route.name) {
      case "liked":
        return {
          ...base,
          kind: t("kind.playlist"),
          title: t("nav.liked"),
          cover: covers(app.liked, 1, "liked")[0] ? <Cover src={covers(app.liked, 1, "liked")[0]} /> : <LikedCover />,
          tracks: app.liked,
          contextId: "liked",
          defaultSort: { key: "addedAt", dir: -1 },
          empty: <EmptyHint icon={<Icons.heart />} title={t("empty.likedTitle")} body={t("empty.likedBody")} />,
        };
      case "recent":
        return {
          ...base,
          kind: t("kind.history"),
          title: t("nav.recent"),
          cover: covers(app.recent, 1, "recent")[0] ? <Cover src={covers(app.recent, 1, "recent")[0]} /> : <Cover />,
          tracks: app.recent,
          contextId: "recent",
          defaultSort: { key: "order", dir: 1 },
          empty: <EmptyHint icon={<Icons.clock />} title={t("empty.recentTitle")} body={t("empty.recentBody")} />,
        };
      case "playlist": {
        const playlist = app.boot?.playlists.find((item) => item.id === id);
        const tracks = app.index.members(app.members[id] ?? []);
        return {
          ...base,
          kind: t("kind.playlist"),
          title: playlist?.name ?? "",
          cover: <Cover src={covers(tracks, 1, `playlist:${id}`)[0]} />,
          tracks,
          contextId: `playlist:${id}`,
          playlistId: id,
          defaultSort: { key: "order", dir: 1 },
          empty: <EmptyHint icon={<Icons.music />} title={t("empty.playlistTitle")} body={t("empty.playlistBody")} />,
        };
      }
      case "artist": {
        const tracks = app.library.filter((track) => track.artist === id);
        return { ...base, kind: t("kind.artist"), title: id || t("library.unknownArtist"), cover: <Cover src={covers(tracks, 1)[0]} round icon="artist" />, tracks, contextId: `artist:${id}`, empty: t("list.empty") };
      }
      case "album": {
        const parts = albumParts(id);
        const tracks = app.library.filter((track) => track.album === parts.album && track.artist === parts.artist);
        return {
          ...base,
          kind: t("kind.album"),
          title: parts.album || t("library.unknownAlbum"),
          cover: <Cover src={covers(tracks, 1)[0]} />,
          tracks,
          contextId: `album:${id}`,
          defaultSort: { key: "trackNumber", dir: 1 },
          columns: { album: false, number: "track" },
          extraMeta: parts.artist ? <strong>{parts.artist}</strong> : undefined,
          empty: t("list.empty"),
        };
      }
      case "folder": {
        const tracks = app.library.filter((track) => track.folder === id);
        return { ...base, kind: t("kind.folder"), title: folderName(id), cover: <Cover src={covers(tracks, 1)[0]} icon="folder" />, tracks, contextId: `folder:${id}`, columns: { folder: false }, extraMeta: <span className="mono ellipsis">{id}</span>, empty: t("list.empty") };
      }
      case "device": {
        const link = app.boot?.links.find((item) => item.deviceId === id);
        const tracks = app.tracks.filter((track) => track.holderIds.includes(id));
        const art = covers(tracks, 1, `device:${id}`)[0];
        return {
          ...base,
          kind: t("kind.device"),
          title: link?.name || id.slice(0, 8),
          cover: art ? <Cover src={art} /> : <Cover icon="device" />,
          tracks,
          contextId: `device:${id}`,
          columns: { folder: false, added: false },
          extraMeta: link ? <span className={`state-pill ${link.state}`}>{t(`link.${link.state}`)}</span> : undefined,
          empty: <EmptyHint icon={<Icons.devices />} title={t("empty.deviceTitle")} body={t("empty.deviceBody")} />,
        };
      }
      default:
        return {
          ...base,
          kind: t("kind.library"),
          title: t("nav.allMusic"),
          cover: covers(app.library, 1, "library")[0] ? <Cover src={covers(app.library, 1, "library")[0]} /> : <Cover />,
          tracks: app.library,
          contextId: "library",
          reorderId: "library",
          defaultSort: { key: "order", dir: 1 },
          shuffleAll: true,
          empty: <EmptyHint icon={<Icons.folder />} title={t("empty.libraryTitle")} body={t("empty.libraryBody")} />,
        };
    }
  }, [route.name, id, app.liked, app.recent, app.boot, app.index, app.members, app.library, app.tracks, t]);
}

export function EmptyHint({ icon, title, body }: { icon: ReactNode; title: string; body: string }) {
  return (
    <div className="empty-hint">
      <div className="empty-icon">{icon}</div>
      <div className="empty-title">{title}</div>
      <div className="empty-body">{body}</div>
    </div>
  );
}
