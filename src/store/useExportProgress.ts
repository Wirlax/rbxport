import { useEffect, useState } from "react";
import { getBackend } from "@/ipc/client";
import type { ExportProgress } from "@/ipc/types";

export function exportPercent(progress: ExportProgress): number {
  return progress.state === "done" ? 100 : progress.total > 0
    ? Math.min(99, Math.max(0, Math.floor(progress.done / progress.total * 100))) : 0;
}

/** All windows follow the backend job, independently of the initiating view. */
export function useExportProgress() {
  const [jobs, setJobs] = useState<ReadonlyMap<string, ExportProgress>>(new Map());
  useEffect(() => {
    let live = true;
    let stop: (() => void) | undefined;
    void getBackend().then(backend => {
      if (!live) return;
      const updated = new Set<string>();
      stop = backend.onExportProgress(progress => {
        updated.add(progress.path);
        if (live) setJobs(current => new Map(current).set(progress.path, progress));
      });
      void backend.exportProgress().then(jobs => {
        if (live) setJobs(current => {
          const next = new Map(current);
          for (const job of jobs) if (!updated.has(job.path)) next.set(job.path, job);
          return next;
        });
      }).catch(() => {});
    });
    return () => { live = false; stop?.(); };
  }, []);
  return jobs;
}
