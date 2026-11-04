import React from "react";
import ReactDOM from "react-dom/client";
import { HashRouter, Route, Routes } from "react-router-dom";
import "./index.css";
import MainWindow from "./windows/MainWindow";
import OverlayWindow from "./windows/OverlayWindow";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <HashRouter>
      <Routes>
        <Route path="/" element={<MainWindow />} />
        <Route path="/overlay" element={<OverlayWindow />} />
      </Routes>
    </HashRouter>
  </React.StrictMode>
);
