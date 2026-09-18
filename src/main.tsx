import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { PreferencesWindow } from "./views/settings/PreferencesWindow";
import { SyncWindow } from "./views/sync/SyncWindow";
import "./styles/base.css";

const el = document.getElementById("root");
if (!el) throw new Error("#root missing from index.html");

// The same bundle serves the Preferences and Sync Manager windows: the
// shell opens them at `#preferences/<pane>` and `#sync`, and that is all
// each draws.
const preferences = window.location.hash.startsWith("#preferences");
const sync = window.location.hash.startsWith("#sync");

createRoot(el).render(
  <StrictMode>
    {preferences ? <PreferencesWindow /> : sync ? <SyncWindow /> : <App />}
  </StrictMode>,
);
