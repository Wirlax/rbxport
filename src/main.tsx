import { lazy, StrictMode, Suspense } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
const PreferencesWindow = lazy(() => import("./views/settings/PreferencesWindow").then(m => ({ default: m.PreferencesWindow })));
const ReportWindow = lazy(() => import("./views/report/ReportBug").then(m => ({ default: m.ReportWindow })));
const SyncWindow = lazy(() => import("./views/sync/SyncWindow").then(m => ({ default: m.SyncWindow })));
import "./styles/base.css";

// Suppress the webview's Reload/Inspect menu in every app window. Leave
// propagation intact so the app's context-menu handlers still receive it.
window.addEventListener("contextmenu", (event) => event.preventDefault());

const el = document.getElementById("root");
if (!el) throw new Error("#root missing from index.html");

// The same bundle serves the Preferences and Sync Manager windows: the
// shell opens them at `#preferences/<pane>` and `#sync`, and that is all
// each draws.
const preferences = window.location.hash.startsWith("#preferences");
const sync = window.location.hash.startsWith("#sync");

createRoot(el).render(
  <StrictMode>
    <Suspense fallback={null}>
    {window.location.hash.startsWith("#report") ? <ReportWindow /> : preferences ? <PreferencesWindow /> : sync ? <SyncWindow /> : <App />}
    </Suspense>
  </StrictMode>,
);
