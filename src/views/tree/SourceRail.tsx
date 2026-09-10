/**
 * The rail beside the tree: which part of the library the tree is showing.
 *
 * rekordbox calls this the tree shortcut column (`browseSetting.xml`,
 * `TreeShortcut`, width 57). Selecting one jumps the tree to that section; it
 * is a shortcut rather than a filter, so nothing is hidden.
 *
 * No Collection button: All Tracks is the first row of the tree and never out
 * of sight, so jumping to it is jumping to where you already are.
 */
import type { SVGProps } from "react";

import type { Source } from "@/lib/tree";

import { DeviceIcon, ExplorerIcon, FolderIcon, HistoryIcon } from "@/components/icons";
import styles from "./SourceRail.module.css";
import { useTooltip } from "@/store/usePreferences";

const SOURCES: ReadonlyArray<{
  id: Source;
  label: string;
  Icon: (props: SVGProps<SVGSVGElement>) => React.ReactElement;
}> = [
  { id: "playlists", label: "Playlists", Icon: FolderIcon },
  { id: "histories", label: "Histories", Icon: HistoryIcon },
  // Above Devices, as rekordbox's rail has it: its tooltip there is
  // `Display Explorer` (german.lang); the label is the section's own name.
  { id: "explorer", label: "Explorer", Icon: ExplorerIcon },
  { id: "devices", label: "Devices", Icon: DeviceIcon },
];

export interface SourceRailProps {
  selected: Source;
  onSelect: (source: Source) => void;
  /** Sources with nothing in them, shown but dimmed rather than hidden. */
  empty?: ReadonlySet<Source>;
}

export function SourceRail({ selected, onSelect, empty }: SourceRailProps) {
  const tip = useTooltip();
  return (
    <div className={styles.rail} role="tablist" aria-label="Library sources" aria-orientation="vertical">
      {SOURCES.map(({ id, label, Icon }) => (
        <button
          key={id}
          type="button"
          className={styles.button}
          role="tab"
          aria-selected={selected === id}
          // Dimmed rather than removed: a missing Devices button reads as a
          // broken app, an empty one reads as no device plugged in.
          data-empty={empty?.has(id) || undefined}
          title={tip(label)}
          aria-label={label}
          onClick={() => onSelect(id)}
        >
          <Icon className={styles.icon} />
          <span className={styles.label}>{label}</span>
        </button>
      ))}
    </div>
  );
}
