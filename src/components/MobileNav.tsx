// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useApp, type Route } from "../app/model";
import { Icons } from "../ui/icons";

/** The phone's tab bar. "Library" opens the sidebar's list of playlists, artists and folders as a sheet. */
export function MobileNav({ libraryOpen, onLibrary }: { libraryOpen: boolean; onLibrary: (open: boolean) => void }) {
  const { t } = useTranslation();
  const app = useApp();
  const name = app.route.name;
  const go = (route: Route) => {
    onLibrary(false);
    app.navigate(route);
  };
  const tab = (active: boolean, label: string, icon: ReactNode, onClick: () => void, badge?: string) => (
    <button type="button" className={`mobile-tab${active ? " active" : ""}`} aria-current={active ? "page" : undefined} onClick={onClick}>
      <span className="mobile-tab-icon">
        {icon}
        {badge && <span className="nav-badge">{badge}</span>}
      </span>
      <span className="mobile-tab-label">{label}</span>
    </button>
  );
  const home = !libraryOpen && name === "home";
  return (
    <nav className="mobile-nav" aria-label={t("nav.home")}>
      {tab(home, t("nav.home"), home ? <Icons.homeFill /> : <Icons.home />, () => go({ name: "home" }))}
      {tab(!libraryOpen && name === "search", t("nav.search"), <Icons.search />, () => go({ name: "search" }))}
      {tab(libraryOpen, t("sidebar.library"), <Icons.library />, () => onLibrary(!libraryOpen))}
      {tab(!libraryOpen && (name === "devices" || name === "device"), t("nav.devices"), <Icons.devices />, () => go({ name: "devices" }), app.boot?.offer ? "!" : undefined)}
      {tab(!libraryOpen && (name === "settings" || name === "problems"), t("nav.settings"), <Icons.settings />, () => go({ name: "settings" }))}
    </nav>
  );
}
