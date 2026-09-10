/**
 * The icon column at the right edge of the browser.
 *
 * Five outlined boxes, top to bottom: My Tag, Related Tracks, Track
 * Suggestion, Information, Sub-Browser — measured from the 2026-09-09
 * capture (`docs/screenshots`, `9.09.26 PM`) [OBS], where the sub-browser's
 * box is lit blue because it is open. The wording is german.lang's.
 *
 * Only the last two do anything here. My Tag, Related Tracks and Track
 * Suggestion open panels this app does not have, so their boxes are drawn
 * and disabled rather than left out: a column with two buttons in it reads as
 * a different program.
 */
import type { SVGProps } from "react";

import {
  InfoIcon, MyTagIcon, RelatedIcon, SubBrowseIcon, SuggestIcon,
} from "@/components/icons";
import styles from "./RightRail.module.css";

type Icon = (props: SVGProps<SVGSVGElement>) => React.ReactElement;

const INERT: ReadonlyArray<{ label: string; title: string; Icon: Icon }> = [
  { label: "My Tag", title: "Display My Tag configuration window.", Icon: MyTagIcon },
  { label: "Related Tracks", title: "Display related track list window.", Icon: RelatedIcon },
  { label: "Track Suggestion", title: "Display Track Suggestion window.", Icon: SuggestIcon },
];

export interface RightRailProps {
  /** Where the shell puts it, since the shell's grid decides that. */
  className?: string | undefined;
  infoOpen: boolean;
  onToggleInfo: () => void;
  subOpen: boolean;
  onToggleSub: () => void;
}

export function RightRail({ className, infoOpen, onToggleInfo, subOpen, onToggleSub }: RightRailProps) {
  return (
    <div className={className ? `${styles.rail} ${className}` : styles.rail} role="toolbar" aria-label="Browser panels" aria-orientation="vertical">
      <div className={styles.group}>
        {INERT.map(({ label, title, Icon }) => (
          <button
            key={label}
            type="button"
            className={styles.button}
            title={title}
            aria-label={label}
            disabled
          >
            <Icon className={styles.icon} />
          </button>
        ))}
        <button
          type="button"
          className={styles.button}
          title="Display information window."
          aria-label="Information"
          aria-pressed={infoOpen}
          onClick={onToggleInfo}
        >
          <InfoIcon className={styles.icon} />
        </button>
        <button
          type="button"
          className={styles.button}
          title="Display sub-browser window."
          aria-label="Sub-Browser Window"
          aria-pressed={subOpen}
          onClick={onToggleSub}
        >
          <SubBrowseIcon className={styles.icon} />
        </button>
      </div>
    </div>
  );
}
