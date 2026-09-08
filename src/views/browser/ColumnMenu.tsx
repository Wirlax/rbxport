/**
 * The header's right-click menu: auto-size, then every column with a tick
 * beside the ones on show.
 *
 * Transcribed from rekordbox's own menu, so the order is the catalogue's and
 * the two auto-size actions sit above a separator at the top.
 */
import { useEffect, useRef } from "react";

import { CATALOGUE, type ColumnKey } from "@/lib/columns";
import styles from "./ColumnMenu.module.css";

/** Matches `.menu`'s min-width, for keeping the menu inside the window. */
const MENU_WIDTH = 240;
/** Never squeezed below this; past it the menu is placed higher instead. */
const MIN_HEIGHT = 220;

export interface ColumnMenuProps {
  x: number;
  y: number;
  /** The column the menu was opened on, for "Auto-size this column". */
  target: ColumnKey | null;
  visible: readonly ColumnKey[];
  onToggle: (key: ColumnKey) => void;
  onAutoSize: (key: ColumnKey) => void;
  onAutoSizeAll: () => void;
  onClose: () => void;
}

export function ColumnMenu({
  x, y, target, visible, onToggle, onAutoSize, onAutoSizeAll, onClose,
}: ColumnMenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Any click outside, any scroll, or Escape closes it — a menu that
    // outlives the thing it was opened on is worse than no menu.
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose]);

  const shown = new Set(visible);

  return (
    <div
      ref={ref}
      className={styles.menu}
      /*
        Thirty-nine items is taller than most windows, so the menu is placed
        and then capped to the room below it — the list inside scrolls. A plain
        clamp on `top` put the menu's *bottom* off screen and left the columns
        near the end unreachable.
      */
      style={{
        left: Math.min(x, window.innerWidth - MENU_WIDTH - 8),
        top: Math.min(y, Math.max(8, window.innerHeight - MIN_HEIGHT)),
        maxHeight: window.innerHeight - Math.min(y, Math.max(8, window.innerHeight - MIN_HEIGHT)) - 8,
      }}
      role="menu"
      aria-label="Columns"
    >
      <button
        type="button"
        className={styles.item}
        role="menuitem"
        disabled={target === null}
        onClick={() => {
          if (target) onAutoSize(target);
          onClose();
        }}
      >
        Auto-size this column
      </button>
      <button
        type="button"
        className={styles.item}
        role="menuitem"
        onClick={() => {
          onAutoSizeAll();
          onClose();
        }}
      >
        Auto-size all columns
      </button>
      <div className={styles.separator} role="separator" />
      <div className={styles.list}>
        {CATALOGUE.map((column) => (
          <button
            key={column.key}
            type="button"
            className={styles.item}
            role="menuitemcheckbox"
            aria-checked={shown.has(column.key)}
            onClick={() => onToggle(column.key)}
          >
            <span className={styles.tick} aria-hidden>
              {shown.has(column.key) ? "✓" : ""}
            </span>
            {column.label}
          </button>
        ))}
      </div>
    </div>
  );
}
