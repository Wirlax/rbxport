import styles from "./StatusBar.module.css";
import { useTooltip } from "@/store/usePreferences";
import { refusal } from "@/lib/menu";

export interface StatusBarProps {
  /** e.g. "Analyzing: 8319 Tracks"; empty when idle. */
  activity?: string;
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
  /** Present only while analysis is running, so it can be stopped. */
  onCancelAnalysis?: (() => void) | undefined;
  /** Tracks that failed analysis in the current run. */
  analysisFailures?: number;
}

export function StatusBar({
  activity = "",
  error = null,
  selection = "",
  readOnly = false,
  protectedLibrary = false,
  onCancelAnalysis,
  analysisFailures = 0,
}: StatusBarProps) {
  const tip = useTooltip();
  return (
    <footer className={styles.statusBar}>
      <span className={styles.logo}>rekordbox-lite</span>
      {readOnly ? (
        <span className={styles.readOnly} title={tip(refusal(protectedLibrary))}>
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
      {error === null || error === "" ? null : (
        <span className={styles.error} role="alert">
          {error}
        </span>
      )}
      <span className={styles.activity}>{activity}</span>
      {analysisFailures > 0 ? (
        <span
          className={styles.failures}
          title={tip("These tracks could not be analysed; the run carried on past them")}
        >
          {analysisFailures} failed
        </span>
      ) : null}
      <span className={styles.selection}>{selection}</span>
    </footer>
  );
}
