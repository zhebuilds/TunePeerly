// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import {
  memo,
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type MouseEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import type { Track } from "../api";
import { beginDrag, clickWasDrag, type DragSource } from "../app/drag";
import { useApp } from "../app/model";
import { useScrollRoot } from "../app/scroll";
import { clockText, dateText, snippetParts } from "../lib/format";
import { albumId, displayTitle, folderName, trackKey, type Sort, type SortKey } from "../lib/library";
import { clock, transport } from "../player/clock";
import { Icons } from "../ui/icons";
import { Cover } from "./Cover";

const ROW = 56;

function playIndex(playTracks: Track[] | undefined, tracks: Track[], at: number): number {
  if (!playTracks) return at;
  const found = playTracks.indexOf(tracks[at]);
  return found >= 0 ? found : at;
}

export type Columns = { album?: boolean; folder?: boolean; added?: boolean; source?: boolean; number?: "index" | "track" };

type Props = {
  tracks: Track[];
  contextId: string;
  sort?: Sort;
  onSort?: (sort: Sort) => void;
  defaultSort?: Sort;
  playlistId?: string;
  reorderId?: string;
  reorderable?: boolean;
  columns?: Columns;
  empty?: ReactNode;
  /** Playback list when `tracks` is only the rows on screen. Indexes follow `tracks`. */
  playTracks?: Track[];
};

export function TrackTable({ tracks, contextId, sort, onSort, defaultSort, playlistId, reorderId, reorderable, columns = {}, empty, playTracks }: Props) {
  const { t } = useTranslation();
  const app = useApp();
  const scroller = useScrollRoot();
  const body = useRef<HTMLDivElement>(null);
  const [range, setRange] = useState({ start: 0, end: 40 });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const anchor = useRef<number | null>(null);
  const currentKey = app.current ? trackKey(app.current) : null;
  const cols = { album: true, folder: true, added: true, source: true, number: "index" as const, ...columns };

  useLayoutEffect(() => {
    const root = scroller.current;
    const list = body.current;
    if (!root || !list) return;
    const update = () => {
      const offset = list.getBoundingClientRect().top - root.getBoundingClientRect().top;
      const start = Math.max(0, Math.floor(-offset / ROW) - 10);
      // A negative end is not "nothing": Array.slice treats it as an offset from the end.
      const visibleEnd = Math.ceil((root.clientHeight - offset) / ROW) + 10;
      const end = Math.min(tracks.length, Math.max(start, visibleEnd));
      setRange((value) => (value.start === start && value.end === end ? value : { start, end }));
    };
    update();
    root.addEventListener("scroll", update, { passive: true });
    const observer = new ResizeObserver(update);
    observer.observe(root);
    return () => {
      root.removeEventListener("scroll", update);
      observer.disconnect();
    };
  }, [scroller, tracks.length]);

  const keys = useMemo(() => tracks.map(trackKey), [tracks]);

  const chosen = useCallback(
    (index: number): Track[] => {
      const key = keys[index];
      if (selected.has(key) && selected.size > 1) return tracks.filter((track) => selected.has(trackKey(track)));
      return [tracks[index]];
    },
    [keys, selected, tracks],
  );

  const select = useCallback(
    (index: number, event: MouseEvent) => {
      const key = keys[index];
      setSelected((value) => {
        if (event.shiftKey && anchor.current !== null) {
          const [from, to] = [Math.min(anchor.current, index), Math.max(anchor.current, index)];
          return new Set(keys.slice(from, to + 1));
        }
        if (event.ctrlKey || event.metaKey) {
          const next = new Set(value);
          if (next.has(key)) next.delete(key);
          else next.add(key);
          anchor.current = index;
          return next;
        }
        anchor.current = index;
        return new Set([key]);
      });
    },
    [keys],
  );

  const header = (key: SortKey, label: ReactNode, className = "") => {
    if (!onSort || !sort) return <span className={`th ${className}`}>{label}</span>;
    const active = sort.key === key;
    const preferred = key === "addedAt" || key === "favorite" || key === "duration" ? -1 : 1;
    const nextSort = (): Sort => (active ? { key, dir: sort.dir === 1 ? -1 : 1 } : { key, dir: preferred });
    return (
      <button type="button" className={`th sortable${active ? " active" : ""} ${className}`} title={t("list.sortColumn")} aria-label={typeof label === "string" ? undefined : t("list.sortColumn")} onClick={() => onSort(nextSort())}>
        <span>{label}</span>
        <Icons.chevronDown className={`sort-mark${active && sort.dir === 1 ? " up" : ""}`} />
      </button>
    );
  };

  if (tracks.length === 0) return <div className="table-empty">{empty ?? t("list.empty")}</div>;

  const gridClass = `track-grid${cols.album ? "" : " no-album"}${cols.folder ? "" : " no-folder"}${cols.added ? "" : " no-added"}${cols.source ? "" : " no-source"}`;

  return (
    <div className="track-table" role="grid" aria-rowcount={tracks.length} onKeyDown={(event) => {
      if (event.key === "Escape") {
        setSelected(new Set());
        return;
      }
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      // the global shortcuts (Ctrl+arrows for the volume, Ctrl+Left/Right for the track) stay untouched
      if (event.ctrlKey || event.altKey || event.shiftKey) return;
      const row = (event.target as HTMLElement).closest<HTMLElement>(".track-row");
      const at = Number(row?.dataset.rowIndex);
      if (!row || !Number.isFinite(at)) return;
      const step = event.key === "ArrowDown" ? 1 : -1;
      const next = at + step;
      if (next < 0 || next >= tracks.length) return;
      event.preventDefault();
      const rows = Array.from(body.current?.querySelectorAll<HTMLElement>(".track-row") ?? []);
      const target = rows[rows.indexOf(row) + step];
      if (target) {
        target.focus({ preventScroll: true });
        target.scrollIntoView({ block: "nearest" });
        return;
      }
      // Rows are virtualised: scroll by one row and pick the row up once it has rendered.
      scroller.current?.scrollBy({ top: step * ROW });
      requestAnimationFrame(() => {
        const el = body.current?.querySelector<HTMLElement>(`[data-row-index="${next}"]`);
        el?.focus({ preventScroll: true });
        el?.scrollIntoView({ block: "nearest" });
      });
    }}>
      <div className={`track-head ${gridClass}`} role="row">
        {sort && onSort && cols.number === "index" ? (
          <button type="button" className={`th num sortable${sort.key === (defaultSort?.key ?? "order") ? " active" : ""}`} title={t("list.sortColumn")} onClick={() => onSort(defaultSort ?? { key: "order", dir: 1 })}>
            <span>#</span>
            <Icons.chevronDown className={`sort-mark${sort.key === (defaultSort?.key ?? "order") && sort.dir === 1 ? " up" : ""}`} />
          </button>
        ) : (
          <span className="th num">#</span>
        )}
        {header("title", t("list.title"), "c-title")}
        {cols.source && <span className="th c-source">{t("list.source")}</span>}
        {cols.album && header("album", t("list.album"), "c-album")}
        {cols.folder && header("folder", t("list.folder"), "c-folder")}
        {cols.added && header("addedAt", t("list.added"), "c-added")}
        {header("favorite", <Icons.heart />, "c-like")}
        {header("duration", <Icons.clock />, "c-time")}
        <span className="th c-more" />
      </div>
      <div ref={body} className="track-body" style={{ height: tracks.length * ROW }}>
        {tracks.slice(range.start, range.end).map((track, offset) => {
          const index = range.start + offset;
          const key = keys[index];
          return (
            <TrackRow
              key={`${key}:${index}`}
              track={track}
              index={index}
              top={index * ROW}
              number={cols.number === "track" ? track.trackNumber ?? "" : index + 1}
              gridClass={gridClass}
              cols={cols}
              current={key === currentKey}
              selected={selected.has(key)}
              dropId={reorderable && reorderId ? `member:${reorderId}:${index}` : undefined}
              onSelect={select}
              onPlay={(at) => {
                if (key === currentKey && at === index && app.current) app.toggle();
                else app.playList(playTracks ?? tracks, playIndex(playTracks, tracks, at), contextId);
              }}
              onMenu={(event, at) => {
                event.preventDefault();
                const items = chosen(at);
                if (!selected.has(keys[at])) setSelected(new Set([keys[at]]));
                app.setMenu({ x: event.clientX, y: event.clientY, tracks: items, playlistId, play: () => app.playList(playTracks ?? tracks, playIndex(playTracks, tracks, at), contextId) });
              }}
              onDrag={(event, at) =>
                beginDrag(
                  event,
                  (): DragSource => {
                    const items = chosen(at);
                    return reorderable && reorderId && items.length === 1
                      ? { type: "member", playlistId: reorderId, index: at, tracks: items }
                      : { type: "tracks", tracks: items };
                  },
                  () => {
                    const items = chosen(at);
                    return items.length === 1 ? displayTitle(items[0], t("library.untitled")) : t("list.songCount", { count: items.length });
                  },
                )
              }
            />
          );
        })}
      </div>
    </div>
  );
}

type RowProps = {
  track: Track;
  index: number;
  top: number;
  number: number | string;
  gridClass: string;
  cols: Required<Omit<Columns, "number">> & { number: string };
  current: boolean;
  selected: boolean;
  dropId?: string;
  onSelect: (index: number, event: MouseEvent) => void;
  onPlay: (index: number) => void;
  onMenu: (event: MouseEvent, index: number) => void;
  onDrag: (event: ReactPointerEvent, index: number) => void;
};

const TrackRow = memo(function TrackRow({ track, index, top, number, gridClass, cols, current, selected, dropId, onSelect, onPlay, onMenu, onDrag }: RowProps) {
  const { t, i18n } = useTranslation();
  const app = useApp();
  const title = displayTitle(track, t("library.untitled"));
  const reason = track.reasonKey ? t(track.reasonKey, { defaultValue: "" }) : "";
  return (
    <div
      className={`track-row ${gridClass}${current ? " current" : ""}${selected ? " selected" : ""}${track.dimmed ? " dim" : ""}`}
      style={{ transform: `translateY(${top}px)` }}
      role="row"
      tabIndex={0}
      data-row-index={index}
      data-drop={dropId}
      title={reason || undefined}
      onPointerDown={(event) => onDrag(event, index)}
      onClick={(event) => {
        if (!clickWasDrag()) onSelect(index, event);
      }}
      onDoubleClick={(event) => {
        if (!(event.target as HTMLElement).closest("button, input, a, select, textarea")) onPlay(index);
      }}
      onContextMenu={(event) => onMenu(event, index)}
      onKeyDown={(event) => {
        if (event.key === "Enter" && !(event.target as HTMLElement).closest("button, input, a, select, textarea")) onPlay(index);
      }}
    >
      <div className="cell num">
        {current ? <NowMark /> : <span className="num-text">{number}</span>}
        <button type="button" className="row-play" aria-label={t("player.play")} onClick={(event) => {
          event.stopPropagation();
          onPlay(index);
        }}>
          {current ? <CurrentPlayIcon /> : <Icons.play />}
        </button>
      </div>
      <div className="cell c-title">
        <Cover src={track.coverUrl} className="row-cover" />
        <div className="title-text">
          <div className="track-title">{title}</div>
          <div className="track-sub">
            {track.badge !== "local" ? <span className={`dot-badge ${track.badge}`} title={t(`badge.${track.badge}`)} /> : null}
            {track.artist ? (
              <button type="button" className="link" onClick={(event) => {
                event.stopPropagation();
                app.navigate({ name: "artist", id: track.artist });
              }}>
                {track.artist}
              </button>
            ) : (
              <span>{t("library.unknownArtist")}</span>
            )}
          </div>
          {track.snippet && (
            <div className="track-snippet">
              {snippetParts(track.snippet).map((part, at) => (part.hit ? <mark key={at}>{part.text}</mark> : <span key={at}>{part.text}</span>))}
            </div>
          )}
        </div>
      </div>
      {cols.source && (
        <div className="cell c-source">
          <span className={`badge ${track.badge}`}>{t(`badge.${track.badge}`)}</span>
          {track.localFileId && (
            <span className={`badge expose${track.exposed ? " on" : ""}`} title={track.exposed ? t("list.exposedHelp") : t("list.hiddenHelp")}>
              {track.exposed ? <Icons.eye /> : <Icons.eyeOff />}
              {track.exposed ? t("list.exposed") : t("list.hidden")}
            </span>
          )}
        </div>
      )}
      {cols.album && (
        <div className="cell c-album">
          {track.album ? (
            <button type="button" className="link" onClick={(event) => {
              event.stopPropagation();
                app.navigate({ name: "album", id: albumId(track.artist, track.album) });
            }}>
              {track.album}
            </button>
          ) : null}
        </div>
      )}
      {cols.folder && (
        <div className="cell c-folder" title={track.folder}>
          {track.folder ? (
            <button type="button" className="link" onClick={(event) => {
              event.stopPropagation();
              app.navigate({ name: "folder", id: track.folder });
            }}>
              {folderName(track.folder)}
            </button>
          ) : null}
        </div>
      )}
      {cols.added && <div className="cell c-added">{dateText(track.addedAt, i18n.language)}</div>}
      <div className="cell c-like">
        <button
          type="button"
          className={`like-btn${track.favorite ? " on" : ""}`}
          aria-label={track.favorite ? t("menu.unlike") : t("menu.like")}
          aria-pressed={track.favorite}
          onClick={(event) => {
            event.stopPropagation();
            void app.like(track);
          }}
        >
          {track.favorite ? <Icons.heartFill /> : <Icons.heart />}
        </button>
      </div>
      <div className="cell c-time">{current ? <RemainingTime total={(track.durationMs ?? 0) / 1000} /> : <TimeCell total={(track.durationMs ?? 0) / 1000} />}</div>
      <div className="cell c-more">
        <button type="button" className="more-btn" aria-label={t("menu.more")} onClick={(event) => {
          event.stopPropagation();
          onMenu(event, index);
        }}>
          <Icons.more />
        </button>
      </div>
    </div>
  );
});

function TimeCell({ total }: { total: number }) {
  return (
    <span className="time-text">
      <span className="time-total">{total > 0 ? clockText(total) : "–"}</span>
      <span className="time-left">{total > 0 ? `-${clockText(total)}` : "–"}</span>
    </span>
  );
}

function RemainingTime({ total }: { total: number }) {
  const time = useSyncExternalStore(clock.subscribe, clock.get);
  const duration = time.duration || total;
  return (
    <span className="time-text">
      <span className="time-total">{duration > 0 ? clockText(duration) : "–"}</span>
      <span className="time-left">{duration > 0 ? `-${clockText(Math.max(0, duration - time.position))}` : "–"}</span>
    </span>
  );
}

function NowMark() {
  const { playing } = useSyncExternalStore(transport.subscribe, transport.get);
  return (
    <span className={`eq${playing ? " on" : ""}`} aria-hidden="true">
      <i />
      <i />
      <i />
      <i />
    </span>
  );
}

function CurrentPlayIcon() {
  const { playing } = useSyncExternalStore(transport.subscribe, transport.get);
  return playing ? <Icons.pause /> : <Icons.play />;
}
