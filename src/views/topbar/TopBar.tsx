import styles from "./TopBar.module.css";
import { GearIcon } from "@/components/icons";

export interface TopBarProps {
  plan?: string;
  clock: string;
  onOpenSettings?: () => void;
}

/**
 * Top strip: plan badge, clock, settings.
 *
 * Deliberately not rekordbox's: its EXPORT mode dropdown and the layout and
 * record buttons beside it are gone by request, and the settings gear sits at
 * the right-hand end. Recorded in TODO.md under "Deliberate divergences" so
 * nobody restores them in the name of matching 7.2.11.
 */
export function TopBar({ plan = "Professional", clock, onOpenSettings }: TopBarProps) {
  return (
    <header className={styles.topBar}>
      <span className={styles.spacer} />
      <span className={styles.plan}>{plan}</span>
      <span className={styles.clock}>{clock}</span>
      <button
        type="button"
        className={styles.settings}
        onClick={onOpenSettings}
        aria-label="Settings"
      >
        <GearIcon className={styles.gear} />
      </button>
    </header>
  );
}
