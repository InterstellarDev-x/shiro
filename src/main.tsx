import React, { useEffect } from "react";
import ReactDOM from "react-dom/client";
import { HashRouter, Route, Routes } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import "./index.css";
import MainWindow from "./windows/MainWindow";
import OverlayWindow from "./windows/OverlayWindow";

function PresenceTitle() {
  useEffect(() => {
    const apply = (label: string | null) => {
      const trimmed = label?.trim();
      if (trimmed) document.title = trimmed;
    };
    invoke<{ label: string }>("presence_state")
      .then((view) => apply(view.label))
      .catch(() => {});
    const onStorage = (event: StorageEvent) => {
      if (event.key === "shiro_presence_ping") apply(event.newValue);
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);
  return null;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <HashRouter>
      <PresenceTitle />
      <Routes>
        <Route path="/" element={<MainWindow />} />
        <Route path="/overlay" element={<OverlayWindow />} />
      </Routes>
    </HashRouter>
  </React.StrictMode>
);
