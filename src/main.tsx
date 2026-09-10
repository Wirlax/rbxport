import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { PreferencesWindow } from "./views/settings/PreferencesWindow";
import "./styles/base.css";

const el = document.getElementById("root");
if (!el) throw new Error("#root missing from index.html");

// The same bundle serves the Preferences window: the shell opens it at
// `#preferences/<pane>`, and that is all it draws.
const preferences = window.location.hash.startsWith("#preferences");

createRoot(el).render(
  <StrictMode>
    {preferences ? <PreferencesWindow /> : <App />}
  </StrictMode>,
);
