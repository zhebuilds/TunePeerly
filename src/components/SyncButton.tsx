// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type Feature } from "../api";
import { useApp } from "../app/model";
import { Icons } from "../ui/icons";
import { useBackClose } from "../lib/back";

type Menu = "pull" | "push";

/** Two actions: send this computer's copy to one or every connected device, or replace it with one
 * connected device's copy. Nothing moves on its own. */
export function SyncButton({ feature, compact, quiet }: { feature: Feature; compact?: boolean; quiet?: boolean }) {
  const { t } = useTranslation();
  const app = useApp();
  const boot = app.boot;
  const settings = app.settings;
  const [open, setOpen] = useState<Menu | null>(null);
  useBackClose(open !== null, () => setOpen(null));
  const [pos, setPos] = useState<{ left: number; top: number }>({ left: 0, top: 0 });
  const root = useRef<HTMLDivElement>(null);
  const online = (boot?.links ?? []).filter((link) => link.state === "online" && link.deviceId !== boot?.deviceId);
  const incognito = Boolean(settings.incognito);
  const unavailable = !boot || incognito || online.length === 0;
  const reason = incognito ? t("sync.pushIncognito") : online.length === 0 ? t("sync.noneOnline") : "";
  const pushTitle = reason || t("sync.pushTitle", { feature: t(`feature.${feature}`) });
  const pullTitle = reason || t("sync.pullTitle", { feature: t(`feature.${feature}`) });
  const buttonClass = compact ? "btn soft small" : quiet ? "link-btn" : "btn ghost";

  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(null);
    };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  }, [open]);

  const toggle = (menu: Menu, rect: DOMRect) => {
    const rows = online.length + (menu === "push" ? 1 : 0);
    const height = 52 + rows * 38;
    const below = rect.bottom + 6 + height < window.innerHeight;
    setPos({ left: Math.max(8, Math.min(rect.left, window.innerWidth - 236)), top: below ? rect.bottom + 6 : Math.max(8, rect.top - 6 - height) });
    setOpen((value) => (value === menu ? null : menu));
  };

  const request = (deviceId: string, name: string) => {
    setOpen(null);
    void api.network("request-sync", { feature, deviceId }).then(() => app.toast(t("toast.requested", { name }))).catch(app.say);
  };

  const send = (deviceId: string | null, name: string) => {
    setOpen(null);
    void api
      .network("sync", deviceId ? { feature, deviceId } : { feature })
      .then(() => app.toast(deviceId ? t("toast.sentTo", { name }) : t("toast.synced")))
      .catch(app.say);
  };

  return (
    <div className={`sync-actions${compact ? " compact" : ""}`} ref={root}>
      <button type="button" className={buttonClass} disabled={unavailable} title={pullTitle} aria-label={pullTitle} aria-expanded={open === "pull"} onClick={(event) => toggle("pull", event.currentTarget.getBoundingClientRect())}>
        <Icons.download />
        {compact ? t("sync.pull") : <span className="btn-label">{t("sync.pull")}</span>}
      </button>
      {open === "pull" && (
        <div className="sync-menu" style={{ left: pos.left, top: pos.top }} role="menu" aria-label={t("sync.pullChoose")}>
          <div className="sync-menu-title">{t("sync.pullChoose")}</div>
          {online.map((link) => (
            <button type="button" role="menuitem" key={link.deviceId} onClick={() => request(link.deviceId, link.name || link.deviceId)}>
              <span>{link.name || link.deviceId}</span>
            </button>
          ))}
        </div>
      )}
      <button type="button" className={buttonClass} disabled={unavailable} title={pushTitle} aria-label={pushTitle} aria-expanded={open === "push"} onClick={(event) => toggle("push", event.currentTarget.getBoundingClientRect())}>
        <Icons.broadcast />
        {compact ? t("sync.push") : <span className="btn-label">{t("sync.push")}</span>}
      </button>
      {open === "push" && (
        <div className="sync-menu" style={{ left: pos.left, top: pos.top }} role="menu" aria-label={t("sync.pushChoose")}>
          <div className="sync-menu-title">{t("sync.pushChoose")}</div>
          <button type="button" role="menuitem" onClick={() => send(null, "")}>
            <span>{t("sync.pushAll")}</span>
            <span className="muted small">{online.length}</span>
          </button>
          {online.map((link) => (
            <button type="button" role="menuitem" key={link.deviceId} onClick={() => send(link.deviceId, link.name || link.deviceId)}>
              <span>{link.name || link.deviceId}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
