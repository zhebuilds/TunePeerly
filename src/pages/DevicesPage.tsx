// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type Link } from "../api";
import { useApp } from "../app/model";
import { CommitInput, Revise, Row, Select, Switch } from "../components/controls";
import { unpairedNearby } from "../lib/devices";
import { Icons } from "../ui/icons";

export function relayLabel(relay: string, t: (key: string, options?: Record<string, unknown>) => string): { text: string; tone: string } {
  if (!relay) return { text: t("relay.off"), tone: "offline" };
  if (relay === "registered") return { text: t("relay.registered"), tone: "online" };
  if (relay === "connecting") return { text: t("relay.connecting"), tone: "pending" };
  if (relay === "badId") return { text: t("relay.badId"), tone: "failed" };
  return { text: t("relay.failed", { reason: relay.replace(/^failed:/, "") }), tone: "failed" };
}

export function DevicesPage() {
  const { t } = useTranslation();
  const app = useApp();
  const boot = app.boot;
  const [nameEpoch, setNameEpoch] = useState(0);
  const [refreshing, setRefreshing] = useState(false);
  if (!boot) return null;

  const settings = app.settings;
  const networkOff = Boolean(settings.incognito);
  // Nearby is only for devices that are not paired yet; a paired device stays under Paired even when it is offline.
  const nearby = unpairedNearby(boot.nearby, boot.links);
  const relay = relayLabel(boot.relay, t);

  const run = (action: string, payload: unknown = null) => void api.network(action, payload).catch(app.say);
  const remove = (link: Link) =>
    app.setDialog({
      kind: "confirm",
      title: t("devices.removeTitle", { name: link.name }),
      body: t("devices.removeBody"),
      action: t("devices.remove"),
      danger: true,
      run: () => run("remove", link.deviceId),
    });
  const block = (link: Link) =>
    app.setDialog({
      kind: "confirm",
      title: t("devices.blockTitle", { name: link.name }),
      body: t("devices.blockBody"),
      action: t("devices.block"),
      danger: true,
      run: () => run("block", link.deviceId),
    });
  // The backend limits how often announces go out, so pressing again only resets the short spinner.
  const refreshNearby = () => {
    setRefreshing(true);
    run("refresh-nearby");
    window.setTimeout(() => setRefreshing(false), 1000);
  };
  const rename = (value: string) => {
    const name = value.trim().slice(0, 64);
    // A blank name is not saved; remounting the field puts the current name back.
    if (!name) {
      setNameEpoch((epoch) => epoch + 1);
      return;
    }
    void app.setSetting("displayName", name).then(app.scheduleRefresh);
  };

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
          <CommitInput key={nameEpoch} label={t("devices.name")} value={boot.deviceName} width={260} onCommit={rename} />
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
                <button type="button" className="btn ghost small danger-text" onClick={() => block(link)}>
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
          <span className="panel-switch" title={t("devices.discoveryHelp")}>
            {settings.discovery && !networkOff && (
              <button
                type="button"
                className={`btn ghost small${refreshing ? " spinning" : ""}`}
                title={t("devices.refreshNearbyHelp")}
                disabled={refreshing}
                onClick={refreshNearby}
              >
                <Icons.sync /> {t("devices.refreshNearby")}
              </button>
            )}
            <span className="device-switch-label">{t("devices.discovery")}</span>
            <Switch label={t("devices.discovery")} checked={Boolean(settings.discovery)} onChange={(value) => void app.setSetting("discovery", value)} />
          </span>
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


