import { errorMessage } from "@/lib/errorMessage";
import { useCallback, useEffect, useRef, useState } from "react";
import { getBackend } from "@/ipc/client";
import type { BackupProgress } from "@/ipc/types";
import { formatBytes } from "@/lib/format";

const EMPTY: BackupProgress = { running: false, phase: "", copiedBytes: 0, totalBytes: 0, error: null, path: null };

export function backupStatus(progress: BackupProgress): string {
  switch (progress.phase) {
    case "preparing": return "Preparing backup…";
    case "copying": return `Creating backup: ${progress.totalBytes > 0 ? Math.min(100, Math.floor(progress.copiedBytes / progress.totalBytes * 100)) : 0}% — ${formatBytes(progress.copiedBytes)} of ${formatBytes(progress.totalBytes)}${progress.currentItem ? ` · ${progress.currentItem}` : ""}`;
    case "compressing": return "Compressing backup…";
    case "validating": return "Verifying backup…";
    case "stopping": return "Stopping backup…";
    case "complete": return "Backup created.";
    case "cancelled": return "Backup stopped.";
    case "failed": return progress.error ?? "Backup failed.";
    default: return "";
  }
}

/** The backend owns the job; every window can reconnect without restarting it. */
export function useBackupProgress() {
  const [progress, setProgress] = useState(EMPTY);
  const [requestError, setRequestError] = useState("");
  const pending = useRef(false);
  useEffect(() => {
    let live = true;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = async () => {
      try {
        const next = await (await getBackend()).backupProgress();
        if (live && !pending.current) setProgress(next);
      } catch {
        // Preserve the last known job while the backend is temporarily unavailable.
      } finally {
        if (live) timer = setTimeout(() => void refresh(), 500);
      }
    };
    void refresh();
    return () => { live = false; clearTimeout(timer); };
  }, []);
  const start = useCallback(async () => {
    if (pending.current || progress.running) return;
    pending.current = true;
    setRequestError("");
    setProgress({ ...EMPTY, running: true, phase: "preparing" });
    try {
      const backend = await getBackend();
      await backend.startBackup();
      setProgress(await backend.backupProgress());
    } catch (e) {
      setRequestError(errorMessage(e));
      setProgress(EMPTY);
    } finally { pending.current = false; }
  }, [progress.running]);
  const stop = useCallback(async () => {
    setRequestError("");
    try {
      const backend = await getBackend();
      await backend.cancelBackup();
      setProgress(await backend.backupProgress());
    } catch (e) { setRequestError(errorMessage(e)); }
  }, []);
  return { progress, start, stop, text: backupStatus(progress), error: requestError || progress.error };
}
