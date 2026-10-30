// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { transport } from "./player/clock";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { setDropHandler } from "./app/drag";
import { AppProvider, useApp, type Route } from "./app/model";
import { ScrollContext } from "./app/scroll";
import { ContextMenu } from "./components/ContextMenu";
import { Tooltip } from "./components/Tooltip";
import { Mascot } from "./components/Mascot";
import { Banners, Overlays } from "./components/Overlays";
import { PlayerBar } from "./components/PlayerBar";
import { CloseGate, WindowControls } from "./components/WindowControls";
import { QueuePanel } from "./components/QueuePanel";
import { Sidebar, useSidebarWidth } from "./components/Sidebar";
import { SEARCH_INPUT_ID, TopBar } from "./components/TopBar";
import { BrowsePage } from "./pages/BrowsePage";
import { DevicesPage } from "./pages/DevicesPage";
import { NoticePanel } from "./components/NoticePanel";
import { HomePage } from "./pages/HomePage";
import { NowPlayingPage } from "./pages/NowPlayingPage";
import { ProblemsPage } from "./pages/ProblemsPage";
import { SearchPage } from "./pages/SearchPage";
import { SettingsPage } from "./pages/SettingsPage";
import { TrackListPage } from "./pages/TrackListPage";

export default function App() {
  return (
    <AppProvider>
      <Shell />
    </AppProvider>
  );
}

function Page({ route }: { route: Route }) {
  switch (route.name) {
    case "home":
      return <HomePage />;
    case "search":
      return <SearchPage scope={route.id} />;
    case "artists":
    case "albums":
    case "folders":
      return <BrowsePage kind={route.name} />;
    case "devices":
      return <DevicesPage />;
    case "settings":
      return <SettingsPage />;
    case "problems":
      return <ProblemsPage />;
    case "now":
      return <NowPlayingPage />;
    default:
      return <TrackListPage route={route} />;
  }
}

function typing(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element) return false;
  if (element.isContentEditable) return true;
  const tag = element.tagName;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag !== "INPUT") return false;
  const type = (element as HTMLInputElement).type;
  return type !== "checkbox" && type !== "radio" && type !== "button";
}

function Shell() {
  const { t } = useTranslation();
  const app = useApp();
  const scroll = useRef<HTMLDivElement>(null);
  const [sidebar, setSidebar] = useSidebarWidth();
  const [dropping, setDropping] = useState(false);
  const latest = useRef(app);
  latest.current = app;

  useEffect(() => {
    scroll.current?.scrollTo({ top: 0 });
  }, [app.route]);

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let stop = () => {};
    let cancelled = false;
    void import("@tauri-apps/api/event").then(async ({ listen }) => {
      const unlisten = await listen<{ action: string }>("desktop-action", (event) => {
        const model = latest.current;
        switch (event.payload.action) {
          case "toggle":
            if (!model.following) model.toggle();
            break;
          case "play":
            if (!model.following && !transport.get().playing) model.toggle();
            break;
          case "pause":
            if (!model.following) model.engine.pause();
            break;
          case "next":
            if (!model.following) model.next();
            break;
          case "previous":
            if (!model.following) model.previous();
            break;
          case "like":
            if (model.shown) void model.like(model.shown);
            break;
          default:
            break;
        }
      });
      if (cancelled) unlisten();
      else stop = unlisten;
    }).catch(() => {});
    return () => {
      cancelled = true;
      stop();
    };
  }, []);

  useEffect(() => {
    setDropHandler((source, target) => {
      const model = latest.current;
      if (target.type === "playlist") void model.addToPlaylist(target.id, source.tracks);
      else if (target.type === "liked") {
        const fresh = source.tracks.filter((track) => !track.favorite);
        for (const track of fresh) void model.like(track);
        if (fresh.length === 0) model.toast(t("toast.alreadyLiked"));
      } else if (target.type === "queue") {
        if (source.type === "queue") model.queueActions.move(source.index, target.index > source.index ? target.index - 1 : target.index);
        else model.queueActions.insert(target.index, source.tracks);
      } else if (target.type === "member" && source.type === "member") {
        const to = target.index > source.index ? target.index - 1 : target.index;
        if (to === source.index) return;
        if (source.playlistId === "library") void model.moveLibrary(source.index, to);
        else void model.movePlaylistMember(source.playlistId, source.index, to);
      }
    });
  }, [t]);

  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    try {
      void getCurrentWebview()
        .onDragDropEvent((event) => {
          const payload = event.payload;
          if (payload.type === "enter" || payload.type === "over") setDropping(true);
          else if (payload.type === "leave") setDropping(false);
          else if (payload.type === "drop") {
            setDropping(false);
            if (payload.paths.length > 0) void latest.current.importPaths(payload.paths, "auto");
          }
        })
        .then((unlisten) => {
          if (cancelled) unlisten();
          else stop = unlisten;
        })
        .catch(() => {});
    } catch {
      // Not running inside Tauri (browser preview).
    }
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const model = latest.current;
      const ctrl = event.ctrlKey || event.metaKey;
      if (model.dialog) return;
      if (ctrl && event.key.toLowerCase() === "k") {
        event.preventDefault();
        const input = document.getElementById(SEARCH_INPUT_ID) as HTMLInputElement | null;
        input?.focus();
        input?.select();
        return;
      }
      if (event.key === "Escape") {
        if (model.menu) model.setMenu(null);
        return;
      }
      if (event.altKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
        event.preventDefault();
        if (event.key === "ArrowLeft") model.back();
        else model.forward();
        return;
      }
      if (typing(event.target) || event.altKey) return;
      const element = event.target as HTMLElement | null;
      const onSlider = element?.tagName === "INPUT";
      // widgets that own the arrow keys themselves (the sidebar splitter) must not seek
      const onWidget = !!element?.closest("[role='separator'], [role='slider']");
      // inside the song list the arrows move the focus between rows (TrackTable), not the playhead
      const inGrid = !!element?.closest("[role='grid']");
      if (event.key === " " || event.code === "Space") {
        if ((event.target as HTMLElement | null)?.closest("button, [role='button']")) return;
        event.preventDefault();
        model.toggle();
      } else if (ctrl && event.key === "ArrowRight") {
        event.preventDefault();
        model.next();
      } else if (ctrl && event.key === "ArrowLeft") {
        event.preventDefault();
        model.previous();
      } else if (ctrl && event.key === "ArrowUp") {
        event.preventDefault();
        model.setVolume(model.volume + 0.05);
      } else if (ctrl && event.key === "ArrowDown") {
        event.preventDefault();
        model.setVolume(model.volume - 0.05);
      } else if (!ctrl && !event.shiftKey && !onSlider && !onWidget && !inGrid && (event.key === "ArrowLeft" || event.key === "ArrowRight") && model.current) {
        event.preventDefault();
        model.engine.skip(event.key === "ArrowRight" ? 5 : -5);
      }
    };
    const onMouse = (event: MouseEvent) => {
      const model = latest.current;
      if (model.dialog || typing(event.target)) return;
      if (event.button === 3) model.back();
      if (event.button === 4) model.forward();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mouseup", onMouse);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mouseup", onMouse);
    };
  }, []);

  if (!app.boot) {
    return (
      <div className="boot-screen">
        <Mascot mood={app.loadError ? "error" : "idle"} size={120} />
        {app.loadError ? (
          <>
            <p>{t("boot.failed")}</p>
            <code className="mono small">{app.loadError}</code>
            <button type="button" className="btn primary" onClick={() => void app.refresh()}>
              {t("common.retry")}
            </button>
          </>
        ) : (
          <p className="muted">{t("boot.loading")}</p>
        )}
      </div>
    );
  }

  return (
    <div
      className={`app${app.queueOpen ? " queue-open" : ""}${app.noticesOpen ? " notices-open" : ""}`}
      onContextMenu={(event) => {
        if (!typing(event.target)) event.preventDefault();
      }}
    >
      <Sidebar
        width={sidebar.width}
        collapsed={sidebar.collapsed}
        onResize={(width) => setSidebar((value) => ({ ...value, width }))}
        onCollapse={(collapsed) => setSidebar((value) => (value.collapsed === collapsed ? value : { ...value, collapsed }))}
      />
      <main className="main-card" data-scrolled="false">
        <TopBar />
        <ScrollContext.Provider value={scroll}>
          <div
            className="main-scroll"
            ref={scroll}
            onScroll={(event) => {
              const element = event.currentTarget;
              const scrolled = element.scrollTop > 24 ? "true" : "false";
              const card = element.parentElement;
              if (card && card.dataset.scrolled !== scrolled) card.dataset.scrolled = scrolled;
            }}
          >
            <Banners />
            <Page route={app.route} />
          </div>
        </ScrollContext.Provider>
      </main>
      {app.queueOpen && <QueuePanel />}
      {app.noticesOpen && <NoticePanel />}
      <PlayerBar />
      <WindowControls />
      <CloseGate />
      <ContextMenu />
      <Tooltip />
      <Overlays dropping={dropping} />
    </div>
  );
}

