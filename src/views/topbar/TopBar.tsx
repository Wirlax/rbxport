import styles from "./TopBar.module.css";

export interface TopBarProps {
  plan?: string;
  clock: string;
}

/** Top strip: mode selector, layout/record controls, plan badge, clock. */
export function TopBar({ plan = "Professional", clock }: TopBarProps) {
  return (
    <header className={styles.topBar}>
      <span className={styles.mode}>EXPORT</span>
      <span className={styles.chevron} aria-hidden />
      <span className={styles.layout} aria-hidden />
      <span className={styles.record} aria-hidden />
      <span className={styles.spacer} />
      <span className={styles.plan}>{plan}</span>
      <span className={styles.clock}>{clock}</span>
    </header>
  );
}
