// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, pickFolder, type Feature, type Grant } from "../api";
import { useApp } from "../app/model";
import { CommitInput, Revise, Row, Segmented, Select, Switch } from "../components/controls";
import { SyncButton } from "../components/SyncButton";
import { problemGroups } from "../lib/library";
import { Icons } from "../ui/icons";

const SOURCE_URL = "https://github.com/zhebuilds/TunePeerly";
const LICENSE_URL = "https://www.gnu.org/licenses/agpl-3.0.html";
const TERMS_URL = `${SOURCE_URL}/blob/main/ADDITIONAL_TERMS.md`;
const FALLBACK_VERSION = "0.6.0";

function openLink(url: string) {
  if ("__TAURI_INTERNALS__" in window) {
    void import("@tauri-apps/plugin-opener")
      .then(({ openUrl }) => openUrl(url))
      .catch(() => { });
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

const FEATURES: Feature[] = ["favorites", "appearance"];

export function SettingsPage() {
  const { t } = useTranslation();
  const app = useApp();
  const boot = app.boot;
  const refs = useRef<Record<string, HTMLElement | null>>({});
  const [nameEpoch, setNameEpoch] = useState(0);
  useEffect(() => {
    if (app.route.name !== "settings" || !app.route.id) return;
    const id = app.route.id;
    // Wait a frame: the app scrolls back to the top on every route change after this effect runs.
    const frame = window.requestAnimationFrame(() => refs.current[id]?.scrollIntoView({ behavior: "smooth", block: "start" }));
    return () => window.cancelAnimationFrame(frame);
  }, [app.route.name, app.route.id]);
  if (!boot) return null;
  const settings = app.settings;
  const set = (key: string, value: unknown) => void app.setSetting(key, value);
  const groups = problemGroups(app.tracks);
  const problems = groups.access.length + groups.playback.length;
  const net = boot.network;
  const rename = (value: string) => {
    const name = value.trim().slice(0, 64);
    // A blank name is not saved; remounting the field puts the current name back.
    if (!name) {
      setNameEpoch((epoch) => epoch + 1);
      return;
    }
    void app.setSetting("displayName", name).then(app.scheduleRefresh);
  };

  const sections = [
    { id: "appearance", label: t("settings.appearance") },
    { id: "library", label: t("settings.library") },
    { id: "sync", label: t("settings.sync") },
    { id: "history", label: t("settings.history") },
    { id: "notices", label: t("settings.notices") },
    { id: "device", label: t("settings.thisDevice") },
    { id: "about", label: t("settings.about") },
  ];

  return (
    <div className="page settings-page">
      <h1 className="page-title">{t("nav.settings")}</h1>
      <nav className="chip-row sticky-chips" aria-label={t("nav.settings")}>
        {sections.map((section) => (
          <button type="button" key={section.id} className="chip" onClick={() => refs.current[section.id]?.scrollIntoView({ behavior: "smooth", block: "start" })}>
            {section.label}
          </button>
        ))}
      </nav>

      <section className="panel" ref={(node) => { refs.current.appearance = node; }}>
        <div className="panel-head">
          <h2>{t("settings.appearance")}</h2>
          <SyncButton feature="appearance" compact />
        </div>
        <Row title={t("settings.theme")}>
          <Segmented
            label={t("settings.theme")}
            value={boot.theme === "light" ? "light" : "dark"}
            onChange={(value) => set("theme", value)}
            options={[
              { value: "light", label: t("settings.light"), icon: <Icons.sun /> },
              { value: "dark", label: t("settings.dark"), icon: <Icons.moon /> },
            ]}
          />
        </Row>
        <Row title={t("settings.miniPlayer")} help={t("settings.miniPlayerHelp")}>
          <Switch label={t("settings.miniPlayer")} checked={Boolean(settings.miniPlayer)} onChange={(value) => set("miniPlayer", value)} />
        </Row>
        <Row title={t("settings.miniTray")} help={t("settings.miniTrayHelp")}>
          <Switch label={t("settings.miniTray")} checked={settings.miniTray !== false} onChange={(value) => set("miniTray", value)} />
        </Row>
        <Row title={t("settings.closeWindow")} help={t("settings.closeWindowHelp")}>
          <Segmented
            label={t("settings.closeWindow")}
            value={settings.closeAction === "tray" || settings.closeAction === "quit" ? String(settings.closeAction) : "ask"}
            onChange={(value) => set("closeAction", value === "ask" ? "" : value)}
            options={[
              { value: "ask", label: t("settings.closeAsk") },
              { value: "tray", label: t("settings.closeTray") },
              { value: "quit", label: t("settings.closeQuit") },
            ]}
          />
        </Row>
        <Row title={t("settings.language")}>
          <Segmented
            label={t("settings.language")}
            value={boot.language === "zh" ? "zh" : "en"}
            onChange={(value) => set("language", value)}
            options={[
              { value: "en", label: "English" },
              { value: "zh", label: "中文" },
            ]}
          />
        </Row>
      </section>

      <section className="panel" ref={(node) => { refs.current.library = node; }}>
        <h2>{t("settings.library")}</h2>
        <p className="setting-help">{t("settings.playbackHelp")}</p>
        <div className="grant-list">
          {boot.grants.length === 0 && <p className="muted">{t("settings.noFolders")}</p>}
          {boot.grants.map((grant) => (
            <GrantRow key={grant.id} grant={grant} />
          ))}
        </div>
        <div className="button-row">
          <button type="button" className="btn primary" onClick={() => void app.addMusic("directory")}>
            <Icons.folder /> {t("library.addFolder")}
          </button>
          <button type="button" className="btn soft" onClick={() => void app.addMusic("file")}>
            <Icons.music /> {t("library.addFiles")}
          </button>
          <button type="button" className="btn ghost" onClick={() => void app.rescan()}>
            <Icons.sync /> {t("library.rescan")}
          </button>
        </div>
        <Row title={t("settings.showUnavailable")} help={t("settings.showUnavailableHelp")}>
          <Switch label={t("settings.showUnavailable")} checked={settings.showUnavailable !== false} onChange={(value) => set("showUnavailable", value)} />
        </Row>
        <Row title={t("settings.exposeDefault")} help={t("settings.exposeDefaultHelp")}>
          <Select
            label={t("settings.exposeDefault")}
            value={settings.exposeDefault === "expose" ? "expose" : "hide"}
            onChange={(value) => set("exposeDefault", value)}
            options={[
              { value: "hide", label: t("exposure.hideDefault"), hint: t("exposure.hideDefaultHint") },
              { value: "expose", label: t("exposure.exposeDefault"), hint: t("exposure.exposeDefaultHint") },
            ]}
          />
        </Row>
        <Row title={t("settings.problems")} help={t("settings.problemsHelp")}>
          <button type="button" className="btn soft" onClick={() => app.navigate({ name: "problems" })}>
            {problems > 0 && <span className="count-badge">{problems}</span>}
            {t("settings.openProblems")} <Icons.chevronRight />
          </button>
        </Row>
        <Row title={t("settings.downloadFolder")} help={settings.downloadFolder ? <span className="mono">{settings.downloadFolder}</span> : t("settings.downloadFolderNone")}>
          <button
            type="button"
            className="btn soft"
            onClick={() =>
              void pickFolder()
                .then((folder) => {
                  if (folder) set("downloadFolder", folder);
                })
                .catch(app.say)
            }
          >
            <Icons.folder /> {t("common.choose")}
          </button>
        </Row>
      </section>

      <section className="panel" ref={(node) => { refs.current.sync = node; }}>
        <h2>{t("settings.sync")}<Revise /></h2>
        <p className="setting-help">{t("settings.syncHelp")}</p>
        <p className="setting-help">{t("settings.overwritePolicyHelp")}</p>
        <Row title={t("settings.requestDevice")} help={t("settings.requestDeviceHelp")}>
          <Select
            label={t("settings.requestDevice")}
            value={String(settings.syncRequestDevice ?? "")}
            onChange={(value) => set("syncRequestDevice", value)}
            options={[{ value: "", label: t("sync.requestNone") }, ...boot.links.map((link) => ({ value: link.deviceId, label: link.name || link.deviceId }))]}
          />
        </Row>
        {FEATURES.map((feature) => {
          const policy = settings.overwritePolicy?.[feature];
          return (
            <Row key={feature} title={t(`feature.${feature}`)} help={t("settings.overwritePolicy", { feature: t(`feature.${feature}`) })}>
              <div className="sync-feature-controls">
                <Select
                  label={t("settings.overwritePolicy", { feature: t(`feature.${feature}`) })}
                  value={policy === "ask" || policy === "deny" ? policy : "silent"}
                  onChange={(value) => set("overwritePolicy", { ...(settings.overwritePolicy ?? {}), [feature]: value })}
                  options={[
                    { value: "silent", label: t("devices.overwriteSilent") },
                    { value: "ask", label: t("devices.overwriteAsk") },
                    { value: "deny", label: t("devices.overwriteDeny") },
                  ]}
                />
                <SyncButton feature={feature} compact />
              </div>
            </Row>
          );
        })}
      </section>

      <section className="panel" ref={(node) => { refs.current.history = node; }}>
        <h2>{t("settings.history")}</h2>
        <Row title={t("settings.historyCap")} help={t("settings.historyCapHelp")}>
          <CommitInput
            label={t("settings.historyCap")}
            type="number"
            width={110}
            value={String(settings.historyCap ?? 100)}
            onCommit={(value) => {
              const cap = Math.round(Number(value));
              if (Number.isFinite(cap) && cap >= 1 && cap <= 10000) set("historyCap", cap);
              else app.setNotice(t("settings.historyCapInvalid"));
            }}
          />
        </Row>
        <Row title={t("settings.historyPause")} help={t("settings.historyPauseHelp")}>
          <Switch label={t("settings.historyPause")} checked={Boolean(settings.historyPaused)} onChange={(value) => set("historyPaused", value)} />
        </Row>
        <Row title={t("settings.incognito")} help={t("settings.incognitoHelp")}>
          <Switch label={t("settings.incognito")} checked={Boolean(settings.incognito)} onChange={(value) => set("incognito", value)} />
        </Row>
        <Row title={t("settings.silence")} help={t("settings.silenceHelp")}>
          <CommitInput
            label={t("settings.silence")}
            value={String(settings.silenceSeconds ?? 60)}
            width={120}
            onCommit={(value) => {
              const seconds = Number(value);
              if (!Number.isInteger(seconds) || seconds < 1 || seconds > 86400) {
                app.say(t("settings.silenceInvalid"));
                return;
              }
              void app.setSetting("silenceSeconds", seconds);
            }}
          />
        </Row>
      </section>

      <section className="panel" ref={(node) => { refs.current.notices = node; }}>
        <h2>{t("settings.notices")}</h2>
        <Row title={t("settings.noticeRead")} help={t("settings.noticeReadHelp")}>
          <Select
            label={t("settings.noticeRead")}
            value={settings.noticeRead === "open" || settings.noticeRead === "click" ? settings.noticeRead : "visible"}
            onChange={(value) => set("noticeRead", value)}
            options={[
              { value: "visible", label: t("settings.noticeReadVisible") },
              { value: "open", label: t("settings.noticeReadOpen") },
              { value: "click", label: t("settings.noticeReadClick") },
            ]}
          />
        </Row>
        <Row title={t("settings.noticeClear")} help={t("settings.noticeClearHelp")}>
          <Select
            label={t("settings.noticeClear")}
            value={settings.noticeClear === "unpinned" || settings.noticeClear === "all" ? settings.noticeClear : "ask"}
            onChange={(value) => set("noticeClear", value)}
            options={[
              { value: "ask", label: t("settings.noticeClearAsk") },
              { value: "unpinned", label: t("settings.noticeClearUnpinned") },
              { value: "all", label: t("settings.noticeClearAll") },
            ]}
          />
        </Row>
      </section>

      <section className="panel" ref={(node) => { refs.current.device = node; }}>
        <h2>{t("settings.thisDevice")}</h2>
        {settings.incognito && (
          <div className="banner warn">
            <Icons.incognito /> <span>{t("settings.networkIncognito")}</span>
          </div>
        )}
        <p className="setting-help">{t("settings.thisDeviceHelp")}</p>
        <Row title={t("devices.name")} help={t("devices.nameHelp")}>
          <CommitInput key={nameEpoch} label={t("devices.name")} value={boot.deviceName} width={260} onCommit={rename} />
        </Row>
        <Row title={<span className="setting-title-line">{t("settings.lanPort")}<Revise /></span>} help={t("settings.lanPortHelp")}>
          <code className="mono small selectable" data-testid="lan-port">{net?.tcpPort || "—"}</code>
        </Row>
        <Row title={t("settings.discoveryUdp")}>
          <code className="mono small selectable" data-testid="discovery-port">{net?.discovery ? net.udpPort : t("settings.discoveryOff")}</code>
        </Row>
        <Row title={t("settings.discoveryGroup")}>
          <code className="mono small selectable" data-testid="discovery-group">{net?.discovery ? net.multicastGroup : t("settings.discoveryOff")}</code>
        </Row>
        <Row title={t("settings.lanAddresses")} help={t("settings.lanAddressesHelp")}>
          <code className="mono small selectable" data-testid="lan-addresses">{net?.discovery && net.addresses.length > 0 ? net.addresses.join(", ") : "—"}</code>
        </Row>
        <Row title={<span className="setting-title-line">{t("settings.deviceId")}<Revise /></span>}>
          <code className="mono small selectable">{boot.certId}</code>
        </Row>
      </section>

      <AboutPanel nodeRef={(node) => { refs.current.about = node; }} />
    </div>
  );
}

function AboutPanel({ nodeRef }: { nodeRef: (node: HTMLElement | null) => void }) {
  const { t } = useTranslation();
  const [version, setVersion] = useState(FALLBACK_VERSION);
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let gone = false;
    void import("@tauri-apps/api/app")
      .then(({ getVersion }) => getVersion())
      .then((value) => {
        if (!gone && value) setVersion(value);
      })
      .catch(() => { });
    return () => {
      gone = true;
    };
  }, []);
  return (
    <section className="panel" ref={nodeRef}>
      <h2>{t("settings.about")}</h2>
      <div className="about-head">
        <img className="about-icon" src="/favicon.png" alt="" />
        <div>
          <div className="about-name">{t("about.name")}</div>
          <div className="about-version">{t("about.version", { version })}</div>
        </div>
      </div>
      <p className="setting-help about-summary">{t("about.summary")}</p>
      <h3>{t("about.credits")}</h3>
      <p className="setting-help">{t("about.creditsOriginal")}</p>
      <p className="setting-help">{t("about.creditsModify")}</p>
      <Row title={t("about.license")} help={t("about.licenseHelp")}>
        <button type="button" className="btn soft" onClick={() => openLink(LICENSE_URL)}>
          {t("about.licenseName")} <Icons.external />
        </button>
      </Row>
      <Row title={t("about.additionalTerms")}>
        <button type="button" className="btn soft" onClick={() => openLink(TERMS_URL)}>
          {t("about.open")} <Icons.external />
        </button>
      </Row>
      <Row
        title={t("about.source")}
        help={
          <>
            <div className="mono selectable about-url">{SOURCE_URL}</div>
            <div>{t("about.sourceNote")}</div>
          </>
        }
      >
        <button type="button" className="btn soft" onClick={() => openLink(SOURCE_URL)}>
          {t("about.open")} <Icons.external />
        </button>
      </Row>
      <Row title={t("about.author")} help={t("about.copyright")} />
      <Row title={t("about.privacy")} help={t("about.privacyHelp")} />
      <Row title={t("about.components")} help={t("about.componentsHelp")} />
    </section>
  );
}

function GrantRow({ grant }: { grant: Grant }) {
  const { t } = useTranslation();
  const app = useApp();
  const same = (a: string, b: string) => a.replace(/[\\/]+$/, "").toLowerCase() === b.replace(/[\\/]+$/, "").toLowerCase();
  const inside = (path: string, folder: string) => {
    const base = folder.replace(/[\\/]+$/, "").toLowerCase();
    const lower = path.toLowerCase();
    return lower.startsWith(base) && (lower[base.length] === "\\" || lower[base.length] === "/");
  };
  const count = app.tracks.filter((track) => track.absolutePath && (grant.kind === "file" ? same(track.absolutePath, grant.path) : inside(track.absolutePath, grant.path))).length;
  const refresh = () => app.scheduleRefresh();
  return (
    <div className="grant-row">
      <span className="grant-icon">{grant.kind === "directory" ? <Icons.folder /> : <Icons.music />}</span>
      <div className="grant-text">
        <div className="grant-path mono" title={grant.path}>
          {grant.path}
        </div>
        <div className="setting-help">
          {grant.kind === "directory" ? t("list.songCount", { count }) : t("settings.singleFile")} · {t("settings.exposeNowHelp")}
        </div>
      </div>
      <Select
        label={t("playlist.exposureDefault")}
        value={grant.exposure}
        onChange={(value) => void api.setExposure("grant", String(grant.id), value).then(refresh).catch(app.say)}
        options={[
          { value: "follow", label: t("exposure.follow"), hint: t("exposure.followHint") },
          { value: "expose", label: t("exposure.expose"), hint: t("exposure.exposeHint") },
          { value: "hide", label: t("exposure.hide"), hint: t("exposure.hideHint") },
        ]}
      />
      <button type="button" className="icon-btn" title={`${t("exposure.exposeAll")}: ${t("exposure.exposeAllHint")}`} aria-label={t("exposure.exposeAll")} onClick={() => void api.exposeMembers("grant", String(grant.id), true).then(() => { app.toast(t("toast.exposed")); refresh(); }).catch(app.say)}>
        <Icons.eye />
      </button>
      <button type="button" className="icon-btn" title={`${t("exposure.hideAll")}: ${t("exposure.hideAllHint")}`} aria-label={t("exposure.hideAll")} onClick={() => void api.exposeMembers("grant", String(grant.id), false).then(() => { app.toast(t("toast.hidden")); refresh(); }).catch(app.say)}>
        <Icons.eyeOff />
      </button>
      <button
        type="button"
        className="icon-btn danger"
        title={t("settings.removeFolder")}
        aria-label={t("settings.removeFolder")}
        onClick={() =>
          app.setDialog({
            kind: "confirm",
            title: t("settings.removeFolderTitle"),
            body: t("settings.removeFolderBody", { path: grant.path }),
            action: t("common.remove"),
            danger: true,
            run: () => void api.removeGrant(grant.id).then(refresh).catch(app.say),
          })
        }
      >
        <Icons.trash />
      </button>
    </div>
  );
}
