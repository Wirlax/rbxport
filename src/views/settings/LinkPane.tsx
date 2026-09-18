/**
 * PRO DJ LINK, a pane of its own: the LINK switch, the interface it runs
 * on, and the players on the network with what each has loaded from us.
 * The interface it runs on is kept with the DJ System preferences, where
 * a stick's defaults also live.
 */
import { useEffect, useState } from "react";

import { getBackend } from "@/ipc/client";
import type { LinkStatus } from "@/ipc/types";
import { usePreferencesContext } from "@/store/usePreferences";
import styles from "./Preferences.module.css";
import { Button, Note, Section, Select } from "./controls";

/** The dropdown's value for "no interface chosen". */
const AUTOMATIC = "";

/**
 * PRO DJ LINK: the LINK switch, the interface it runs on, and the players
 * on the network with what each has loaded from us.
 *
 * The interface is a preference, so the strip's LINK button honours it too;
 * "Automatic" leaves the choice to the app, which takes the interface the
 * players are reached through. It is changed with LINK off: a session is
 * bound to its interface for as long as it runs.
 *
 * Status arrives by event as it changes and is read once on open; the
 * session itself outlives the pane, as LINK does — a source that vanished
 * when Preferences closed would be no source at all.
 */
export function LinkPane() {
  const { preferences, update } = usePreferencesContext();
  const linkInterface = preferences.djSystem.linkInterface;
  const onChoose = (name: string | null) => update("djSystem", { linkInterface: name });
  const [link, setLink] = useState<LinkStatus | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    let stop: (() => void) | undefined;
    void (async () => {
      const backend = await getBackend();
      if (!live) return;
      stop = backend.onLinkStatus((status) => {
        if (live) setLink(status);
      });
      const status = await backend.linkStatus();
      if (live) setLink(status);
    })();
    return () => {
      live = false;
      stop?.();
    };
  }, []);

  const interfaces = link?.interfaces ?? [];
  // A chosen interface that is not there right now (unplugged, renamed) is
  // kept in the store and shown as such, not silently swapped for another.
  const choices = [
    { value: AUTOMATIC, label: "Automatic" },
    ...interfaces.map((i) => ({ value: i.name, label: `${i.name} — ${i.address}` })),
  ];
  if (linkInterface !== null && !interfaces.some((i) => i.name === linkInterface)) {
    choices.push({ value: linkInterface, label: `${linkInterface} — not present` });
  }

  const toggle = () => {
    setBusy(true);
    void (async () => {
      const backend = await getBackend();
      try {
        setLink(link?.on ? await backend.stopLinkExport() : await backend.startLinkExport(linkInterface ?? undefined));
      } finally {
        setBusy(false);
      }
    })();
  };

  return (
    <Section title="PRO DJ LINK" label="Link">
      {/* No button while LINK cannot be turned on: the reason below says why. */}
      {link !== null && !link.on && link.problem ? null : (
        <div className={styles.actions} data-spaced>
          <Button onClick={toggle} disabled={busy || link === null}>
            {link?.on ? "Disconnect" : "Connect to PRO DJ LINK"}
          </Button>
        </div>
      )}
      <Select
        label="Network interface"
        caption="Network interface"
        plain
        value={linkInterface ?? AUTOMATIC}
        disabled={link === null || link.on}
        choices={choices}
        onChange={(value) => onChoose(value === AUTOMATIC ? null : value)}
      />
      {link === null ? null : link.on ? (
        <>
          <Note>
            On as <b>rekordbox</b>
            {link.interface ? ` on ${link.interface.name} (${link.interface.address})` : ""}.
            Players list the library under LINK.
          </Note>
          {link.players.length === 0 ? (
            <Note>No players have announced themselves yet.</Note>
          ) : (
            <ul className={styles.list} aria-label="Players on the link">
              {link.players.map((player) => (
                <li key={player.number}>
                  <span className={styles.listTitle}>
                    {player.name} — {player.kind} {player.number}
                    {player.master ? " · MASTER" : ""}
                  </span>
                  <span className={styles.listPath}>
                    {player.loaded
                      ? `${player.playing ? "Playing" : "Loaded"}: ${player.loaded.title}` +
                        (player.loaded.artist ? ` — ${player.loaded.artist}` : "")
                      : `Nothing of ours loaded · ${player.address}`}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </>
      ) : link.problem ? (
        <Note failed>{link.problem}</Note>
      ) : null}
    </Section>
  );
}
