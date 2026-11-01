// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useEffect, useRef, useState, useSyncExternalStore, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { useApp } from "../app/model";
import { coverSrc, Cover } from "./Cover";
import { clockText } from "../lib/format";
import { displayTitle, trackKey } from "../lib/library";
import { clock, transport } from "../player/clock";
import { Icons } from "../ui/icons";
import { isAndroid } from "../lib/platform";

export function PlayerBar() {
  const { t } = useTranslation();
  const app = useApp();
  const { playing } = useSyncExternalStore(transport.subscribe, transport.get);
  const track = app.shown;
  const order = app.queue.order;
  const repeat = app.queue.repeat;
  const failed = Boolean(track && app.failure?.key === trackKey(track));
  // The badge names the device streaming to this computer, if any.
  const remoteName = app.engine.remoteDevice ? app.boot?.links.find((link) => link.deviceId === app.engine.remoteDevice)?.name : null;
  const showPlaying = playing;

  useMediaSession();
  useDesktopNowPlaying();

  return (
    <footer className={`player-bar${isAndroid && app.route.name === "now" && !app.queueOpen && !app.noticesOpen ? " expanded" : ""}`}>
      {isAndroid && <MiniProgress fallback={(track?.durationMs ?? 0) / 1000} />}
      <div className="player-left">
        {track ? (
          <>
            <button type="button" className="player-cover" aria-label={t("now.open")} onClick={() => app.navigate(app.route.name === "now" ? { name: "home" } : { name: "now" })}>
              <Cover src={track.coverUrl} />
            </button>
            <div className="player-text">
              <button type="button" className="player-title" title={displayTitle(track, t("library.untitled"))} onClick={() => app.navigate({ name: "now" })}>
                {displayTitle(track, t("library.untitled"))}
              </button>
              <div className="player-sub">
                <span className={`badge mini ${failed ? "error" : track.badge}`} title={failed && app.failure ? t(app.failure.reason, { defaultValue: "" }) : undefined}>
                  {failed ? t("badge.error") : remoteName ? remoteName : t(`badge.${track.badge}`)}
                </span>
                {track.artist ? (
                  <button type="button" className="link" onClick={() => app.navigate({ name: "artist", id: track.artist })}>
                    {track.artist}
                  </button>
                ) : (
                  <span className="muted">{t("library.unknownArtist")}</span>
                )}
              </div>
            </div>
            <button type="button" className={`icon-btn${track.favorite ? " liked" : ""}`} aria-label={track.favorite ? t("menu.unlike") : t("menu.like")} title={track.favorite ? t("menu.unlike") : t("menu.like")} onClick={() => void app.like(track)}>
              {track.favorite ? <Icons.heartFill /> : <Icons.heart />}
            </button>
          </>
        ) : (
          <div className="player-idle muted">{t("player.idle")}</div>
        )}
      </div>

      <div className="player-center">
        <div className="player-controls">
          <button type="button" className={`icon-btn${order !== "manual" ? " on" : ""}`} aria-pressed={order !== "manual"} aria-label={t("player.shuffle")} title={order === "libraryRandom" ? t("player.libraryRandom") : t("player.shuffle")} onClick={app.toggleShuffle}>
            <Icons.shuffle />
          </button>
          <button type="button" className="icon-btn" aria-label={t("player.previous")} title={t("player.previousShortcut")} onClick={app.previous} disabled={!track}>
            <Icons.prev />
          </button>
          <button type="button" className="play-btn" aria-label={showPlaying ? t("player.pause") : t("player.play")} title={t("player.playShortcut")} onClick={app.toggle} disabled={!track && app.library.length === 0 && app.queue.future.length === 0}>
            {showPlaying ? <Icons.pause /> : <Icons.play />}
          </button>
          <button type="button" className="icon-btn" aria-label={t("player.next")} title={t("player.nextShortcut")} onClick={app.next} disabled={!track}>
            <Icons.next />
          </button>
          <button type="button" className={`icon-btn${repeat !== "off" ? " on" : ""}`} aria-label={t(`player.repeat.${repeat}`)} title={t(`player.repeat.${repeat}`)} onClick={app.cycleRepeat}>
            {repeat === "one" ? <Icons.repeatOne /> : <Icons.repeat />}
          </button>
        </div>
        <Seek disabled={!track} fallback={(track?.durationMs ?? 0) / 1000} />
      </div>

      <div className="player-right">
        <button type="button" className={`icon-btn${app.route.name === "now" ? " on" : ""}`} aria-label={t("player.lyrics")} title={t("player.lyrics")} onClick={() => (app.route.name === "now" ? app.back() : app.navigate({ name: "now" }))}>
          <Icons.lyrics />
        </button>
        <button type="button" className={`icon-btn${app.queueOpen ? " on" : ""}`} aria-label={t("player.queue")} title={t("player.queue")} aria-pressed={app.queueOpen} onClick={() => app.setQueueOpen(!app.queueOpen)}>
          <Icons.queue />
        </button>
        <Volume />
        <button type="button" className={`icon-btn notice-btn${app.noticesOpen ? " on" : ""}`} aria-label={t("player.notices")} title={t("player.notices")} aria-pressed={app.noticesOpen} onClick={() => app.setNoticesOpen(!app.noticesOpen)}>
          <Icons.bell />
          {app.unreadNotices > 0 && <span className="notice-badge">{app.unreadNotices > 99 ? "99+" : app.unreadNotices}</span>}
        </button>
      </div>
    </footer>
  );
}

/** The phone's thin, read-only progress line along the top of the mini player. */
function MiniProgress({ fallback }: { fallback: number }) {
  const time = useSyncExternalStore(clock.subscribe, clock.get);
  const duration = time.duration > 0 ? time.duration : fallback;
  const fill = duration > 0 ? Math.min(100, (time.position / duration) * 100) : 0;
  return (
    <div className="mini-progress" aria-hidden="true">
      <span style={{ width: `${fill}%` }} />
    </div>
  );
}

function Seek({ disabled, fallback }: { disabled: boolean; fallback: number }) {
  const { t } = useTranslation();
  const app = useApp();
  const time = useSyncExternalStore(clock.subscribe, clock.get);
  const [scrub, setScrub] = useState<number | null>(null);
  const [remaining, setRemaining] = useState(() => localStorage.getItem("showRemaining") === "1");
  const duration = time.duration > 0 ? time.duration : fallback;
  const position = scrub ?? Math.min(time.position, duration || time.position);
  const fill = duration > 0 ? (position / duration) * 100 : 0;
  const commit = () => {
    if (scrub !== null) app.seek(scrub);
    setScrub(null);
  };
  return (
    <div className="seek">
      <span className="time mono">{clockText(position)}</span>
      <input
        type="range"
        className="slider"
        min={0}
        max={duration || 1}
        step={0.1}
        value={position}
        disabled={disabled || duration <= 0}
        aria-label={t("player.seek")}
        style={{ "--fill": `${fill}%` } as CSSProperties}
        onChange={(event) => setScrub(Number(event.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={() => scrub !== null && commit()}
      />
      <button
        type="button"
        className="time mono time-toggle"
        title={t("player.toggleRemaining")}
        onClick={() => {
          localStorage.setItem("showRemaining", remaining ? "0" : "1");
          setRemaining(!remaining);
        }}
      >
        {remaining && duration > 0 ? `-${clockText(Math.max(0, duration - position))}` : clockText(duration)}
      </button>
    </div>
  );
}

function Volume() {
  const { t } = useTranslation();
  const app = useApp();
  const level = app.muted ? 0 : app.volume;
  const Icon = level === 0 ? Icons.volumeMute : level < 0.5 ? Icons.volumeLow : Icons.volume;
  const box = useRef<HTMLDivElement>(null);
  const latest = useRef(app);
  latest.current = app;
  useEffect(() => {
    const node = box.current;
    if (!node) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const model = latest.current;
      model.setVolume(model.volume + (event.deltaY < 0 ? 0.05 : -0.05));
    };
    node.addEventListener("wheel", onWheel, { passive: false });
    return () => node.removeEventListener("wheel", onWheel);
  }, []);
  return (
    <div className="volume" ref={box}>
      <button type="button" className="icon-btn" aria-label={app.muted ? t("player.unmute") : t("player.mute")} title={app.muted ? t("player.unmute") : t("player.mute")} onClick={app.toggleMute}>
        <Icon />
      </button>
      <input
        type="range"
        className="slider"
        min={0}
        max={1}
        step={0.01}
        value={level}
        aria-label={t("player.volume")}
        title={`${Math.round(level * 100)}%`}
        style={{ "--fill": `${level * 100}%` } as CSSProperties}
        onChange={(event) => app.setVolume(Number(event.target.value))}
      />
    </div>
  );
}

function useDesktopNowPlaying() {
  const { t, i18n } = useTranslation();
  const app = useApp();
  const { playing } = useSyncExternalStore(transport.subscribe, transport.get);
  const time = useSyncExternalStore(clock.subscribe, clock.get);
  const track = app.shown;
  const position = time.position;
  const second = Math.floor(position);
  const duration = time.duration || (track?.durationMs ?? 0) / 1000;
  // Android colours its status and navigation bars from the theme sent here, so resend when it changes.
  const [look, setLook] = useState(0);
  useEffect(() => {
    if (!isAndroid) return;
    const watch = new MutationObserver(() => setLook((value) => value + 1));
    watch.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => watch.disconnect();
  }, []);
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    const cover = track?.coverUrl && !/^(data:|blob:|https?:)/.test(track.coverUrl) ? track.coverUrl : null;
    void api.desktopUpdate({
      title: track ? displayTitle(track, t("library.untitled")) : "",
      artist: track?.artist ?? "",
      album: track?.album ?? "",
      coverPath: cover,
      playing: Boolean(track) && playing,
      favorite: Boolean(track?.favorite),
      duration,
      position,
      theme: document.documentElement.dataset.theme === "dark" ? "dark" : "light",
      language: i18n.language.startsWith("zh") ? "zh" : "en",
      tips: {
        previous: t("player.previous"),
        play: t("player.play"),
        pause: t("player.pause"),
        next: t("player.next"),
        like: t("menu.like"),
      },
    }).catch(() => {});
  }, [track, playing, second, duration, i18n.language, t, look]);
}

function useMediaSession() {
  const { t } = useTranslation();
  const app = useApp();
  const track = app.shown;
  const { playing } = useSyncExternalStore(transport.subscribe, transport.get);
  useEffect(() => {
    if (!("mediaSession" in navigator)) return;
    const session = navigator.mediaSession;
    session.metadata = track
      ? new MediaMetadata({
          title: displayTitle(track, t("library.untitled")),
          artist: track.artist,
          album: track.album,
          artwork: track.coverUrl ? [{ src: coverSrc(track.coverUrl) }] : [],
        })
      : null;
    session.playbackState = playing ? "playing" : "paused";
  }, [track, t, playing]);
  useEffect(() => {
    if (!("mediaSession" in navigator)) return;
    const session = navigator.mediaSession;
    const set = (action: MediaSessionAction, handler: MediaSessionActionHandler | null) => {
      try {
        session.setActionHandler(action, handler);
      } catch {
        // Some actions are not supported by every WebView.
      }
    };
    set("play", () => app.play());
    set("pause", () => app.pause());
    set("previoustrack", () => app.previous());
    set("nexttrack", () => app.next());
    set("seekto", (details) => {
      if (details.seekTime !== undefined) app.seek(details.seekTime);
    });
    return () => {
      for (const action of ["play", "pause", "previoustrack", "nexttrack", "seekto"] as MediaSessionAction[]) set(action, null);
    };
  }, [app.play, app.pause, app.previous, app.next, app.seek]);
}
