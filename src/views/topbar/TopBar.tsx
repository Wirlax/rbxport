/**
 * Top strip: information, plan badge, settings, monitoring, master level, clock.
 *
 * The order and the parts are rekordbox's own, read off a capture of 7.2.11
 * running: an info button, the "Professional" badge, the settings gear, a
 * headphone button, a horizontal level meter, then the clock at the far right.
 *
 * Deliberately not rekordbox's: its EXPORT mode dropdown and the layout and
 * record buttons at the left are gone by request. Recorded in TODO.md under
 * "Deliberate divergences" so nobody restores them in the name of matching
 * 7.2.11.
 */
import { GearIcon, HeadphonesIcon, InfoIcon } from "@/components/icons";
import styles from "./TopBar.module.css";

export interface TopBarProps {
  plan?: string;
  clock: string;
  onOpenSettings?: () => void;
  /**
   * Master output level, 0 to 1. There is no audio engine behind this yet, so
   * it is drawn at rest rather than omitted — an absent meter reads as a
   * missing feature, an empty one as silence, which is the truth.
   */
  level?: number;
  /** Whether headphone monitoring is on. */
  monitoring?: boolean;
  onToggleMonitoring?: () => void;
}

export function TopBar({
  plan = "Professional",
  clock,
  onOpenSettings,
  level = 0,
  monitoring = false,
  onToggleMonitoring,
}: TopBarProps) {
  const filled = Math.min(Math.max(Number.isFinite(level) ? level : 0, 0), 1);
  return (
    <header className={styles.topBar}>
      <span className={styles.spacer} />

      <button type="button" className={styles.icon} aria-label="Information">
        <InfoIcon className={styles.glyph} />
      </button>

      <span className={styles.plan}>{plan}</span>

      <button
        type="button"
        className={styles.icon}
        onClick={onOpenSettings}
        aria-label="Settings"
      >
        <GearIcon className={styles.glyph} />
      </button>

      <button
        type="button"
        className={styles.icon}
        onClick={onToggleMonitoring}
        aria-label="Headphone monitoring"
        aria-pressed={monitoring}
        data-on={monitoring || undefined}
      >
        <HeadphonesIcon className={styles.glyph} />
      </button>

      <div
        className={styles.meter}
        role="meter"
        aria-label="Master level"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(filled * 100)}
      >
        <span className={styles.meterFill} style={{ width: `${filled * 100}%` }} />
      </div>

      <span className={styles.clock} data-testid="clock">{clock}</span>
    </header>
  );
}
