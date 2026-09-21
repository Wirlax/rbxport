import styles from "./StatusBar.module.css";
import { useTooltip } from "@/store/usePreferences";
import { refusal } from "@/lib/menu";

export interface StatusBarProps {
  /** The build's version, shown beside the name; null until it is read. */
  version?: string | null;
  /** General status text; empty when idle. */
  activity?: string;
  analysisProgress?: { completed: number; total: number } | undefined;
  backupActivity?: string;
  backupProgress?: { phase: string; copiedBytes: number; totalBytes: number } | undefined;
  /**
   * Something went wrong, said in red.
   *
   * Errors belong here rather than over the thing that raised them: the
   * player used to print its own across the pad row and the tree beneath it.
   */
  error?: string | null;
  /** e.g. "Selected: 4 Tracks, 18 minutes, 58.4 MB"; empty when nothing is selected. */
  selection?: string;
  readOnly?: boolean;
  /** Library Protection in Preferences is why, rather than rekordbox running. */
  protectedLibrary?: boolean;
  onExplainReadOnly?: (() => void) | undefined;
  onOpenProtection?: (() => void) | undefined;
  onReportBug?: (() => void) | undefined;
  /** Present only while analysis is running, so it can be stopped. */
  onCancelAnalysis?: (() => void) | undefined;
  /** Tracks that failed analysis in the current run. */
  analysisFailures?: number;
}

export function StatusBar({
  version = null,
  activity = "",
  analysisProgress,
  backupActivity = "",
  backupProgress,
  error = null,
  selection = "",
  readOnly = false,
  protectedLibrary = false,
  onExplainReadOnly,
  onOpenProtection,
  onReportBug,
  onCancelAnalysis,
  analysisFailures = 0,
}: StatusBarProps) {
  const tip = useTooltip();
  const analysisPercent = analysisProgress && analysisProgress.total > 0
    ? Math.min(100, Math.max(0, Math.floor(analysisProgress.completed / analysisProgress.total * 100))) : 0;
  const backupPercent = backupProgress && backupProgress.totalBytes > 0
    ? Math.min(100, Math.max(0, Math.floor(backupProgress.copiedBytes / backupProgress.totalBytes * 100))) : 0;
  return (
    <footer className={styles.statusBar}>
      <span className={styles.logo}>
        rbxport
        {version === null ? null : <> <span className={styles.version}>{version}</span></>}
      </span>
      {readOnly ? (
        <button type="button" className={styles.readOnly} title={refusal(protectedLibrary)} onClick={onExplainReadOnly}>
          Read-only
        </button>
      ) : null}
      {/*
        Stop comes before the progress text, not after it. After it the button
        slides left and right as the counter's width changes, which makes it
        hard to hit — a moving target for a person, and unclickable for a test.
      */}
      {onCancelAnalysis ? (
        <button type="button" className={styles.stop} onClick={onCancelAnalysis}>
          Stop
        </button>
      ) : null}
      {error === null || error === "" ? null : (
        <span className={styles.error} role="alert">
          {error}
          {onOpenProtection ? (
            <button type="button" className={styles.warningAction} onClick={onOpenProtection}>
              Open Preferences
            </button>
          ) : null}
        </span>
      )}
      {backupProgress ? <span className={styles.backupMeter} title={backupActivity}>
        <span>Backup</span>
        <progress className={styles.backupProgress} aria-label="Backup progress"
          max={100} value={backupProgress.totalBytes > 0 ? backupPercent : undefined} />
        <span className={styles.backupPercent}>({backupPercent}%)</span>
      </span>
        : backupActivity ? <span className={styles.backupActivity} role="status">{backupActivity}</span> : null}
      {analysisProgress ? <span className={styles.analysisMeter}>
        <span>Analyzing {analysisProgress.total} {analysisProgress.total === 1 ? "track" : "tracks"}</span>
        <progress className={styles.backupProgress} aria-label="Analysis progress"
          max={100} value={analysisPercent} />
        <span className={styles.backupPercent}>({analysisPercent}%)</span>
      </span> : <span className={styles.activity}>{activity}</span>}
      {analysisFailures > 0 ? (
        <span
          className={styles.failures}
          title={tip("These tracks could not be analysed; the run carried on past them")}
        >
          {analysisFailures} failed
        </span>
      ) : null}
      <span className={styles.selection}>{selection}</span>
      {onReportBug ? <button type="button" className={styles.reportBug} onClick={onReportBug}>Report bug</button> : null}
    </footer>
  );
}
