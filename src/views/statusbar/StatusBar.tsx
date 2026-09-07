import styles from "./StatusBar.module.css";

export interface StatusBarProps {
  /** e.g. "Analyzing: 8319 Tracks"; empty when idle. */
  activity?: string;
  /** e.g. "Selected: 4 Tracks, 18 minutes, 58.4 MB"; empty when nothing is selected. */
  selection?: string;
  readOnly?: boolean;
}

export function StatusBar({ activity = "", selection = "", readOnly = false }: StatusBarProps) {
  return (
    <footer className={styles.statusBar}>
      <span className={styles.logo}>rekordbox-lite</span>
      {readOnly ? (
        <span className={styles.readOnly} title="rekordbox is running, so the library is open read-only">
          Read-only
        </span>
      ) : null}
      <span className={styles.activity}>{activity}</span>
      <span className={styles.selection}>{selection}</span>
    </footer>
  );
}
