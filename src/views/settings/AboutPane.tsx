/**
 * About: what this is and which version, and whether it looks for a newer
 * one on its own. Not a pane rekordbox has — its version is under the
 * application menu — but the update check belongs with the version it
 * checks, so it is here as well as under Advanced › Others.
 */
import { useEffect, useState } from "react";

import { getBackend } from "@/ipc/client";
import { usePreferencesContext } from "@/store/usePreferences";
import styles from "./Preferences.module.css";
import { Button, Note, Section, Toggle } from "./controls";

export function AboutPane() {
  const { preferences, update } = usePreferencesContext();
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void getBackend()
      .then((backend) => backend.appVersion())
      .then((found) => {
        if (live) setVersion(found);
      })
      .catch(() => {
        // A build with no shell behind it has no version to give; the pane
        // shows a dash rather than an error nobody asked about.
      });
    return () => {
      live = false;
    };
  }, []);

  return (
    <>
      <Section title="rbxport">
        <dl className={styles.facts}>
          <dt>Version</dt>
          <dd data-testid="about-version">{version ?? "—"}</dd>
        </dl>
        <Note>
          An export-mode rekordbox: the shared library, analysis, USB export
          and link export, and nothing else.
        </Note>
      </Section>
      <Section title="Updates">
        <Toggle
          label="Automatically check for updates"
          checked={preferences.advanced.checkUpdates}
          onChange={(checkUpdates) => update("advanced", { checkUpdates })}
        />
        <div className={styles.actions}>
          <Button
            onClick={() => {
              // The Update Manager belongs to the main window, so the check
              // is asked for there — this pane may be a window of its own.
              void getBackend().then((backend) => backend.requestPreferencesReset("updates"));
            }}
          >
            Check for Updates…
          </Button>
        </div>
        <Note>
          Checked a moment after the app starts, when this is on; a new
          version is downloaded and installed from the Update Manager, with
          what changed shown first.
        </Note>
      </Section>
    </>
  );
}
