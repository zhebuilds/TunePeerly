// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import "./styles.css";
import { isAndroid } from "./lib/platform";

if (isAndroid) document.documentElement.classList.add("android");

async function start() {
  const tauri = "__TAURI_INTERNALS__" in window;
  if (import.meta.env.DEV && !tauri) await import("./dev/mock");
  let mini = false;
  if (tauri) {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    mini = getCurrentWindow().label === "mini";
  }
  const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
  if (mini) {
    const { MiniPlayer } = await import("./components/MiniPlayer");
    root.render(
      <React.StrictMode>
        <MiniPlayer />
      </React.StrictMode>,
    );
    return;
  }
  const { default: App } = await import("./App");
  root.render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

void start();
