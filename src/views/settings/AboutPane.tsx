/**
 * About: what this is and which version. Not a pane rekordbox has — its
 * version is under the application menu. The update check is Advanced ›
 * Others' alone, so the switch is in one place rather than two.
 */
import { useEffect, useState } from "react";

import { getBackend } from "@/ipc/client";
import styles from "./Preferences.module.css";
import { Section } from "./controls";

export function AboutPane() {
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
    <Section title="rbxport">
      <dl className={styles.facts}>
        <dt>Version</dt>
        <dd data-testid="about-version">{version ?? "—"}</dd>
        <dt>By</dt>
        <dd>@TRIODEOfficial</dd>
      </dl>
    </Section>
  );
}
