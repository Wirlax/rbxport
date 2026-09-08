/**
 * The rail beside the tree: which part of the library the tree is showing.
 *
 * rekordbox calls this the tree shortcut column (`browseSetting.xml`,
 * `TreeShortcut`, width 57). Selecting one narrows the tree to that section
 * rather than opening a different window.
 */
import type { SVGProps } from "react";

import { CollectionIcon, DeviceIcon, FolderIcon, HistoryIcon } from "@/components/icons";
import styles from "./SourceRail.module.css";

export type Source = "collection" | "playlists" | "histories" | "devices";

const SOURCES: ReadonlyArray<{
  id: Source;
  label: string;
  Icon: (props: SVGProps<SVGSVGElement>) => React.ReactElement;
}> = [
  { id: "collection", label: "Collection", Icon: CollectionIcon },
  { id: "playlists", label: "Playlists", Icon: FolderIcon },
  { id: "histories", label: "Histories", Icon: HistoryIcon },
  { id: "devices", label: "Devices", Icon: DeviceIcon },
];

export interface SourceRailProps {
  selected: Source;
  onSelect: (source: Source) => void;
  /** Sources with nothing in them, shown but dimmed rather than hidden. */
  empty?: ReadonlySet<Source>;
}

export function SourceRail({ selected, onSelect, empty }: SourceRailProps) {
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
          title={label}
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
