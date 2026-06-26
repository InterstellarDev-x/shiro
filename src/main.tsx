import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import "./index.css";
import MainWindow from "./windows/MainWindow";
import OverlayWindow from "./windows/OverlayWindow";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<MainWindow />} />
        <Route path="/overlay" element={<OverlayWindow />} />
      </Routes>
    </BrowserRouter>
  </React.StrictMode>
);
