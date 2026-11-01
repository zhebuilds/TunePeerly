// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import type { Track } from "../api";
import { beginDrag, clickWasDrag } from "../app/drag";
import { useApp } from "../app/model";
import { clockText } from "../lib/format";
import { albumParts, displayTitle } from "../lib/library";
import { transport } from "../player/clock";
import { Icons } from "../ui/icons";
import { Cover } from "./Cover";

export function QueuePanel() {
  const { t } = useTranslation();
  const app = useApp();
  const { playing } = useSyncExternalStore(transport.subscribe, transport.get);
  const future = app.queue.future.map((ref) => ({ ref, track: app.lookup(ref) }));
  const contextLabel = useContextLabel(app.queue.contextId);

  return (
    <aside className="queue-panel" aria-label={t("player.queue")}>
      <header className="queue-head" data-tauri-drag-region>
        <h2>{t("queue.title")}</h2>
        <button type="button" className="icon-btn" data-tauri-drag-region="false" aria-label={t("common.close")} onClick={() => app.setQueueOpen(false)}>
          <Icons.close />
        </button>
      </header>
      <div className="queue-scroll">
        <section>
          <h3>{t("queue.nowPlaying")}</h3>
          {app.current ? (
            <QueueRow track={app.current} current playing={playing} onPlay={app.toggle} />
          ) : (
            <p className="muted small">{t("queue.nothing")}</p>
          )}
        </section>
        <section>
          <div className="queue-sub">
            <h3>
              {app.queue.order === "libraryRandom" ? t("queue.randomFrom") : t("queue.nextFrom")} <span className="accent-text">{contextLabel}</span>
            </h3>
            {future.length > 0 && (
              <button type="button" className="link-btn" onClick={app.queueActions.clear}>
                {t("queue.clear")}
              </button>
            )}
          </div>
          {future.length === 0 ? (
            <div className="queue-empty" data-drop="queue:0">
              <p className="muted small">{app.queue.order === "libraryRandom" ? t("queue.randomHint") : t("queue.emptyHint")}</p>
            </div>
          ) : (
            <ol className="queue-list">
              {future.map(({ ref, track }, at) => (
                <li
                  key={`${ref.key}:${at}`}
                  className="queue-item"
                  data-drop={`queue:${at}`}
                  onPointerDown={(event) =>
                    track &&
                    beginDrag(
                      event,
                      () => ({ type: "queue", index: at, tracks: [track] }),
                      () => displayTitle(track, t("library.untitled")),
                    )
                  }
                  onContextMenu={(event) => {
                    event.preventDefault();
                    if (track) app.setMenu({ x: event.clientX, y: event.clientY, tracks: [track], queueIndex: at });
                  }}
                >
                  {track ? (
                    <QueueRow
                      track={track}
                      onPlay={() => {
                        if (!clickWasDrag()) app.queueActions.jump(at);
                      }}
                      onRemove={() => app.queueActions.remove(at)}
                    />
                  ) : (
                    <div className="queue-row missing">
                      <span className="muted small">{t("queue.missing")}</span>
                      <button type="button" className="icon-btn small" aria-label={t("queue.remove")} onClick={() => app.queueActions.remove(at)}>
                        <Icons.close />
                      </button>
                    </div>
                  )}
                </li>
              ))}
            </ol>
          )}
        </section>
      </div>
    </aside>
  );
}

function QueueRow({ track, current, playing, onPlay, onRemove }: { track: Track; current?: boolean; playing?: boolean; onPlay: () => void; onRemove?: () => void }) {
  const { t } = useTranslation();
  return (
    <div
      className={`queue-row${current ? " current" : ""}${track.dimmed ? " dim" : ""}`}
      onDoubleClick={(event) => {
        if (!(event.target as HTMLElement).closest("button, input, a, select, textarea")) onPlay();
      }}
    >
      <button type="button" className="queue-cover" aria-label={t("player.play")} onClick={onPlay}>
        <Cover src={track.coverUrl} />
        <span className="queue-cover-play">{current && playing ? <Icons.pause /> : <Icons.play />}</span>
      </button>
      <div className="queue-text">
        <div className="queue-title">{displayTitle(track, t("library.untitled"))}</div>
        <div className="queue-artist">{track.artist || t("library.unknownArtist")}</div>
      </div>
      <span className="queue-time mono">{track.durationMs ? clockText(track.durationMs / 1000) : ""}</span>
      {onRemove && (
        <button type="button" className="icon-btn small queue-remove" aria-label={t("queue.remove")} title={t("queue.remove")} onClick={onRemove}>
          <Icons.close />
        </button>
      )}
    </div>
  );
}

function useContextLabel(contextId: string): string {
  const { t } = useTranslation();
  const app = useApp();
  const [kind, ...rest] = contextId.split(":");
  const value = rest.join(":");
  switch (kind) {
    case "liked":
      return t("nav.liked");
    case "recent":
      return t("nav.recent");
    case "playlist":
      return app.boot?.playlists.find((item) => item.id === value)?.name ?? t("kind.playlist");
    case "artist":
    case "album":
      return albumParts(value).album || t("kind.album");
    case "folder":
      return value.split(/[\\/]/).filter(Boolean).pop() ?? value;
    case "device":
      return app.boot?.links.find((link) => link.deviceId === value)?.name ?? t("kind.device");
    case "random":
      return t("nav.allMusic");
    case "search":
      return t("nav.search");
    case "added":
      return t("home.added");
    default:
      return t("nav.allMusic");
  }
}
