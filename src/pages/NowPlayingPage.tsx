// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useCallback, useEffect, useMemo, useRef, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { useApp } from "../app/model";
import { Cover } from "../components/Cover";
import { Mascot } from "../components/Mascot";
import { tintFor } from "../lib/format";
import { albumId, displayTitle, trackKey } from "../lib/library";
import { activeLine, parseLyrics, type Lyrics } from "../lib/lrc";
import { clock, transport } from "../player/clock";
import { Icons } from "../ui/icons";

export function NowPlayingPage() {
  const { t } = useTranslation();
  const app = useApp();
  const track = app.shown;
  const { playing } = useSyncExternalStore(transport.subscribe, transport.get);
  const failed = Boolean(track && app.failure?.key === trackKey(track));
  const lyrics = useMemo<Lyrics | null>(() => {
    if (!track || !app.lyrics || app.lyrics.key !== trackKey(track) || !app.lyrics.text.trim()) return null;
    return parseLyrics(app.lyrics.text);
  }, [track, app.lyrics]);

  if (!track) {
    return (
      <div className="page now-page empty">
        <Mascot mood="idle" size={180} />
        <h1>{t("now.nothingTitle")}</h1>
        <p className="muted">{t("now.nothingBody")}</p>
        <button type="button" className="btn primary" onClick={() => app.shuffleLibrary()}>
          <Icons.shuffle /> {t("list.shuffleLibrary")}
        </button>
      </div>
    );
  }

  const live = playing;
  const mood = failed ? "error" : live ? "playing" : "paused";
  const tint = tintFor(track.album || track.title);
  return (
    <div className="page now-page" style={{ ["--tint" as string]: tint }}>
      <div className="now-art">
        <div className="now-cover-wrap">
          <Cover src={track.coverUrl} className={`now-cover${live ? " spinning-glow" : ""}`} />
        </div>
        <div className="now-meta">
          <div className="now-title-row">
            <h1 className="now-title" title={displayTitle(track, t("library.untitled"))}>
              {displayTitle(track, t("library.untitled"))}
            </h1>
            <button type="button" className={`icon-btn lg${track.favorite ? " liked" : ""}`} aria-label={track.favorite ? t("menu.unlike") : t("menu.like")} onClick={() => void app.like(track)}>
              {track.favorite ? <Icons.heartFill /> : <Icons.heart />}
            </button>
          </div>
          <div className="now-sub">
            {track.artist ? (
              <button type="button" className="link" onClick={() => app.navigate({ name: "artist", id: track.artist })}>
                {track.artist}
              </button>
            ) : (
              <span>{t("library.unknownArtist")}</span>
            )}
            {track.album && (
              <>
                <span className="dot-sep">·</span>
                <button type="button" className="link" onClick={() => app.navigate({ name: "album", id: albumId(track.artist, track.album) })}>
                  {track.album}
                </button>
              </>
            )}
          </div>
          <div className="now-foot">
            <div className="now-badges">
              <span className={`badge ${track.badge}`}>{t(`badge.${track.badge}`)}</span>
              {app.engine.remoteDevice && <span className="badge remote">{t("now.from", { name: app.boot?.links.find((link) => link.deviceId === app.engine.remoteDevice)?.name ?? "" })}</span>}
              {failed && app.failure && <span className="badge error">{t(app.failure.reason, { defaultValue: t("notice.playbackFailed") })}</span>}
            </div>
            <div className="now-mascot">
              <Mascot mood={mood} size={88} />
            </div>
          </div>
        </div>
      </div>
      <LyricsPanel key={trackKey(track)} lyrics={lyrics} hasLocal={Boolean(track.localFileId)} />
    </div>
  );
}

function LyricsPanel({ lyrics, hasLocal }: { lyrics: Lyrics | null; hasLocal: boolean }) {
  const { t } = useTranslation();
  const app = useApp();
  const box = useRef<HTMLDivElement>(null);
  const userScroll = useRef(0);
  const lines = lyrics?.timed ? lyrics.lines : null;
  const getActive = useCallback(() => (lines ? activeLine(lines, clock.get().position) : -1), [lines]);
  const active = useSyncExternalStore(clock.subscribe, getActive);

  useEffect(() => {
    if (active < 0 || !box.current || Date.now() - userScroll.current < 4000) return;
    const node = box.current.querySelector<HTMLElement>(`[data-line="${active}"]`);
    if (!node) return;
    // offsetTop would resolve against the nearest positioned ancestor rather than
    // the panel itself, so measure from the two rects instead.
    const boxRect = box.current.getBoundingClientRect();
    const offset = node.getBoundingClientRect().top - boxRect.top + box.current.scrollTop;
    const target = offset - box.current.clientHeight / 2 + node.clientHeight / 2;
    box.current.scrollTo({ top: Math.max(0, target), behavior: "smooth" });
  }, [active]);

  if (!lyrics) {
    return (
      <div className="lyrics-panel empty">
        <Icons.lyrics />
        <p>{hasLocal ? t("now.noLyrics") : t("now.noLyricsRemote")}</p>
        <p className="muted small">{t("now.lyricsHint")}</p>
      </div>
    );
  }

  return (
    <div
      className={`lyrics-panel${lyrics.timed ? " timed" : ""}`}
      ref={box}
      onWheel={() => {
        userScroll.current = Date.now();
      }}
      onPointerDown={() => {
        userScroll.current = Date.now();
      }}
    >
      <div className="lyrics-pad" />
      {lyrics.lines.map((line, at) =>
        lyrics.timed ? (
          <button
            type="button"
            key={`${line.time}-${at}`}
            data-line={at}
            className={`lyric-line${at === active ? " active" : at < active ? " past" : ""}`}
            onClick={() => {
              userScroll.current = 0;
              app.seek(line.time);
            }}
          >
            {line.text || "♪"}
          </button>
        ) : (
          <p key={at} className="lyric-line plain">
            {line.text || "\u00a0"}
          </p>
        ),
      )}
      <div className="lyrics-pad" />
    </div>
  );
}
