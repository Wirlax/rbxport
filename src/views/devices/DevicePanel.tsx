/**
 * What is on a connected stick, and how to put a playlist on it.
 *
 * Rekordbox calls this the Sync Manager. There is no reference capture of that
 * window — it is one of the panels the sketch never got to — so the layout
 * here is ours, built from the same tokens as the rest of the chrome rather
 * than guessed at from memory. Recorded in TODO.md under the panels that still
 * want a capture.
 */
import { useCallback, useMemo, useState } from "react";

import type { Device, TreeNode } from "@/ipc/types";
import { capacityText, contentsText, fullness } from "@/lib/devices";
import styles from "./DevicePanel.module.css";

export interface DevicePanelProps {
  device: Device;
  /** Every playlist in the library, so one can be chosen to write. */
  playlists: readonly TreeNode[];
  /** Writes `playlistId` to the device. */
  onSync: (playlistId: string) => Promise<void>;
  /** Re-reads the connected volumes. */
  onRefresh: () => void;
  busy?: boolean;
}

export function DevicePanel({ device, playlists, onSync, onRefresh, busy = false }: DevicePanelProps) {
  const choices = useMemo(() => playlists.filter((node) => node.kind === "playlist"), [playlists]);
  const [chosen, setChosen] = useState("");
  const playlistId = chosen || choices[0]?.id || "";

  const sync = useCallback(() => {
    if (playlistId) void onSync(playlistId);
  }, [onSync, playlistId]);

  const used = fullness(device);
  const capacity = capacityText(device);

  return (
    <section className={styles.panel} aria-label={`Device ${device.name}`}>
      <header className={styles.head}>
        <h2 className={styles.name}>{device.name}</h2>
        <span className={styles.path}>{device.path}</span>
        <button type="button" className={styles.refresh} onClick={onRefresh}>
          Refresh
        </button>
      </header>

      {used === null ? null : (
        <div className={styles.capacity}>
          <div
            className={styles.bar}
            role="progressbar"
            aria-label="Space used"
            aria-valuenow={Math.round(used * 100)}
            aria-valuemin={0}
            aria-valuemax={100}
          >
            <span className={styles.fill} style={{ width: `${used * 100}%` }} />
          </div>
          <span className={styles.capacityText}>{capacity}</span>
        </div>
      )}

      <p className={styles.contents}>{contentsText(device)}</p>

      <div className={styles.actions}>
        <label className={styles.label} htmlFor="device-playlist">
          Playlist
        </label>
        <select
          id="device-playlist"
          className={styles.select}
          value={playlistId}
          onChange={(e) => setChosen(e.target.value)}
          disabled={choices.length === 0 || busy}
        >
          {choices.map((node) => (
            <option key={node.id} value={node.id}>
              {node.name}
            </option>
          ))}
        </select>
        <button
          type="button"
          className={styles.sync}
          onClick={sync}
          disabled={playlistId === "" || busy}
        >
          {busy ? "Writing…" : device.export?.ours === true ? "Sync" : "Export"}
        </button>
      </div>

      {choices.length === 0 ? (
        <p className={styles.note}>There are no playlists to write yet.</p>
      ) : null}
    </section>
  );
}
