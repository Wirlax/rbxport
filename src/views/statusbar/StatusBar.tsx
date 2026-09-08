import styles from "./StatusBar.module.css";

export interface StatusBarProps {
  /** e.g. "Analyzing: 8319 Tracks"; empty when idle. */
  activity?: string;
  /** e.g. "Selected: 4 Tracks, 18 minutes, 58.4 MB"; empty when nothing is selected. */
  selection?: string;
  readOnly?: boolean;
  /** Present only while analysis is running, so it can be stopped. */
  onCancelAnalysis?: (() => void) | undefined;
  /** Tracks that failed analysis in the current run. */
  analysisFailures?: number;
}

export function StatusBar({
  activity = "",
  selection = "",
  readOnly = false,
  onCancelAnalysis,
  analysisFailures = 0,
}: StatusBarProps) {
  return (
    <footer className={styles.statusBar}>
      <span className={styles.logo}>rekordbox-lite</span>
      {readOnly ? (
        <span className={styles.readOnly} title="rekordbox is running, so the library is open read-only">
          Read-only
        </span>
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
      <span className={styles.activity}>{activity}</span>
      {analysisFailures > 0 ? (
        <span
          className={styles.failures}
          title="These tracks could not be analysed; the run carried on past them"
        >
          {analysisFailures} failed
        </span>
      ) : null}
      <span className={styles.selection}>{selection}</span>
    </footer>
  );
}
