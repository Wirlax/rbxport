import { useEffect, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { getBackend } from "@/ipc/client";
import type { BackupSizes } from "@/ipc/types";
import { formatBytes } from "@/lib/format";
import { Button } from "./controls";
import styles from "./BackupsPane.module.css";

const PARTS = [
  { key: "database", label: "Database", color: "var(--c-accent)" },
  { key: "waveforms", label: "Waveform previews", color: "var(--c-label-aqua)" },
  { key: "cues", label: "Memory & hot cues", color: "var(--c-label-orange)" },
  { key: "beatGrids", label: "Beat grids", color: "var(--c-label-purple)" },
  { key: "phrases", label: "Phrase analysis", color: "var(--c-label-pink)" },
  { key: "artwork", label: "Artwork thumbnails", color: "var(--c-label-green)" },
  { key: "vocals", label: "Vocal analysis", color: "var(--c-label-yellow)" },
  { key: "other", label: "Other analysis", color: "var(--c-text-dim)" },
] as const;

function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  return formatBytes(bytes);
}

function UpdatedTime({ timestamp }: { timestamp: number }) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, []);
  const minutes = Math.max(0, Math.floor((now - timestamp) / 60_000));
  const count = minutes < 60 ? minutes : minutes < 1440 ? Math.floor(minutes / 60) : Math.floor(minutes / 1440);
  const unit = minutes < 60 ? "minute" : minutes < 1440 ? "hour" : "day";
  const label = minutes === 0 ? "just now" : `${count} ${unit}${count === 1 ? "" : "s"} ago`;
  return <time dateTime={new Date(timestamp).toISOString()} title={new Date(timestamp).toLocaleString()}>{label}</time>;
}

export function BackupSizeChart() {
  const [sizes, setSizes] = useState<BackupSizes | null>(null);
  const [failed, setFailed] = useState(false);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let live = true;
    setLoading(true);
    setFailed(false);
    void getBackend().then(b => b.backupSizes(attempt > 0)).then(value => {
      if (live) setSizes(value);
    }).catch(() => { if (live) setFailed(true); })
      .finally(() => { if (live) setLoading(false); });
    return () => { live = false; };
  }, [attempt]);
  const total = sizes ? PARTS.reduce((sum, part) => sum + sizes[part.key], 0) : 0;
  return <figure className={styles.breakdown} aria-label="Backup contents">
    <figcaption className={styles.breakdownHeading}>
      <strong>Rekordbox Data</strong>
      {sizes ? <span>{size(total)} total · {sizes.trackCount.toLocaleString()} {sizes.trackCount === 1 ? "track" : "tracks"}</span> : null}
    </figcaption>
    <div className={styles.sizeBarFrame} aria-busy={loading}>
      <div className={styles.sizeBar} role="img" aria-label={!sizes ? "Rekordbox data size not calculated yet" : total === 0 ? "No data to back up" : PARTS.map(part => `${part.label}: ${size(sizes[part.key])}`).join(", ")}>
        {sizes && total > 0 ? PARTS.filter(part => sizes[part.key] > 0).map(part => <span key={part.key}
          title={`${part.label}: ${size(sizes[part.key])} (${(sizes[part.key] / total * 100).toFixed(1)}%)`}
          style={{ width: `${sizes[part.key] / total * 100}%`, backgroundColor: part.color }} />) : null}
      </div>
      {loading ? <div className={styles.sizeCalculating} role="status">
        <LoaderCircle size={16} className={styles.sizeSpinner} aria-hidden="true" />
        Calculating Rekordbox data size
      </div> : null}
    </div>
    <ul className={styles.sizeLegend} aria-label="Backup size breakdown">
      {PARTS.map(part => <li key={part.key}>
        <span className={styles.sizeDot} style={{ backgroundColor: part.color }} aria-hidden="true" />
        <span>{part.label}</span><span className={styles.sizeValue}>{sizes ? size(sizes[part.key]) : "—"}</span>
      </li>)}
    </ul>
    {failed ? <p className={styles.error} role="alert">Couldn’t calculate Rekordbox data size. Click Refresh to try again.</p> : null}
    <div className={styles.sizeFooter}>
      <span>(Last updated: {sizes ? <UpdatedTime key={sizes.updatedAt} timestamp={sizes.updatedAt} /> : "—"})</span>
      <Button disabled={loading} onClick={() => { setLoading(true); setAttempt(value => value + 1); }}>Refresh</Button>
    </div>
  </figure>;
}
