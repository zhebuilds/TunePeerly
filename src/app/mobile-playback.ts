// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

import { useEffect, useRef } from "react";
import { isAndroidApp } from "../api";
import { clock, transport } from "../player/clock";
import type { AppModel } from "./model";

type MediaAction = { action: string; positionMs: number };

/**
 * On Android the player is an audio element inside the web view, so the notification and the media
 * session live on the native side. This mirrors the player into the foreground service about once a
 * second and turns the notification buttons back into the app's own transport calls.
 *
 * On the desktop it does nothing: there the operating system media controls come from the platform
 * layer, not from here.
 */
export function useMobilePlayback(app: AppModel) {
  const appRef = useRef(app);
  appRef.current = app;

  useEffect(() => {
    if (!isAndroidApp()) return;
    let disposed = false;
    let signature = "";
    let lastSentAt = 0;

    const push = (force: boolean) => {
      if (disposed) return;
      const model = appRef.current;
      const track = model.shown;
      const playing = transport.get().playing;
      const time = clock.get();
      const title = track?.title ?? "";
      const artist = track?.artist ?? "";
      const next = `${title}\u0000${artist}\u0000${playing}`;
      const now = Date.now();
      // A tick every second is enough for a progress bar; a change of song or state goes out at once.
      if (!force && next === signature && now - lastSentAt < 1000) return;
      signature = next;
      lastSentAt = now;
      void import("@tauri-apps/api/core")
        .then(({ invoke }) =>
          invoke("mobile_playback_update", {
            title,
            artist,
            playing,
            positionMs: Math.max(0, Math.round(time.position * 1000)),
            durationMs: Math.max(0, Math.round(time.duration * 1000)),
          })
        )
        .catch(() => undefined);
    };

    const stopClock = clock.subscribe(() => push(false));
    const stopTransport = transport.subscribe(() => push(true));
    push(true);

    let unlisten: (() => void) | null = null;
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<MediaAction>("mobile-media-action", (event) => {
          const model = appRef.current;
          const { action, positionMs } = event.payload;
          const playing = transport.get().playing;
          if (action === "play" && !playing) model.toggle();
          else if (action === "pause" && playing) model.toggle();
          else if (action === "next") model.next();
          else if (action === "previous") model.previous();
          else if (action === "seek") model.seek(positionMs / 1000);
          else if (action === "stop" && playing) model.toggle();
        })
      )
      .then((off) => {
        if (disposed) off();
        else unlisten = off;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      stopClock();
      stopTransport();
      unlisten?.();
    };
  }, []);

  // The title changes without a clock tick, so a new song is pushed right away.
  useEffect(() => {
    if (!isAndroidApp()) return;
    const model = appRef.current;
    const track = model.shown;
    void import("@tauri-apps/api/core")
      .then(({ invoke }) =>
        invoke("mobile_playback_update", {
          title: track?.title ?? "",
          artist: track?.artist ?? "",
          playing: transport.get().playing,
          positionMs: 0,
          durationMs: Math.max(0, track?.durationMs ?? 0),
        })
      )
      .catch(() => undefined);
  }, [app.shown?.contentId ?? app.shown?.localFileId ?? app.shown?.absolutePath]);
}
