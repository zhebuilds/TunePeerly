// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type Link } from "../api";
import { useApp } from "../app/model";
import { CommitInput, Revise, Row, Select } from "../components/controls";
import { Icons } from "../ui/icons";

function ipv4(text: string): boolean {
  const parts = text.trim().split(".");
  return parts.length === 4 && parts.every((part) => /^\d{1,3}$/.test(part) && Number(part) <= 255);
}

const RELAY_FAILURES = ["need_ipv4", "dns", "io", "tls", "missing_cert", "cert_mismatch", "unexpected"];

export function relayLabel(relay: string, t: (key: string, options?: Record<string, unknown>) => string): { text: string; tone: string } {
  if (!relay) return { text: t("relay.off"), tone: "offline" };
  if (relay === "registered") return { text: t("relay.registered"), tone: "online" };
  if (relay === "connecting") return { text: t("relay.connecting"), tone: "pending" };
  if (relay === "badId") return { text: t("relay.badId"), tone: "failed" };
  if (relay === "paused:incognito") return { text: t("relay.pausedIncognito"), tone: "pending" };
  const reason = relay.replace(/^failed:/, "");
  if (RELAY_FAILURES.includes(reason)) return { text: t(`relay.fail.${reason}`), tone: "failed" };
  return { text: t("relay.failed", { reason }), tone: "failed" };
}

export function DevicesPage() {
  const { t } = useTranslation();
  const app = useApp();
  const boot = app.boot;
  const [host, setHost] = useState("");
  const [port, setPort] = useState("");
  if (!boot) return null;

  const settings = app.settings;
  const networkOff = Boolean(settings.incognito);
  const pairedIds = new Set(boot.links.map((link) => link.deviceId));
  const nearby = boot.nearby.filter((item) => !pairedIds.has(item.deviceId) || boot.links.find((link) => link.deviceId === item.deviceId)?.state !== "online");
  const relay = relayLabel(boot.relay, t);
  const validHost = ipv4(host);
  const portNumber = Number(port);
  const validPort = Number.isInteger(portNumber) && portNumber > 0 && portNumber < 65536;

  const run = (action: string, payload: unknown = null) => void api.network(action, payload).catch(app.say);
  const connect = (target: string) => {
    const [address, rawPort] = target.split(":");
    setHost(address);
    setPort(rawPort ?? "");
    run("manual", { host: address, port: Number(rawPort) });
  };
  const remove = (link: Link) =>
    app.setDialog({
      kind: "confirm",
      title: t("devices.removeTitle", { name: link.name }),
      body: t("devices.removeBody"),
      action: t("devices.remove"),
      danger: true,
      run: () => run("remove", link.deviceId),
    });

  return (
    <div className="page settings-page devices-page">
      <h1 className="page-title">{t("nav.devices")}</h1>

      {networkOff && (
        <div className="banner warn">
          <Icons.warning />
          <span>{settings.incognito ? t("devices.incognitoOff") : t("devices.networkOff")}</span>
          <button type="button" className="btn soft small" onClick={() => app.navigate({ name: "settings" })}>
            {t("nav.settings")}
          </button>
        </div>
      )}

      <section className="panel">
        <h2>{t("devices.thisDevice")}</h2>
        <Row title={t("devices.name")} help={t("devices.nameHelp")}>
          <CommitInput label={t("devices.name")} value={boot.deviceName} width={260} onCommit={(value) => void app.setSetting("displayName", value.trim().slice(0, 64)).then(app.scheduleRefresh)} />
        </Row>
        <Row title={t("devices.status")}>
          <div className="pill-row">
            <span className="state-pill online">{t("devices.port", { port: boot.listenPort || "—" })}</span>
            <span className={`state-pill ${relay.tone}`} title={boot.relay}>
              {relay.text}
            </span>
          </div>
        </Row>
        <Row title={t("devices.addresses")} help={t("devices.addressesHelp")}>
          <div className="address-list">
            {(boot.localAddresses ?? []).length === 0 ? (
              <span className="muted small">{t("devices.noAddresses")}</span>
            ) : (
              (boot.localAddresses ?? []).map((address) => (
                <span key={address} className="chip mono static">
                  {address}
                </span>
              ))
            )}
          </div>
        </Row>
        <div className="setting-text">
          <div className="setting-title">{t("devices.pairLan")}</div>
          <div className="setting-help">{t("devices.pairLanHelp")}</div>
        </div>
      </section>

      <section className="panel">
        <h2>{t("devices.paired")}<Revise /></h2>
        {boot.links.length === 0 ? (
          <p className="muted">{t("devices.noPaired")}</p>
        ) : (
          <ul className="device-list">
            {boot.links.map((link) => (
              <li key={link.deviceId} className="device-item">
                <span className={`device-dot ${link.state}`} />
                <div className="device-text">
                  <button type="button" className="device-name" onClick={() => app.navigate({ name: "device", id: link.deviceId })}>
                    {link.name}
                  </button>
                  <div className="device-sub">
                    {t(`link.${link.state}`)}
                    {link.path && ` · ${link.path === "relay" ? t("devices.viaRelay") : t("devices.viaLan")}`}
                    {link.state === "online" && ` · ${t("list.songCount", { count: app.tracks.filter((track) => track.holderIds.includes(link.deviceId)).length })}`}
                  </div>
                </div>
                <Select
                  label={t("devices.trust")}
                  value={link.trust === "verify" ? "verify" : "full"}
                  onChange={(value) => run("trust", { deviceId: link.deviceId, trust: value })}
                  options={[
                    { value: "full", label: t("devices.trustFull") },
                    { value: "verify", label: t("devices.trustVerify") },
                  ]}
                />
                {link.state === "online" ? (
                  <button type="button" className="btn ghost small" onClick={() => run("disconnect", link.deviceId)}>
                    {t("devices.disconnect")}
                  </button>
                ) : (
                  <button type="button" className="btn primary small" onClick={() => run("pair", link.deviceId)}>
                    {t("devices.connect")}
                  </button>
                )}
                <button type="button" className="btn ghost small" onClick={() => app.navigate({ name: "device", id: link.deviceId })}>
                  {t("devices.browse")}
                </button>
                <button type="button" className="icon-btn" aria-label={t("devices.remove")} title={t("devices.remove")} onClick={() => remove(link)}>
                  <Icons.trash />
                </button>
                <button type="button" className="btn ghost small danger-text" onClick={() => run("block", link.deviceId)}>
                  {t("devices.block")}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2>
          {t("devices.nearby")}
          <Revise />
          {settings.discovery && !networkOff && <span className="scanning" aria-hidden="true" />}
        </h2>
        {!settings.discovery ? (
          <p className="muted">{t("devices.discoveryOff")}</p>
        ) : nearby.length === 0 ? (
          <p className="muted">{t("devices.noNearby")}</p>
        ) : (
          <ul className="device-list">
            {nearby.map((item) => (
              <li key={item.deviceId} className="device-item">
                <span className="device-dot nearby" />
                <div className="device-text">
                  <div className="device-name static">{item.name || item.deviceId.slice(0, 7)}</div>
                  <div className="device-sub">
                    {t("devices.addresses")}
                    {boot.failures?.[item.deviceId] ? ` · ${t("devices.unreachable")}` : ""}
                  </div>
                  <div className="address-list">
                    {item.hosts.map((address) => (
                      <button
                        type="button"
                        key={address}
                        className="chip mono"
                        title={t("devices.pairVia", { address })}
                        onClick={() => run("pair", { deviceId: item.deviceId, host: address })}
                      >
                        {address}:{item.port}
                      </button>
                    ))}
                  </div>
                </div>
                <button type="button" className="btn ghost small danger-text" aria-label={t("devices.block")} title={t("devices.block")} onClick={() => run("block", item.deviceId)}>
                  {t("devices.block")}
                </button>
                <button type="button" className="btn primary small" onClick={() => run("pair", item.deviceId)}>
                  <Icons.link /> {t("devices.pair")}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2>{t("devices.manual")}<Revise /></h2>
        <p className="setting-help">{t("devices.manualHelp")}</p>
        <form
          className="manual-form"
          onSubmit={(event) => {
            event.preventDefault();
            if (validHost && validPort) run("manual", { host: host.trim(), port: portNumber });
          }}
        >
          <input className={`input${host && !validHost ? " invalid" : ""}`} value={host} placeholder="192.168.1.20" aria-label={t("devices.address")} onChange={(event) => setHost(event.target.value)} />
          <input className={`input port${port && !validPort ? " invalid" : ""}`} value={port} placeholder="22000" inputMode="numeric" aria-label={t("devices.portLabel")} onChange={(event) => setPort(event.target.value.replace(/\D/g, ""))} />
          <button type="submit" className="btn primary" disabled={!validHost || !validPort || boot.manual === "dialing"}>
            {boot.manual === "dialing" ? t("devices.connecting") : t("devices.connect")}
          </button>
          {boot.manual && boot.manual !== "badText" && <span className={`state-text ${boot.manual}`}>{t(`manual.${boot.manual}`)}</span>}
        </form>
        {(settings.manualAddresses ?? []).length > 0 && (
          <div className="chip-row">
            <span className="muted small">{t("devices.recent")}</span>
            {(settings.manualAddresses ?? []).map((item) => (
              <button type="button" key={item} className="chip mono" onClick={() => connect(item)}>
                {item}
              </button>
            ))}
          </div>
        )}
      </section>

      <section className="panel">
        <h2>{t("devices.blocked")}</h2>
        <p className="setting-help">{t("devices.blockHelp")}</p>
        {(boot.blocks ?? []).length === 0 ? (
          <p className="muted">{t("devices.noBlocked")}</p>
        ) : (
          <ul className="device-list">
            {(boot.blocks ?? []).map((item) => (
              <li key={item.deviceId} className="device-item">
                <span className="device-dot offline" />
                <div className="device-text">
                  <div className="device-name static">{item.name || item.deviceId.slice(0, 7)}</div>
                </div>
                <button type="button" className="btn ghost small" onClick={() => run("unblock", item.deviceId)}>
                  {t("devices.unblock")}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}


