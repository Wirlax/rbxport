import styles from "./App.module.css";

/**
 * Export-mode shell. Regions are filled in over Milestone 0 (see docs/PLAN.md):
 * player · tree · browser · status bar. The layout constants come from
 * design/tokens/tokens.json, measured from the real app.
 */
export function App() {
  return (
    <div className={styles.window}>
      <header className={styles.topBar}>
        <span className={styles.mode}>EXPORT</span>
      </header>
      <section className={styles.player} aria-label="Preview player" />
      <div className={styles.body}>
        <nav className={styles.tree} aria-label="Library tree" />
        <main className={styles.browser} aria-label="Track browser" />
      </div>
      <footer className={styles.statusBar}>
        <span>rekordbox-lite</span>
      </footer>
    </div>
  );
}
