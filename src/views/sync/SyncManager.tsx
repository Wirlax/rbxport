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
 * Nothing here holds the library: the tree is the same flat list the shell
 * fetches, and every count and name on a device comes from the backend
 * reading the stick.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, LoaderCircle, Search, X } from "lucide-react";

import { EjectIcon, FolderIcon, ListIcon } from "@/components/icons";
import { getBackend } from "@/ipc/client";
import type { Device, DeviceSyncState, TreeNode } from "@/ipc/types";
import { formatSpace } from "@/lib/devices";
import { exportSummary } from "@/lib/exportSummary";
import { errorMessage } from "@/lib/errorMessage";
import { nodesForSource, subtreeIds, toggle, visibleNodes } from "@/lib/tree";
import { startWindowDrag, toggleWindowMaximise } from "@/lib/windowDrag";
import { usePreferences } from "@/store/usePreferences";
import { useExportProgress, exportPercent } from "@/store/useExportProgress";
import { StopExport } from "@/components/StopExport";
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
  const exportJobs = useExportProgress();
  const window_ = useRef<HTMLDivElement>(null);
  const [tree, setTree] = useState<readonly TreeNode[]>([]);
  const [devices, setDevices] = useState<readonly Device[]>([]);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const [ticked, setTicked] = useState<ReadonlySet<string>>(new Set());
  const [tickedDevices, setTickedDevices] = useState<ReadonlySet<string>>(new Set());
  const [expandedDevices, setExpandedDevices] = useState<ReadonlySet<string>>(new Set());
  const [deviceErrors, setDeviceErrors] = useState<ReadonlyMap<string, string>>(new Map());
  const [states, setStates] = useState<ReadonlyMap<string, DeviceSyncState>>(new Map());
  const [query, setQuery] = useState("");
  const [loadingTree, setLoadingTree] = useState(true);
  const [treeError, setTreeError] = useState("");
  const [loadingDevices, setLoadingDevices] = useState(false);
  const [devicesError, setDevicesError] = useState("");
  // This window may live in its own webview, so it keeps its own live view
  // of rekordbox's process lock instead of relying on the main window.
  // Unknown is locked: do not briefly enable SYNC before the first check.
  const [rekordboxOpen, setRekordboxOpen] = useState<boolean | null>(null);
  const [operation, setOperation] = useState<"sync" | "import" | "eject" | null>(null);
  const [ejectingPath, setEjectingPath] = useState<string | null>(null);
  const busy = operation !== null || [...exportJobs.values()].some(job => job.state === "writing");
  const [ejectAfterSync, setEjectAfterSync] = useState(false);
  /** What is happening now, or what happened: one line, or one per stick. */
  const [status, setStatus] = useState<string[]>([]);
  // DJ System in Preferences is what a stick with no settings of its own
  // gets, as it is on every export from the shell.
  const preferences = usePreferences();
  const stickDefaults = preferences.djSystem;
  const deleteUnlistedMusic = preferences.usbExport.deleteUnlistedMusic;
  const compatibilityFormat = preferences.usbExport.maximumCompatibility ? preferences.usbExport.conversionFormat : undefined;

  const nodes = useMemo(() => playlistNodes(tree), [tree]);
  const playlists = useMemo(() => nodes.filter(node => node.kind === "playlist"), [nodes]);
  const byId = useMemo(() => new Map(nodes.map((n) => [n.id, n] as const)), [nodes]);
  const search = query.trim().toLocaleLowerCase();
  const visible = useMemo(() => search
    ? playlists.filter(node => node.name.toLocaleLowerCase().includes(search))
    : visibleNodes(nodes, collapsed), [nodes, playlists, collapsed, search]);
  const selectedCount = playlists.filter(node => ticked.has(node.id)).length;
  const selectionSummary = `${selectedCount} playlist${selectedCount === 1 ? "" : "s"} → ${tickedDevices.size} USB device${tickedDevices.size === 1 ? "" : "s"}`;
  const selectionHint = selectedCount === 0 && tickedDevices.size === 0 ? "Select playlists and a USB device."
    : selectedCount === 0 ? "Select playlists to sync."
    : tickedDevices.size === 0 ? "Select a USB device to sync to."
    : "Selected playlists will sync to each selected device.";
  const syncHint = rekordboxOpen
    ? "Quit rekordbox to enable synchronization."
    : rekordboxOpen === null
      ? "Checking whether rekordbox is running…"
      : selectionHint;

  useEffect(() => {
    let live = true;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = async () => {
      try {
        const backend = await getBackend();
        const summary = await backend.librarySummary();
        if (live) setRekordboxOpen(summary.readOnly);
      } catch {
        // Preserve the last known state during a temporary backend failure.
      } finally {
        if (live) timer = setTimeout(() => void refresh(), 2000);
      }
    };
    void refresh();
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, []);

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
      .catch(() => { if (live) setTreeError("Couldn’t load playlists. Reopen Sync Manager to try again."); })
      .finally(() => { if (live) setLoadingTree(false); });
    return () => {
      live = false;
    };
  }, []);

  const refreshDevices = useCallback(async () => {
    setLoadingDevices(true);
    setDevicesError("");
    try {
      const backend = await getBackend();
      const found = await backend.listDevices();
      setDevices(found);
      // A stick that was pulled is not a destination any more.
      const present = new Set(found.map((d) => d.path));
      setTickedDevices((current) => new Set([...current].filter((path) => present.has(path))));
      return found;
    } catch (e) {
      setDevicesError("Couldn’t read USB devices. Click Refresh to try again.");
      throw e;
    } finally { setLoadingDevices(false); }
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
    setDeviceErrors(current => { const next = new Map(current); next.delete(path); return next; });
    setStates((current) => new Map(current).set(path, read));
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
      void readDevice(path, true).catch(e => {
        setDeviceErrors(current => new Map(current).set(path, e instanceof Error ? e.message : "Could not read this USB device."));
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

  const canSync = rekordboxOpen === false && selectedCount > 0 && tickedDevices.size > 0 && !busy && !loadingDevices;

  const ejectDevice = async (device: Device) => {
    if (busy || loadingDevices) return;
    setOperation("eject");
    setEjectingPath(device.path);
    setStatus([`Ejecting ${device.name}…`]);
    try {
      const backend = await getBackend();
      await backend.ejectDevice(device.path);
      setDevices(current => current.filter(d => d.path !== device.path));
      setTickedDevices(current => new Set([...current].filter(path => path !== device.path)));
      setExpandedDevices(current => new Set([...current].filter(path => path !== device.path)));
      setStates(current => { const next = new Map(current); next.delete(device.path); return next; });
      setDeviceErrors(current => { const next = new Map(current); next.delete(device.path); return next; });
      setStatus([`${device.name}: Safely ejected.`]);
      onSynced?.();
    } catch (e) {
      setStatus([`${device.name}: Could not eject. ${errorMessage(e)}`]);
    } finally {
      setEjectingPath(null);
      setOperation(null);
    }
  };

  const sync = useCallback(() => {
    if (!canSync) return;
    // In tree order, so the playlists land on the stick as they are filed.
    const playlists = nodes.filter((n) => n.kind === "playlist" && ticked.has(n.id)).map((n) => n.id);
    const destinations = devices.filter((d) => tickedDevices.has(d.path)).map((d) => d.path);
    const nameOf = (path: string) => devices.find((d) => d.path === path)?.name ?? path;
    setOperation("sync");
    setStatus(["Preparing sync…"]);
    void (async () => {
      let stop = () => {};
      try {
        const backend = await getBackend();
        // Close the interval between the last poll and the click: rekordbox
        // may have launched while the button was still visibly enabled.
        const summary = await backend.librarySummary();
        setRekordboxOpen(summary.readOnly);
        if (summary.readOnly) {
          setStatus(["Quit rekordbox to enable synchronization."]);
          return;
        }
        stop = backend.onSyncProgress((progress) => {
          if (progress.state === "writing") setStatus([`Writing to ${nameOf(progress.path)}…`]);
          if (progress.state === "ejecting") setStatus([`Ejecting ${nameOf(progress.path)}…`]);
        });
        const reports = await backend.syncDevices(playlists, destinations, stickDefaults, false, ejectAfterSync, deleteUnlistedMusic, compatibilityFormat);
        setStatus(reports.map((r) => {
          const summary = r.report ? exportSummary(nameOf(r.path), r.report) : `${nameOf(r.path)}: ${r.error ?? "The sync failed."}`;
          return summary + (r.ejected ? " Safely ejected." : r.ejectError ? ` Not ejected: ${r.ejectError}` : "");
        }));
        // What the sticks hold now, without touching the ticks.
        await refreshDevices().catch(() => {});
        await Promise.all(reports.filter(r => !r.ejected).map(({ path }) => readDevice(path, false).catch(() => {})));
        onSynced?.();
      } catch (e) {
        setStatus([e instanceof Error ? e.message : "The sync could not be written."]);
      } finally {
        stop();
        setOperation(null);
      }
    })();
  }, [canSync, nodes, ticked, devices, tickedDevices, stickDefaults, ejectAfterSync, deleteUnlistedMusic, compatibilityFormat, refreshDevices, readDevice, onSynced]);

  const importCues = () => {
    if (busy || tickedDevices.size === 0) return;
    setOperation("import");
    setStatus([]);
    void (async () => {
      try {
        const backend = await getBackend();
        if (!await backend.confirm("Import cue and beat-grid changes from the selected USB devices? This replaces cues and grids for matching tracks in your library.")) return;
        const results: string[] = [];
        for (const device of devices.filter(d => tickedDevices.has(d.path))) {
          setStatus([`Importing cues and beat grids from ${device.name}…`]);
          try {
            const result = await backend.importUsb(device.path, true, false, false);
            results.push(`${device.name}: updated ${result.tracks} tracks${result.skipped ? `; skipped ${result.skipped}` : ""}.`);
          } catch (e) { results.push(`${device.name}: ${e instanceof Error ? e.message : String(e)}`); }
        }
        setStatus(results);
        onSynced?.();
      } catch (e) { setStatus([e instanceof Error ? e.message : String(e)]); }
      finally { setOperation(null); }
    })();
  };

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
          <div className={styles.headingRow}>
            <div><h2 className={styles.heading}>rbxport</h2><p className={styles.columnNote}>{selectedCount} of {playlists.length} playlists selected</p></div>
            <button type="button" className={styles.textButton} disabled={busy || selectedCount === 0}
              onClick={() => setTicked(new Set())}>Clear selection</button>
          </div>
          <div className={styles.search}>
            <Search size={14} aria-hidden="true" />
            <input type="search" aria-label="Search playlists" placeholder="Search playlists" value={query} onChange={e => setQuery(e.currentTarget.value)} />
            {query ? <button type="button" aria-label="Clear playlist search" onClick={() => setQuery("")}><X size={14} aria-hidden="true" /></button> : null}
          </div>
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
                  style={{ paddingLeft: `${8 + (search ? 0 : node.depth - 1) * 18}px` }}
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
                  <label className={styles.rowSelection}>
                  {folder ? <FolderIcon className={styles.icon} /> : <ListIcon className={styles.icon} />}
                  <span className={styles.name}>{node.name}</span>
                  <TickBox state={state} label={node.name} disabled={busy} onChange={(on) => tickNode(node, on)} />
                  </label>
                </div>
              );
            })}
            {loadingTree ? <p className={styles.empty}>Loading playlists…</p>
              : treeError ? <p className={styles.empty} role="alert">{treeError}</p>
              : nodes.length === 0 ? <p className={styles.empty}>No playlists yet. Create a playlist in your library to get started.</p>
              : visible.length === 0 ? <p className={styles.empty}>No playlists match “{query.trim()}”.</p> : null}
          </div>
        </section>

        <div className={styles.middle}>
          <div className={styles.syncActions}>
          <button
            type="button"
            className={styles.sync}
            onClick={sync}
            disabled={!canSync}
            title={rekordboxOpen ? "Quit rekordbox to enable synchronization." : undefined}
            aria-label="SYNC"
            aria-busy={busy || undefined}
            aria-describedby="sync-selection-hint"
          >
            {operation === "sync" ? <><LoaderCircle size={16} className={styles.spinner} aria-hidden="true" /> Syncing…</> : <>SYNC <ArrowRight size={16} aria-hidden="true" /></>}
          </button>
          <label className={styles.ejectOption}>
            <input type="checkbox" className={styles.tick} checked={ejectAfterSync} disabled={busy}
              aria-label="Eject after syncing" onChange={e => setEjectAfterSync(e.currentTarget.checked)} />
            Eject after syncing
          </label>
          </div>
          <div className={styles.importActions}>
          <button type="button" className={styles.button} onClick={importCues}
            disabled={busy || tickedDevices.size === 0 || preferences.advanced.protectLibrary}
            title={preferences.advanced.protectLibrary ? "Turn off Library Protection to import cues and grids." : "Import cues and beat grids from USB to rbxport"}>
            {operation === "import" ? <LoaderCircle size={14} className={styles.spinner} aria-hidden="true" /> : <ArrowLeft size={14} aria-hidden="true" />} {operation === "import" ? "Importing…" : "CUE GRID INFO"}
          </button>
          </div>
        </div>

        <section className={styles.column} aria-label="Device">
          <div className={styles.headingRow}>
            <div><h2 className={styles.heading}>Device</h2><p className={styles.columnNote}>{tickedDevices.size} of {devices.length} selected</p></div>
            <button
              type="button"
              className={styles.refresh}
              onClick={() => void refreshDevices().catch(() => {})}
              disabled={busy || loadingDevices}
            >
              {loadingDevices ? "Refreshing…" : "Refresh"}
            </button>
          </div>
          {devicesError ? <p className={styles.empty} role="alert">{devicesError}</p> : null}
          <div className={styles.list} role="tree" aria-label="Devices">
            {devices.map((device) => {
              const on = tickedDevices.has(device.path);
              const expanded = expandedDevices.has(device.path);
              const read = states.get(device.path);
              const job = exportJobs.get(device.path);
              const fileSystem = device.fileSystem?.toUpperCase().replace(/^VFAT$|^MSDOS$/, "FAT") || "Unknown filesystem";
              const free = device.totalBytes > 0 ? `${device.freeBytes === 0 ? "0.0 GB" : formatSpace(device.freeBytes)} free (${Math.round(device.freeBytes / device.totalBytes * 100)}%)` : "Space unknown";
              const freePercent = device.totalBytes > 0 ? Math.max(0, Math.min(100, device.freeBytes / device.totalBytes * 100)) : null;
              const usedPercent = freePercent === null ? null : 100 - freePercent;
              const used = device.totalBytes > 0 ? formatSpace(Math.max(0, device.totalBytes - device.freeBytes)) || "0.0 GB" : "";
              return <div key={device.path} className={styles.device} role="treeitem" aria-expanded={expanded} data-ticked={on || undefined}>
                <div className={styles.row}>
                  <button type="button" className={styles.twisty} data-open={expanded ? "" : undefined}
                    aria-label={`${expanded ? "Collapse" : "Expand"} ${device.name}`} disabled={busy} onClick={() => {
                      setExpandedDevices(current => toggle(current, device.path));
                      if (!expanded) void readDevice(device.path, false).catch(e => setDeviceErrors(current => new Map(current).set(device.path, e instanceof Error ? e.message : "Could not read this USB device.")));
                    }} />
                  <label className={styles.rowSelection}>
                  <span className={styles.name} title={device.path}>{device.name}</span>
                  <TickBox state={on ? "on" : "off"} label={device.name} disabled={busy} onChange={(next) => tickDevice(device.path, next)} />
                  </label>
                  <button type="button" className={styles.ejectButton}
                    aria-label={`Eject ${device.name}`} title={`Safely eject ${device.name}`}
                    disabled={busy || loadingDevices} aria-busy={ejectingPath === device.path || undefined}
                    onClick={() => void ejectDevice(device)}>
                    {ejectingPath === device.path
                      ? <LoaderCircle size={14} className={styles.spinner} aria-hidden="true" />
                      : <EjectIcon />}
                  </button>
                </div>
                <div className={styles.storage}>
                  <p className={styles.capacity}>{fileSystem}{device.totalBytes > 0 ? ` · ${formatSpace(device.totalBytes)} total` : ""}</p>
                  <div className={styles.spaceBar} role={freePercent === null ? "img" : "meter"}
                    aria-label={`${device.name} storage used`} aria-valuemin={usedPercent === null ? undefined : 0}
                    aria-valuemax={usedPercent === null ? undefined : 100} aria-valuenow={usedPercent ?? undefined}
                    aria-valuetext={usedPercent === null ? "Space unknown" : `${used} used; ${free}`} title={free}>
                    {usedPercent !== null ? <span style={{ width: `${usedPercent}%` }} /> : null}
                  </div>
                  <div className={styles.storageLabels}>{usedPercent !== null ? <span><i aria-hidden="true" />{used} used</span> : null}<span>{free}</span></div>
                  {job ? <div className={styles.exportProgress}>
                    <span>{job.state === "cancelled" ? "Export stopped" : job.state === "failed" ? "Export failed" : job.state === "done" ? "Export complete" : `Exporting ${device.name}`} ({exportPercent(job)}%)</span>
                    <progress aria-label={`Exporting ${device.name}`} max={100} value={exportPercent(job)} />
                    {job.state === "writing" ? <StopExport path={job.path} className={styles.button} /> : null}
                    {job.state === "failed" ? <span role="alert">{job.title}</span> : null}
                  </div> : null}
                </div>
                {!expanded && deviceErrors.has(device.path) ? <p className={styles.capacity} role="alert">{deviceErrors.get(device.path)}</p> : null}
                {expanded ? <div className={styles.library} role="group" aria-label={`${device.name} library`}>
                  {deviceErrors.has(device.path) ? <p role="alert">{deviceErrors.get(device.path)}</p> : read === undefined ? <div className={styles.libraryNote}>Reading…</div>
                    : <DeviceLibraries libraries={read.libraries ?? [{ name: "Device Library", nodes: read.onDevice.map((name, i) => ({ id: String(i+1), parentId: "0", name, folder: false })) }]} />}
                </div> : null}
              </div>;
            })}
            {devices.length === 0 ? <p className={styles.empty}>{loadingDevices ? "Looking for USB devices…" : "Connect a USB device, then click Refresh."}</p> : null}
          </div>
        </section>
      </div>
      <footer className={styles.footer}>
        <div className={styles.status} role="status" aria-live="polite">
          {busy ? <LoaderCircle size={16} className={styles.spinner} aria-hidden="true" /> : null}
          <div>
          {status.map((line) => (
            <div key={line}>{line}</div>
          ))}
          {status.length === 0 ? <span className={styles.selectionSummary}>{selectionSummary}</span> : null}
          {status.length === 0 ? <span id="sync-selection-hint" className={styles.idleStatus}>{syncHint}</span> : null}
          </div>
        </div>
        {status.length > 0 ? <span id="sync-selection-hint" hidden>{syncHint}</span> : null}
        <button type="button" className={styles.button} onClick={onClose}>
          {busy ? "Run in background" : "Close"}
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

function DeviceLibraries({ libraries }: { libraries: NonNullable<DeviceSyncState["libraries"]> }) {
  if (libraries.length === 0) return <p className={styles.libraryNote}>No libraries on this device yet.</p>;
  return <>{libraries.map(library => {
    const children = (parent: string, ancestors: Set<string>): React.ReactNode => library.nodes
      .filter(node => node.parentId === parent && !ancestors.has(node.id))
      .map(node => node.folder ? <details key={node.id} open role="treeitem">
        <summary><FolderIcon className={styles.icon} /> {node.name}</summary>
        <div role="group">{children(node.id, new Set([...ancestors, node.id]))}</div>
      </details> : <div key={node.id} className={styles.playlistLeaf} role="treeitem"><ListIcon className={styles.icon} /> {node.name}</div>);
    return <details key={library.name} open role="treeitem">
      <summary>{library.name}</summary>
      <div role="group"><details open role="treeitem">
        <summary><FolderIcon className={styles.icon} /> Playlists</summary>
        <div role="group">{library.nodes.length ? children("0", new Set()) : <p className={styles.libraryNote}>No playlists on this device yet.</p>}</div>
      </details></div>
    </details>;
  })}</>;
}
