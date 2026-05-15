import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { HashRouter } from "react-router-dom";
import App from "./App";
import Settings from "./windows/Settings";
import "./index.css";

const windowLabel = getCurrentWindow().label;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {windowLabel === "settings" ? (
      <HashRouter>
        <Settings />
      </HashRouter>
    ) : (
      <App />
    )}
  </StrictMode>,
);
