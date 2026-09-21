import { useEffect, useRef, useState } from "react";
import { getBackend } from "@/ipc/client";
import type { Backend, Backup } from "@/ipc/types";
import { formatBytes } from "@/lib/format";
import { useBackupProgress } from "@/store/useBackupProgress";
import { usePreferences } from "@/store/usePreferences";
import { Button, Section } from "./controls";
import styles from "./BackupsPane.module.css";
import { BackupSizeChart } from "./BackupSizeChart";
import layout from "./PaneLayout.module.css";

export function BackupsPane({ readOnly = false }: { readOnly?: boolean }) {
  const job = useBackupProgress();
  const [backups, setBackups] = useState<Backup[]>([]);
  const [directory, setDirectory] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const preferences = usePreferences();
  const protectedLibrary = useRef(preferences.advanced.protectLibrary);
  protectedLibrary.current = preferences.advanced.protectLibrary;
  const running = useRef(false);
  useEffect(() => {
    let live = true;
    void getBackend().then(b => Promise.all([b.listBackups(), b.backupDirectory()])).then(([entries, path]) => {
      if (live) { setBackups(entries); setDirectory(path); }
    }).catch(e => { if (live) setError(String(e instanceof Error ? e.message : e)); })
      .finally(() => { if (live) setLoading(false); });
    return () => { live = false; };
  }, []);
  useEffect(() => {
    if (!job.progress.path) return;
    let live = true;
    void getBackend().then(b => b.listBackups()).then(entries => {
      if (live) setBackups(entries);
    }).catch(e => { if (live) setError(String(e)); });
    return () => { live = false; };
  }, [job.progress.path]);
  const run = async (label: string, action: (backend: Backend) => Promise<string>) => {
    if (running.current) return;
    running.current = true;
    setBusy(label); setError(""); setMessage("");
    try {
      const backend = await getBackend();
      const message = await action(backend);
      setMessage(message);
      setBackups(await backend.listBackups());
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { running.current = false; setBusy(""); }
  };
  const unavailable = loading || busy !== "" || job.progress.running;
  return <Section title="Backups">
    <section className={layout.summary} aria-label="Backup your Library">
      <div>
        <strong>Backup your Library</strong>
        <p className={layout.help}>Backs up the database, memory, hot cues, beat grid, and waveform previews. Music files are not backed up.</p>
        <p className={styles.status} role="status" aria-live="polite">{job.progress.running ? job.text : busy || message || job.text || (loading ? "Loading backups…" : "")}</p>
        {job.progress.running ? <p className={layout.help}>Backup started in the background. You can continue to use rbexport while it's backing up.</p> : null}
        {job.progress.running ? <div className={styles.actions}>
          <progress aria-label="Backup progress" max={job.progress.totalBytes || 1}
            value={job.progress.phase === "copying" ? job.progress.copiedBytes : undefined} />
          <Button disabled={job.progress.phase === "stopping"} onClick={() => void job.stop()}>Stop backup</Button>
        </div> : null}
      </div>
      <Button disabled={unavailable || readOnly} onClick={() => {
        setError(""); setMessage(""); void job.start();
      }}>Create backup</Button>
    </section>
    {readOnly ? <p className={layout.notice}>Quit rekordbox before creating or restoring a backup.</p> : null}
    {preferences.advanced.protectLibrary ? <p className={layout.notice}>Library Protection is on. Turn it off in Advanced to restore a backup.</p> : null}
    {error || job.error ? <p role="alert" className={styles.error}>{error || job.error}</p> : null}

    <BackupSizeChart />
    <div className={layout.heading}>
      <h4>Saved backups <span className={layout.count}>{loading ? "—" : backups.length}</span></h4>
      {!loading && backups.length > 0 ? <span>{formatBytes(backups.reduce((total, backup) => total + backup.bytes, 0))} total</span> : null}
    </div>
    {directory ? <p className={layout.help}>Your backups will be stored in <button type="button" role="link" className={styles.directoryLink} onClick={() => {
      setError("");
      void getBackend().then(b => b.openBackupDirectory()).catch(e => setError(e instanceof Error ? e.message : String(e)));
    }}>{directory}</button></p> : null}
    {!loading && backups.length === 0 ? <div className={layout.empty}>
      <strong>{error ? "Backups unavailable" : "No backups yet."}</strong>
      <p>{error ? "Reopen this page to try again." : "Create your first backup to save a restore point for your library."}</p>
    </div> : null}
    {backups.length > 0 ? <div className={styles.tableScroll}><table className={styles.table} aria-label="Library backups">
      <thead><tr><th>Date</th><th>Size</th><th>Includes</th><th>Actions</th></tr></thead>
      <tbody>{backups.map(backup => {
        const date = new Date(backup.createdAt).toLocaleString();
        return <tr key={backup.path}>
          <td><time dateTime={new Date(backup.createdAt).toISOString()}>{date}</time></td>
          <td className={styles.size}>{formatBytes(backup.bytes)}</td>
          <td>{backup.includesAnalysis ? "Database + analysis" : "Database only"}</td>
          <td><div className={styles.actions}>
            <Button disabled={unavailable || readOnly || preferences.advanced.protectLibrary} onClick={() => void run("Restoring backup…", async b => {
              if (protectedLibrary.current) throw new Error("Turn off Library Protection before restoring.");
              if (!await b.confirm(`Restore the backup from ${date}? This replaces your current library${backup.includesAnalysis ? " and cue/grid analysis" : " database"}. Changes made since this backup will be lost.`)) return "";
              if (protectedLibrary.current) throw new Error("Turn off Library Protection before restoring.");
              await b.restoreBackup(backup.path);
              return "Backup restored.";
            })}>Restore</Button>
            <Button disabled={unavailable} onClick={() => void run("Deleting backup…", async b => {
              if (!await b.confirm(`Delete the backup from ${date}? This cannot be undone.`)) return "";
              await b.deleteBackup(backup.path); return "Backup deleted.";
            })}>Delete</Button>
          </div></td>
        </tr>;
      })}</tbody>
    </table></div> : null}
  </Section>;
}
