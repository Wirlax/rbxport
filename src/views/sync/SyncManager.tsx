/**
 * The Sync Manager: which playlists go to which sticks.
 *
 * rekordbox's has three columns — iTunes, rekordbox, Device — with a SYNC
 * button between the library and the device. Ours has the two that apply:
 * the library's playlists on the left, every connected device on the right,
 * and SYNC between them. More than one device can be ticked, and every
 * ticked device ends up holding exactly the ticked playlists, so two sticks
 * synced together are the same stick twice.
 *
 * Under a ticked device sits rekordbox's "Automatic synchronization": ticked,
 * it is written into the stick's sync record, and the shell writes the same
 * playlists to that stick again whenever it is plugged in.
 *
 * Nothing here holds the library: the tree is the same flat list the shell
 * fetches, and every count and name on a device comes from the backend
 * reading the stick.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { FolderIcon, ListIcon } from "@/components/icons";
import { getBackend } from "@/ipc/client";
import type { Device, DeviceSyncState, SyncDeviceReport, TreeNode } from "@/ipc/types";
import { capacityText } from "@/lib/devices";
import { exportSummary } from "@/lib/exportSummary";
import { nodesForSource, subtreeIds, toggle, visibleNodes } from "@/lib/tree";
import { startWindowDrag, toggleWindowMaximise } from "@/lib/windowDrag";
import { usePreferences } from "@/store/usePreferences";
import styles from "./SyncManager.module.css";

export interface SyncManagerProps {
  /** Drawn as a window of its own, so the shell's chrome is around it. */
  windowed?: boolean;
  onClose: () => void;
  /** A sync finished, so the shell can re-read what its devices hold. */
  onSynced?: () => void;
}

/** Ticked, not ticked, or a folder with some of its playlists ticked. */
type Tick = "on" | "off" | "some";

/**
 * The playlists and folders alone: no All Tracks, no Playlists heading,
 * because neither is a thing a stick can be given.
 */
function playlistNodes(tree: readonly TreeNode[]): TreeNode[] {
  return nodesForSource(tree, "playlists").filter((n) => n.kind === "folder" || n.kind === "playlist");
}

/** Every playlist under `folder`, itself excluded. */
function playlistsUnder(nodes: readonly TreeNode[], folder: TreeNode, byId: ReadonlyMap<string, TreeNode>): string[] {
  const out: string[] = [];
  for (const id of subtreeIds(nodes, folder)) {
    if (id !== folder.id && byId.get(id)?.kind === "playlist") out.push(id);
  }
  return out;
}

/**
 * A checkbox that can be part-way: a folder with some of its playlists
 * ticked. `indeterminate` is a property, not an attribute, so it is set by
 * hand after every render.
 */
function TickBox({
  state, label, disabled, onChange,
}: {
  state: Tick;
  label: string;
  disabled?: boolean;
  onChange: (on: boolean) => void;
}) {
  const box = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (box.current) box.current.indeterminate = state === "some";
  }, [state]);
  return (
    <input
      ref={box}
      type="checkbox"
      className={styles.tick}
      checked={state === "on"}
      aria-checked={state === "some" ? "mixed" : state === "on"}
      aria-label={label}
      disabled={disabled}
      onChange={(e) => onChange(e.currentTarget.checked)}
    />
  );
}

export function SyncManager({ windowed = false, onClose, onSynced }: SyncManagerProps) {
  const window_ = useRef<HTMLDivElement>(null);
  const [tree, setTree] = useState<readonly TreeNode[]>([]);
  const [devices, setDevices] = useState<readonly Device[]>([]);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const [ticked, setTicked] = useState<ReadonlySet<string>>(new Set());
  const [tickedDevices, setTickedDevices] = useState<ReadonlySet<string>>(new Set());
  const [states, setStates] = useState<ReadonlyMap<string, DeviceSyncState>>(new Map());
  /** Devices to sync again on their own when plugged in. */
  const [automatic, setAutomatic] = useState<ReadonlySet<string>>(new Set());
  const [busy, setBusy] = useState(false);
  /** What is happening now, or what happened: one line, or one per stick. */
  const [status, setStatus] = useState<string[]>([]);
  // DJ System in Preferences is what a stick with no settings of its own
  // gets, as it is on every export from the shell.
  const stickDefaults = usePreferences().djSystem;

  const nodes = useMemo(() => playlistNodes(tree), [tree]);
  const byId = useMemo(() => new Map(nodes.map((n) => [n.id, n] as const)), [nodes]);
  const visible = useMemo(() => visibleNodes(nodes, collapsed), [nodes, collapsed]);

  // The same tree the shell fetches, once, on open. Its folders open one
  // level deep, as rekordbox's manager opens them: the top folders show,
  // and what is inside them waits to be asked for.
  useEffect(() => {
    let live = true;
    void getBackend()
      .then((backend) => backend.playlistTree())
      .then((read) => {
        if (!live) return;
        setTree(read);
        setCollapsed(new Set(playlistNodes(read).filter((n) => n.kind === "folder" && n.depth > 1).map((n) => n.id)));
      })
      .catch(() => {
        // Not up yet: the panel stays empty, as the shell's tree would.
      });
    return () => {
      live = false;
    };
  }, []);

  const refreshDevices = useCallback(async () => {
    const backend = await getBackend();
    const found = await backend.listDevices();
    setDevices(found);
    // A stick that was pulled is not a destination any more.
    const present = new Set(found.map((d) => d.path));
    setTickedDevices((current) => new Set([...current].filter((path) => present.has(path))));
    return found;
  }, []);
  useEffect(() => {
    void refreshDevices().catch(() => {
      // No devices to list: the column says so.
    });
  }, [refreshDevices]);

  useEffect(() => {
    window_.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  /** Reads what a stick holds and was last given, and ticks that back on. */
  const readDevice = useCallback(async (path: string, restore: boolean) => {
    const backend = await getBackend();
    const read = await backend.deviceSyncState(path);
    setStates((current) => new Map(current).set(path, read));
    if (restore) {
      setAutomatic((current) => {
        const next = new Set(current);
        if (read.automatic) next.add(path);
        else next.delete(path);
        return next;
      });
    }
    if (restore && read.selected.length > 0) {
      setTicked((current) => {
        const next = new Set(current);
        for (const playlist of read.selected) if (byId.has(playlist.libraryId)) next.add(playlist.libraryId);
        return next;
      });
    }
  }, [byId]);

  const tickDevice = useCallback((path: string, on: boolean) => {
    setTickedDevices((current) => {
      const next = new Set(current);
      if (on) next.add(path);
      else next.delete(path);
      return next;
    });
    // Ticking a stick brings its last selection back: the union across
    // ticked sticks, so a second stick adds to the first's rather than
    // replacing it. Unticking takes nothing away.
    if (on) {
      void readDevice(path, true).catch(() => {
        setStates((current) => new Map(current).set(path, { selected: [], onDevice: [], automatic: false }));
      });
    }
  }, [readDevice]);

  const tickState = useCallback((node: TreeNode): Tick => {
    if (node.kind !== "folder") return ticked.has(node.id) ? "on" : "off";
    const under = playlistsUnder(nodes, node, byId);
    if (under.length === 0) return "off";
    const on = under.filter((id) => ticked.has(id)).length;
    return on === 0 ? "off" : on === under.length ? "on" : "some";
  }, [ticked, nodes, byId]);

  const tickNode = useCallback((node: TreeNode, on: boolean) => {
    const ids = node.kind === "folder" ? playlistsUnder(nodes, node, byId) : [node.id];
    setTicked((current) => {
      const next = new Set(current);
      for (const id of ids) {
        if (on) next.add(id);
        else next.delete(id);
      }
      return next;
    });
  }, [nodes, byId]);

  const canSync = ticked.size > 0 && tickedDevices.size > 0 && !busy;

  const sync = useCallback(() => {
    if (!canSync) return;
    // In tree order, so the playlists land on the stick as they are filed.
    const playlists = nodes.filter((n) => n.kind === "playlist" && ticked.has(n.id)).map((n) => n.id);
    const destinations = devices.filter((d) => tickedDevices.has(d.path)).map((d) => d.path);
    const nameOf = (path: string) => devices.find((d) => d.path === path)?.name ?? path;
    setBusy(true);
    setStatus([]);
    void (async () => {
      const backend = await getBackend();
      const stop = backend.onSyncProgress((progress) => {
        if (progress.state === "writing") setStatus([`Writing to ${nameOf(progress.path)}…`]);
      });
      try {
        // The automatic tick is per stick, but a run writes every stick from
        // one selection, so the sticks that differ on it go in a run of
        // their own.
        const reports: SyncDeviceReport[] = [];
        for (const wanted of [true, false]) {
          const these = destinations.filter((path) => automatic.has(path) === wanted);
          if (these.length > 0) reports.push(...(await backend.syncDevices(playlists, these, stickDefaults, wanted)));
        }
        setStatus(reports.map((r) => (r.report ? exportSummary(nameOf(r.path), r.report) : `${nameOf(r.path)}: ${r.error ?? "The sync failed."}`)));
        // What the sticks hold now, without touching the ticks.
        await refreshDevices();
        await Promise.all(destinations.map((path) => readDevice(path, false).catch(() => {})));
        onSynced?.();
      } catch (e) {
        setStatus([e instanceof Error ? e.message : "The sync could not be written."]);
      } finally {
        stop();
        setBusy(false);
      }
    })();
  }, [canSync, nodes, ticked, devices, tickedDevices, automatic, stickDefaults, refreshDevices, readDevice, onSynced]);

  const body = (
    <div
      ref={window_}
      className={styles.window}
      data-windowed={windowed || undefined}
      // The backdrop closes on click; the window must not pass its own through.
      onMouseDown={(e) => e.stopPropagation()}
      role="dialog"
      aria-modal={windowed ? undefined : "true"}
      aria-label="Sync Manager"
      tabIndex={-1}
    >
      <header
        className={styles.titlebar}
        onMouseDown={windowed ? startWindowDrag : undefined}
        onDoubleClick={windowed ? toggleWindowMaximise : undefined}
      >
        {windowed ? null : (
          <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
            ✕
          </button>
        )}
        Sync Manager
      </header>
      <div className={styles.body}>
        <section className={styles.column} aria-label="rbxport">
          <h2 className={styles.heading}>rbxport</h2>
          <div className={styles.list} role="tree" aria-label="Playlists">
            {visible.map((node) => {
              const folder = node.kind === "folder";
              const open = !collapsed.has(node.id);
              const state = tickState(node);
              return (
                <div
                  key={node.id}
                  className={styles.row}
                  role="treeitem"
                  aria-expanded={folder ? open : undefined}
                  aria-selected={state === "on"}
                  style={{ paddingLeft: `${8 + (node.depth - 1) * 18}px` }}
                >
                  <button
                    type="button"
                    className={styles.twisty}
                    data-open={folder && open ? "" : undefined}
                    data-leaf={folder ? undefined : ""}
                    aria-label={folder ? (open ? `Collapse ${node.name}` : `Expand ${node.name}`) : undefined}
                    tabIndex={folder ? 0 : -1}
                    onClick={() => folder && setCollapsed((current) => toggle(current, node.id))}
                  />
                  {folder ? <FolderIcon className={styles.icon} /> : <ListIcon className={styles.icon} />}
                  <span className={styles.name}>{node.name}</span>
                  <TickBox state={state} label={node.name} disabled={busy} onChange={(on) => tickNode(node, on)} />
                </div>
              );
            })}
            {tree.length > 0 && nodes.length === 0 ? <p className={styles.empty}>No playlists yet.</p> : null}
          </div>
        </section>

        <div className={styles.middle}>
          <button
            type="button"
            className={styles.sync}
            onClick={sync}
            disabled={!canSync}
            aria-label="SYNC"
            aria-busy={busy || undefined}
          >
            SYNC <span className={styles.chevron} aria-hidden>›</span>
          </button>
        </div>

        <section className={styles.column} aria-label="Device">
          <div className={styles.headingRow}>
            <h2 className={styles.heading}>Device</h2>
            <button
              type="button"
              className={styles.refresh}
              onClick={() => void refreshDevices().catch(() => {})}
              disabled={busy}
            >
              Refresh
            </button>
          </div>
          <div className={styles.list} role="list" aria-label="Devices">
            {devices.map((device) => {
              const on = tickedDevices.has(device.path);
              const read = states.get(device.path);
              return (
                <div key={device.path} className={styles.device} role="listitem" data-ticked={on || undefined}>
                  <div className={styles.row}>
                    <span className={styles.name}>{device.name}</span>
                    <TickBox
                      state={on ? "on" : "off"}
                      label={device.name}
                      disabled={busy}
                      onChange={(next) => tickDevice(device.path, next)}
                    />
                  </div>
                  {on ? (
                    <div className={styles.library} aria-label={`${device.name} library`}>
                      <div className={styles.libraryHead}>Device Library</div>
                      {read === undefined ? (
                        <div className={styles.libraryNote}>Reading…</div>
                      ) : read.onDevice.length === 0 ? (
                        <div className={styles.libraryNote}>No playlists on this device yet.</div>
                      ) : (
                        <ul className={styles.libraryList}>
                          {read.onDevice.map((name, i) => (
                            // Names can repeat on a stick; the position is what tells them apart.
                            <li key={`${i}:${name}`}>{name}</li>
                          ))}
                        </ul>
                      )}
                      <div className={styles.capacity}>{capacityText(device)}</div>
                      <label className={styles.row}>
                        <span className={styles.name}>Automatic synchronization</span>
                        <TickBox
                          state={automatic.has(device.path) ? "on" : "off"}
                          label={`Automatic synchronization for ${device.name}`}
                          disabled={busy}
                          onChange={(on) =>
                            setAutomatic((current) => {
                              const next = new Set(current);
                              if (on) next.add(device.path);
                              else next.delete(device.path);
                              return next;
                            })
                          }
                        />
                      </label>
                    </div>
                  ) : null}
                </div>
              );
            })}
            {devices.length === 0 ? <p className={styles.empty}>No devices connected.</p> : null}
          </div>
        </section>
      </div>
      <footer className={styles.footer}>
        <div className={styles.status} role="status" aria-live="polite">
          {status.map((line) => (
            <div key={line}>{line}</div>
          ))}
        </div>
        <button type="button" className={styles.button} onClick={onClose}>
          Close
        </button>
      </footer>
    </div>
  );

  if (windowed) return body;
  return (
    <div className={styles.backdrop} onMouseDown={onClose} role="presentation">
      {body}
    </div>
  );
}
