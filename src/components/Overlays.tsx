// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { api, pickFolder } from "../api";
import { useApp } from "../app/model";
import { Select } from "./controls";
import { Icons } from "../ui/icons";

export function Overlays({ dropping }: { dropping: boolean }) {
  return (
    <>
      <Toasts />
      <DialogHost />
      <OfferDialog />
      <MoveDialog />
      <DownloadWatcher />
      {dropping && <DropOverlay />}
    </>
  );
}

function Toasts() {
  const app = useApp();
  return (
    <div className="toasts" role="status" aria-live="polite">
      {app.toasts.map((toast) => (
        <div key={toast.id} className="toast">
          {toast.text}
        </div>
      ))}
    </div>
  );
}

const VISIBLE_BANNERS = 3;

export function Banners() {
  const { t } = useTranslation();
  const app = useApp();
  const pending = app.boot?.pending ?? [];
  const asks = app.overwriteAsks;
  const sourceName = (id: string | null) => (id ? app.boot?.links.find((link) => link.deviceId === id)?.name ?? id.slice(0, 7) : "");
  const rows = [
    ...asks.map((ask) => ({ key: `ask-${ask.id}`, kind: "ask" as const, ask })),
    ...pending.map((item) => ({ key: `conflict-${item.feature}`, kind: "conflict" as const, item })),
    ...app.alerts.map((item) => ({ key: `alert-${item.id}`, kind: "alert" as const, item })),
  ].slice(0, VISIBLE_BANNERS);
  if (rows.length === 0) return null;
  return (
    <div className="banners">
      {rows.map((row) => {
        if (row.kind === "ask") {
          const ask = row.ask;
          return (
            <div key={row.key} className="banner conflict" role="alert">
              <Icons.sync />
              <span>{t("overwrite.body", { feature: t(`feature.${ask.feature}`), device: ask.name || ask.deviceId.slice(0, 7), seconds: ask.remaining })}</span>
              <div className="button-row">
                <button type="button" className="btn ghost small" onClick={() => void api.network("resolve-overwrite", { id: ask.id, accept: false }).then(app.scheduleRefresh).catch(app.say)}>
                  {t("overwrite.keep")}
                </button>
                <button type="button" className="btn primary small" onClick={() => void api.network("resolve-overwrite", { id: ask.id, accept: true }).then(app.scheduleRefresh).catch(app.say)}>
                  {t("overwrite.take")}
                </button>
              </div>
            </div>
          );
        }
        if (row.kind === "conflict") {
          const item = row.item;
          return (
            <div key={row.key} className="banner conflict" role="alert">
              <Icons.sync />
              <span>{t("conflict.body", { feature: t(`feature.${item.feature}`), device: sourceName(item.source) })}</span>
              <div className="button-row">
                <button type="button" className="btn ghost small" onClick={() => void api.network("resolve", { feature: item.feature, take: false }).then(app.scheduleRefresh).catch(app.say)}>
                  {t("conflict.keep")}
                </button>
                <button type="button" className="btn primary small" onClick={() => void api.network("resolve", { feature: item.feature, take: true }).then(app.scheduleRefresh).catch(app.say)}>
                  {t("conflict.take")}
                </button>
              </div>
            </div>
          );
        }
        return <TimedAlert key={row.key} id={row.item.id} text={row.item.text} onClose={app.dismissAlert} />;
      })}
    </div>
  );
}

function TimedAlert({ id, text, onClose }: { id: number; text: string; onClose: (id: number) => void }) {
  const { t } = useTranslation();
  useEffect(() => {
    const timer = window.setTimeout(() => onClose(id), 10000);
    return () => window.clearTimeout(timer);
  }, [id, onClose]);
  return (
    <div className="banner notice" role="alert">
      <Icons.warning />
      <span>{text}</span>
      <button type="button" className="icon-btn small" aria-label={t("common.close")} onClick={() => onClose(id)}>
        <Icons.close />
      </button>
    </div>
  );
}

export function Modal({ children, onClose, label }: { children: ReactNode; onClose: () => void; label: string }) {
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const first = box.current?.querySelector<HTMLElement>("input, textarea, button.primary, button");
    first?.focus();
    if (first instanceof HTMLInputElement) first.select();
    return () => previous?.focus?.();
  }, []);
  return (
    <div
      className="modal-layer"
      onPointerDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={box}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label={label}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.stopPropagation();
            onClose();
          }
        }}
      >
        {children}
      </div>
    </div>
  );
}

function DialogHost() {
  const { t } = useTranslation();
  const app = useApp();
  const dialog = app.dialog;
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (dialog?.kind === "prompt") setValue(dialog.value);
    setBusy(false);
  }, [dialog]);
  if (!dialog) return null;
  const close = () => app.setDialog(null);
  const submit = async () => {
    if (busy) return;
    if (dialog.kind === "prompt" && !value.trim()) return;
    setBusy(true);
    try {
      if (dialog.kind === "prompt") await dialog.run(value.trim());
      else await dialog.run();
    } catch (error) {
      app.say(error);
    } finally {
      setBusy(false);
      close();
    }
  };
  return (
    <Modal onClose={close} label={dialog.title}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <h2 className="modal-title">{dialog.title}</h2>
        {dialog.kind === "confirm" ? (
          <p className="modal-body">{dialog.body}</p>
        ) : (
          <label className="field">
            <span>{dialog.label}</span>
            <input className="input" value={value} maxLength={120} onChange={(event) => setValue(event.target.value)} />
          </label>
        )}
        <div className="modal-actions">
          <button type="button" className="btn ghost" onClick={close}>
            {t("common.cancel")}
          </button>
          <button type="submit" className={`btn ${dialog.kind === "confirm" && dialog.danger ? "danger" : "primary"}`} disabled={busy || (dialog.kind === "prompt" && !value.trim())}>
            {dialog.action}
          </button>
        </div>
      </form>
    </Modal>
  );
}

const FINISHED = ["expired", "locked", "busy", "cancelled", "failed"];

function OfferDialog() {
  const { t } = useTranslation();
  const app = useApp();
  const offer = app.boot?.offer;
  const answering = useRef(false);
  const [trust, setTrust] = useState(() => (offer?.trust === "verify" ? "verify" : "full"));
  const [typed, setTyped] = useState("");
  const [now, setNow] = useState(() => Date.now());
  const deviceId = offer?.deviceId;
  const savedTrust = offer?.trust === "verify" ? "verify" : "full";
  const status = offer?.status ?? "";
  const expiresAt = offer?.expiresAt ?? null;
  useEffect(() => {
    // A saved device starts from the trust chosen for it before; a new one from full trust.
    setTrust(savedTrust);
    setTyped("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [deviceId]);
  useEffect(() => {
    // A wrong code is used up: clear the box for the new one.
    if (status === "wrong") setTyped("");
  }, [status]);
  useEffect(() => {
    if (!expiresAt) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [expiresAt]);
  if (!offer) return null;
  const role = offer.role ?? "incoming";
  const name = offer.name || offer.deviceId.slice(0, 7);
  const finished = FINISHED.includes(status);
  const checking = status === "checking";
  const digits = typed.replace(/\D/g, "").slice(0, 6);
  const send = (action: string, payload: unknown) => {
    if (answering.current) return;
    answering.current = true;
    void api.network(action, payload).then(app.scheduleRefresh).catch(app.say).finally(() => {
      answering.current = false;
    });
  };
  const close = () => send("reject", offer.deviceId);
  const chooseTrust = (value: string) => {
    setTrust(value);
    // Kept with the pairing until the device is saved.
    if (role === "incoming") void api.network("trust", { deviceId: offer.deviceId, trust: value }).catch(app.say);
  };
  const submit = () => {
    if (role === "approve") send("confirm", { deviceId: offer.deviceId, trust });
    else if (role === "outgoing" && digits.length === 6 && !checking) send("confirm", { deviceId: offer.deviceId, trust, code: digits });
  };
  const seconds = expiresAt ? Math.max(0, Math.ceil((expiresAt - now) / 1000)) : 0;
  const shown = offer.code ? `${offer.code.slice(0, 3)} ${offer.code.slice(3)}` : "";
  const title = role === "approve" ? t("offer.approveTitle") : role === "outgoing" ? t("offer.enterTitle", { name }) : t("offer.title");
  const message = (() => {
    if (role === "approve") return t("offer.approveBody", { name });
    if (finished) return t(`offer.${role === "outgoing" ? "asking" : "asked"}.${status}`, { name });
    if (role === "outgoing") {
      if (checking) return t("offer.checking");
      if (status === "wrong") return t("offer.asking.wrong", { name });
      if (status === "badCode") return t("offer.badCode");
      return t("offer.enterBody", { name });
    }
    if (checking) return t("offer.checking");
    if (status === "wrong") return t("offer.asked.wrong", { name, count: offer.triesLeft ?? 0 });
    return t("offer.showBody", { name });
  })();
  const problem = status === "wrong" || status === "badCode" || (finished && status !== "cancelled");
  return (
    <Modal onClose={() => (role !== "incoming" || finished) && close()} label={title}>
      <form
        className="offer"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <div className="offer-icon">
          <Icons.link />
        </div>
        <h2 className="modal-title">{title}</h2>
        <p className={`modal-body${problem ? " offer-problem" : ""}`} role={problem ? "alert" : undefined}>
          {message}
        </p>
        {role === "incoming" && shown && (
          <>
            <div className="pair-code mono" aria-label={t("offer.codeLabel")}>
              {shown}
            </div>
            <p className="setting-help">{t("offer.codeTime", { count: seconds })}</p>
          </>
        )}
        {role === "outgoing" && !finished && (
          <label className="field pair-code-field">
            <span>{t("offer.codeLabel")}</span>
            <input
              className="input pair-code-input mono"
              value={typed}
              inputMode="numeric"
              autoComplete="one-time-code"
              maxLength={7}
              placeholder="000000"
              disabled={checking}
              onChange={(event) => setTyped(event.target.value.replace(/[^\d ]/g, ""))}
            />
          </label>
        )}
        {!finished && (
          <Select
            label={t("devices.trust")}
            value={trust}
            onChange={chooseTrust}
            options={[
              { value: "full", label: t("devices.trustFull"), hint: t("offer.trustHelp") },
              { value: "verify", label: t("devices.trustVerify"), hint: t("offer.trustHelp") },
            ]}
          />
        )}
        <div className="modal-actions">
          {finished ? (
            <button type="button" className="btn primary" onClick={close}>
              {t("offer.close")}
            </button>
          ) : (
            <>
              <button type="button" className="btn ghost" onClick={close}>
                {role === "outgoing" ? t("offer.cancel") : t("offer.reject")}
              </button>
              {role === "outgoing" && (
                <button type="submit" className="btn primary" disabled={digits.length !== 6 || checking}>
                  {t("offer.pair")}
                </button>
              )}
              {role === "approve" && (
                <button type="submit" className="btn primary">
                  {t("offer.allow")}
                </button>
              )}
            </>
          )}
        </div>
      </form>
    </Modal>
  );
}

function MoveDialog() {
  const { t } = useTranslation();
  const app = useApp();
  const move = app.moves[0];
  if (!move) return null;
  const answer = (accept: boolean) => {
    app.setMoves((items) => items.slice(1));
    void api
      .moveMembership(move.previousContentId, move.nextContentId, accept)
      .then(() => accept && app.scheduleRefresh())
      .catch(app.say);
  };
  return (
    <Modal onClose={() => answer(false)} label={t("move.title")}>
      <h2 className="modal-title">{t("move.title")}</h2>
      <p className="modal-body">{t("move.body", { title: move.title })}</p>
      <div className="modal-actions">
        <button type="button" className="btn ghost" onClick={() => answer(false)}>
          {t("move.keep")}
        </button>
        <button type="button" className="btn primary" onClick={() => answer(true)}>
          {t("move.accept")}
        </button>
      </div>
    </Modal>
  );
}

function DownloadWatcher() {
  const { t } = useTranslation();
  const app = useApp();
  const download = app.downloadFeed?.current ?? app.boot?.download ?? null;
  const seen = useRef("");
  useEffect(() => {
    if (!download) return;
    // seq changes on every new request, so asking again after a cancel shows the dialog again.
    const key = `${download.seq ?? 0}:${download.contentId}:${download.state}:${download.detail}`;
    if (key === seen.current) return;
    seen.current = key;
    const track = app.index.get({ contentId: download.contentId });
    const title = track?.title || t("library.untitled");
    switch (download.state) {
      case "failed":
        app.setNotice(download.detail ? t(download.detail, { defaultValue: t("download.failed", { title }) }) : t("download.failed", { title }));
        break;
      case "need-folder":
        app.setDialog({
          kind: "confirm",
          title: t("download.needFolderTitle"),
          body: t("download.needFolderBody"),
          action: t("common.choose"),
          run: async () => {
            const folder = await pickFolder();
            if (!folder) return;
            await app.setSetting("downloadFolder", folder);
            await api.network("download", download.contentId).catch(app.say);
          },
        });
        break;
    }
  }, [download, app, t]);
  return null;
}

function DropOverlay() {
  const { t } = useTranslation();
  return (
    <div className="drop-overlay">
      <div className="drop-card">
        <Icons.download />
        <div className="drop-title">{t("drop.title")}</div>
        <div className="muted">{t("drop.body")}</div>
      </div>
    </div>
  );
}
