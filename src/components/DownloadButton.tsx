// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type Download, type DownloadFeed, type DownloadHistoryItem } from "../api";
import { useApp } from "../app/model";
import { displayTitle } from "../lib/library";
import { Icons } from "../ui/icons";
import { isAndroid } from "../lib/platform";
import { useBackClose } from "../lib/back";

const RING = 2 * Math.PI * 15;

function fraction(item: Download | null): number | null {
  if (!item || item.state !== "started" || !item.total) return null;
  return Math.max(0, Math.min(1, (item.received ?? 0) / item.total));
}

export function DownloadButton() {
  const { t } = useTranslation();
  const app = useApp();
  const [open, setOpen] = useState(false);
  useBackClose(open, () => setOpen(false));
  const root = useRef<HTMLDivElement>(null);
  const feed: DownloadFeed = app.downloadFeed ?? {
    current: app.boot?.download ?? null,
    queue: app.boot?.downloadQueue ?? [],
    history: app.boot?.downloadHistory ?? [],
  };
  const active = feed.current?.state === "started" ? feed.current : null;
  const progress = fraction(active);
  const waiting = feed.queue.length;
  const titleOf = (contentId: string, title?: string) => {
    if (title) return title;
    const track = app.index.get({ contentId });
    return track ? displayTitle(track, t("library.untitled")) : t("library.untitled");
  };

  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  }, [open]);

  const reveal = (path: string) => void api.revealPath(path).catch(app.say);

  return (
    <div className="download-anchor" ref={root}>
      <button
        type="button"
        className={`round-btn download-btn${open ? " on" : ""}`}
        aria-label={t("download.panel")}
        aria-expanded={open}
        title={active ? t("download.started", { title: titleOf(active.contentId, active.title) }) : t("download.panel")}
        onClick={() => setOpen((value) => !value)}
      >
        {(active || waiting > 0) && (
          <svg className={`download-ring${progress === null && active ? " unknown" : ""}`} viewBox="0 0 36 36" aria-hidden="true">
            <circle cx="18" cy="18" r="15" className="ring-track" />
            <circle
              cx="18"
              cy="18"
              r="15"
              className="ring-bar"
              style={progress === null ? undefined : { strokeDashoffset: RING * (1 - progress) }}
            />
          </svg>
        )}
        <Icons.download />
        {waiting > 0 && <span className="download-count">{waiting}</span>}
      </button>
      {open && (
        <div className="download-pop" role="dialog" aria-label={t("download.panel")}>
          <h3>{t("download.panel")}</h3>
          {!active && feed.queue.length === 0 && feed.history.length === 0 ? (
            <p className="muted small">{t("download.empty")}</p>
          ) : (
            <>
              {(active || feed.queue.length > 0) && (
                <section>
                  <h4>{t("download.now")}</h4>
                  {active && <ActiveRow title={titleOf(active.contentId, active.title)} progress={progress} />}
                  {feed.queue.map((item) => (
                    <QueueRow key={item.contentId} title={titleOf(item.contentId, item.title)} label={t("download.waiting")} />
                  ))}
                </section>
              )}
              {feed.history.length > 0 && (
                <section>
                  <h4>{t("download.history")}</h4>
                  {feed.history.map((item) => (
                    <HistoryRow key={`${item.at}:${item.contentId}:${item.path}`} item={item} title={titleOf(item.contentId, item.title)} showLabel={t("download.show")} failedLabel={t("download.failed", { title: titleOf(item.contentId, item.title) })} onShow={() => reveal(item.path)} />
                  ))}
                </section>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}

function ActiveRow({ title, progress }: { title: string; progress: number | null }) {
  const width = progress === null ? 35 : Math.round(progress * 100);
  return (
    <div className="download-row">
      <div className="download-text">
        <div className="download-title">{title}</div>
        <div className="download-meter" aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress === null ? undefined : width}>
          <span style={{ width: `${width}%` }} />
        </div>
      </div>
      <span className="download-pct mono">{progress === null ? "" : `${width}%`}</span>
    </div>
  );
}

function QueueRow({ title, label }: { title: string; label: string }) {
  return (
    <div className="download-row">
      <div className="download-text">
        <div className="download-title">{title}</div>
        <div className="muted small">{label}</div>
      </div>
    </div>
  );
}

function HistoryRow({ item, title, showLabel, failedLabel, onShow }: { item: DownloadHistoryItem; title: string; showLabel: string; failedLabel: string; onShow: () => void }) {
  return (
    <div className="download-row">
      <div className="download-text">
        <div className="download-title">{title}</div>
        {!item.ok && <div className="muted small">{failedLabel}</div>}
      </div>
      {item.ok && item.path && !isAndroid && (
        <button type="button" className="icon-btn" aria-label={showLabel} title={showLabel} onClick={onShow}>
          <Icons.folder />
        </button>
      )}
    </div>
  );
}
