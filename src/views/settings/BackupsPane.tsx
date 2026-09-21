import { errorMessage } from "@/lib/errorMessage";
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
    }).catch(e => { if (live) setError(errorMessage(e)); })
      .finally(() => { if (live) setLoading(false); });
    return () => { live = false; };
  }, []);
  useEffect(() => {
    if (!job.progress.path) return;
    let live = true;
    void getBackend().then(b => b.listBackups()).then(entries => {
      if (live) setBackups(entries);
    }).catch(e => { if (live) setError(errorMessage(e)); });
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
    } catch (e) { setError(errorMessage(e)); }
    finally { running.current = false; setBusy(""); }
  };
  const unavailable = loading || busy !== "" || job.progress.running;
  const copying = job.progress.phase === "copying" && job.progress.totalBytes > 0;
  const percent = Math.min(100, Math.max(0, Math.floor(job.progress.copiedBytes / (job.progress.totalBytes || 1) * 100)));
  const stopping = job.progress.phase === "stopping";
  return <Section title="Backups">
    <section className={`${layout.summary} ${job.progress.running ? styles.activeBackup : ""}`} aria-label="Backup your Library">
      {job.progress.running ? <div className={styles.backupProgress}>
        <div className={styles.progressHeading} role="status" aria-live="polite">
          <strong>{job.progress.phase === "copying" ? "Backing up your library" : job.text}</strong>
          {copying ? <span className={styles.percent}>{percent}%</span> : null}
        </div>
        <progress className={styles.progressBar} aria-label="Backup progress" max={100}
          value={copying ? percent : undefined} />
        <div className={styles.progressDetails}>
          {copying ? `${formatBytes(job.progress.copiedBytes)} of ${formatBytes(job.progress.totalBytes)}`
            : stopping ? "Removing the unfinished backup…"
              : job.progress.phase === "compressing" ? "Finishing your compressed ZIP backup."
              : job.progress.phase === "validating" ? "Checking the saved files before finishing."
                : "Getting your library files ready."}
        </div>
        {!stopping && job.progress.currentItem ? <div className={styles.currentItem} title={job.progress.currentItem} aria-label="Current backup item">
          {job.progress.currentItem}
        </div> : null}
        <div className={styles.progressFooter}>
          <p>You can keep using RBXport while this runs.</p>
          <Button disabled={stopping} onClick={() => void job.stop()}>{stopping ? "Stopping…" : "Stop backup"}</Button>
        </div>
      </div> : <>
      <div>
        <strong>Backup your Library</strong>
        <p className={layout.help}>Save your library in a compressed ZIP. Music files are not backed up.</p>
        <p className={styles.status} role="status" aria-live="polite">{busy || message || (job.error ? "" : job.text) || (loading ? "Loading backups…" : "")}</p>
      </div>
      <Button disabled={unavailable || readOnly} onClick={() => {
        setError(""); setMessage(""); void job.start();
      }}>Create backup</Button>
      </>}
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
      void getBackend().then(b => b.openBackupDirectory()).catch(e => setError(errorMessage(e)));
    }}>{directory}</button></p> : null}
    {!loading && backups.length === 0 ? <div className={layout.empty}>
      <strong>{error ? "Backups unavailable" : "No backups yet."}</strong>
      <p>{error ? "Reopen this page to try again." : "Create your first backup to save a restore point for your library."}</p>
    </div> : null}
    {backups.length > 0 ? <div className={styles.tableScroll}><table className={styles.table} aria-label="Library backups">
      <thead><tr><th>Date</th><th>Size on disk</th><th>Includes</th><th>Actions</th></tr></thead>
      <tbody>{backups.map(backup => {
        const date = new Date(backup.createdAt).toLocaleString();
        return <tr key={backup.path}>
          <td><time dateTime={new Date(backup.createdAt).toISOString()}>{date}</time></td>
          <td className={styles.size}>{formatBytes(backup.bytes)}</td>
          <td>{backup.includesArtwork ? "Database + analysis + artwork" : backup.includesAnalysis ? "Database + analysis" : "Database only"}</td>
          <td><div className={styles.actions}>
            <Button disabled={unavailable || readOnly || preferences.advanced.protectLibrary} onClick={() => void run("Restoring backup…", async b => {
              if (protectedLibrary.current) throw new Error("Turn off Library Protection before restoring.");
              if (!await b.confirm(`Restore the backup from ${date}? This replaces your current library${backup.includesArtwork ? ", analysis, and artwork" : backup.includesAnalysis ? " and cue/grid analysis" : " database"}. Changes made since this backup will be lost.`)) return "";
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
