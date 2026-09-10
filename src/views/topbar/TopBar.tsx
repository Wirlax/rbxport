/**
 * Top strip: settings, level, meters, clock.
 *
 * The order and the parts are rekordbox's own, read off a capture of 7.2.11
 * running: the settings gear, the master level knob, a two-channel output
 * meter, a processor meter, then the clock at the far right. The headphone
 * button that stood where the knob is now was ours, not rekordbox's.
 *
 * Deliberately not rekordbox's: its EXPORT mode dropdown and the layout and
 * record buttons at the left, and the info button and "Professional" plan
 * badge before the gear, are gone by request. The gear and everything after
 * it keep their places: they hang off the right edge, so nothing slides into
 * the room the two left. Recorded in TODO.md under "Deliberate divergences"
 * so nobody restores them in the name of matching 7.2.11.
 */
import { GearIcon } from "@/components/icons";
import { VolumeKnob } from "./VolumeKnob";
import type { PlayerLayout } from "@/lib/layout";
import { LayoutMenu } from "./LayoutMenu";
import styles from "./TopBar.module.css";

export interface TopBarProps {
  clock: string;
  onOpenSettings?: () => void;
  /**
   * Master output level, 0 to 1. There is no audio engine behind this yet, so
   * it is drawn at rest rather than omitted — an absent meter reads as a
   * missing feature, an empty one as silence, which is the truth.
   */
  level?: number;
  /** Master level, 0 to 1, and where a turn of the knob goes. */
  onLevelChange?: (level: number) => void;
  /** The loudest sample the device was given, per channel, 0 to 1. */
  peakLeft?: number;
  peakRight?: number;
  /** Processor, as a fraction of one core. */
  cpu?: number;
  /** How much of the window the deck takes. rekordbox puts this at the left. */
  layout?: PlayerLayout;
  onLayoutChange?: (layout: PlayerLayout) => void;
}

/** 0 to 1, and never a NaN: a NaN width is a bar that does not draw. */
function clamp(value: number): number {
  return Math.min(Math.max(Number.isFinite(value) ? value : 0, 0), 1);
}

export function TopBar({
  clock,
  onOpenSettings,
  level = 1,
  onLevelChange,
  peakLeft = 0,
  peakRight = 0,
  cpu = 0,
  layout = "one",
  onLayoutChange,
}: TopBarProps) {
  const filled = clamp(level);
  return (
    <header className={styles.topBar}>
      <LayoutMenu layout={layout} onChange={onLayoutChange} />

      <span className={styles.spacer} />

      <button
        type="button"
        className={styles.icon}
        onClick={onOpenSettings}
        aria-label="Settings"
      >
        <GearIcon className={styles.glyph} />
      </button>

      <VolumeKnob level={filled} onChange={onLevelChange} />

      {/* Two channels, stacked, as the capture has them. Peaks rather than an
          average: eleven milliseconds averaged is a meter that never moves. */}
      <div className={styles.vu} aria-label="Master output" role="group">
        {([["L", peakLeft], ["R", peakRight]] as const).map(([channel, peak]) => (
          <div
            key={channel}
            className={styles.meter}
            role="meter"
            aria-label={`Master output ${channel}`}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(clamp(peak) * 100)}
          >
            <span className={styles.meterFill} style={{ width: `${clamp(peak) * 100}%` }} />
          </div>
        ))}
      </div>

      <div
        className={`${styles.meter} ${styles.cpu}`}
        role="meter"
        aria-label="Processor"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(clamp(cpu) * 100)}
      >
        <span className={styles.meterFill} style={{ width: `${clamp(cpu) * 100}%` }} />
      </div>

      <span className={styles.clock} data-testid="clock">{clock}</span>
    </header>
  );
}
