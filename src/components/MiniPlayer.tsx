// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent } from "react";
import { useTranslation } from "react-i18next";
import { api, type DesktopTrack } from "../api";
import { coverSrc } from "./Cover";
import { Icons } from "../ui/icons";

function useDragOrClick(onClick: () => void) {
  const start = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  return {
    onPointerDown: (event: ReactPointerEvent) => {
      if (event.button !== 0) return;
      start.current = { x: event.screenX, y: event.screenY, moved: false };
    },
    onPointerMove: (event: ReactPointerEvent) => {
      const origin = start.current;
      if (!origin || origin.moved) return;
      if (Math.hypot(event.screenX - origin.x, event.screenY - origin.y) <= 4) return;
      origin.moved = true;
      void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => getCurrentWindow().startDragging()).catch(() => {});
    },
    onPointerUp: () => {
      const origin = start.current;
      start.current = null;
      if (origin && !origin.moved) onClick();
    },
  };
}

export function MiniPlayer() {
  const { t, i18n } = useTranslation();
  const [track, setTrack] = useState<DesktopTrack | null>(null);
  useEffect(() => {
    let gone = false;
    let stop = () => {};
    void api.desktopState().then((data) => { if (!gone) setTrack(data); }).catch(() => {});
    void import("@tauri-apps/api/event").then(async ({ listen }) => {
      const unlisten = await listen<DesktopTrack>("desktop-state", (event) => {
        if (!gone) setTrack(event.payload);
      });
      if (gone) unlisten();
      else stop = unlisten;
    }).catch(() => {});
    return () => {
      gone = true;
      stop();
    };
  }, []);
  useEffect(() => {
    if (!track) return;
    document.documentElement.dataset.theme = track.theme === "light" ? "light" : "dark";
    const lang = track.language === "zh" ? "zh" : "en";
    if (i18n.language !== lang) void i18n.changeLanguage(lang);
  }, [track, i18n]);

  const command = (action: string) => void api.desktopCommand(action).catch(() => {});
  const restore = useDragOrClick(() => command("restore"));
  const restoreKey = (event: ReactKeyboardEvent) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    command("restore");
  };
  const title = track?.title || t("player.idle");
  const cover = track?.coverPath ? coverSrc(track.coverPath) : "";
  return (
    <div
      className="mini-player"
      onPointerDown={(event) => {
        if (event.button !== 0 || event.target !== event.currentTarget) return;
        void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => getCurrentWindow().startDragging()).catch(() => {});
      }}
    >
      <div className="mini-open" role="button" tabIndex={0} aria-label={t("now.open")} {...restore} onKeyDown={restoreKey}>
        {cover ? <img src={cover} alt="" /> : <Icons.music />}
      </div>
      <div className="mini-text" role="button" tabIndex={0} {...restore} onKeyDown={restoreKey}>
        <span className="mini-title">{title}</span>
        <span className="mini-artist">{track?.artist || t("library.unknownArtist")}</span>
      </div>
      <div className="mini-transport">
        <button type="button" className="mini-ctrl" aria-label={t("player.previous")} title={t("player.previous")} onClick={() => command("previous")} disabled={!track?.title}>
          <Icons.prev />
        </button>
        <button type="button" className="mini-ctrl play" aria-label={track?.playing ? t("player.pause") : t("player.play")} title={track?.playing ? t("player.pause") : t("player.play")} onClick={() => command("toggle")} disabled={!track?.title}>
          {track?.playing ? <Icons.pause /> : <Icons.play />}
        </button>
        <button type="button" className="mini-ctrl" aria-label={t("player.next")} title={t("player.next")} onClick={() => command("next")} disabled={!track?.title}>
          <Icons.next />
        </button>
      </div>
      <button type="button" className={`mini-ctrl like${track?.favorite ? " on" : ""}`} aria-label={track?.favorite ? t("menu.unlike") : t("menu.like")} title={track?.favorite ? t("menu.unlike") : t("menu.like")} onClick={() => command("like")} disabled={!track?.title}>
        {track?.favorite ? <Icons.heartFill /> : <Icons.heart />}
      </button>
      <button type="button" className="mini-close" aria-label={t("mini.close")} title={t("mini.close")} onClick={() => command("closeMini")}>
        <Icons.close />
      </button>
    </div>
  );
}
