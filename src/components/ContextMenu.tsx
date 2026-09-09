/**
 * A right-click menu, drawn as rekordbox draws its own.
 *
 * The rows come in as data — see `src/lib/contextMenus.ts`, which transcribes
 * rekordbox's lists — so this only decides where the menu goes, what closes
 * it, and how a greyed row differs from a live one.
 */
import { useEffect, useRef } from "react";

import { enabled, SEPARATOR, type MenuContext, type MenuRow } from "@/lib/contextMenus";
import styles from "./ContextMenu.module.css";

/** Kept off the window's edges, so a menu opened near one is still readable. */
const EDGE = 8;

export interface ContextMenuProps<A extends string> {
  x: number;
  y: number;
  rows: readonly MenuRow<A>[];
  context: MenuContext;
  /** Named for the screen reader, since a menu with no name is "menu". */
  label: string;
  onChoose: (action: A) => void;
  onClose: () => void;
}

export function ContextMenu<A extends string>({
  x, y, rows, context, label, onChoose, onClose,
}: ContextMenuProps<A>) {
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Anything outside, any scroll, any resize, or Escape closes it: a menu
    // that outlives the thing it was opened on is worse than no menu.
    const outside = (event: MouseEvent) => {
      if (!box.current?.contains(event.target as Node)) onClose();
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", outside, true);
    window.addEventListener("keydown", key);
    window.addEventListener("resize", onClose);
    window.addEventListener("scroll", onClose, true);
    return () => {
      window.removeEventListener("mousedown", outside, true);
      window.removeEventListener("keydown", key);
      window.removeEventListener("resize", onClose);
      window.removeEventListener("scroll", onClose, true);
    };
  }, [onClose]);

  // Placed after the first paint, from its own measured size: a menu that is
  // one row shorter than the last is a different height, and guessing it puts
  // the bottom of a long menu under the status bar.
  useEffect(() => {
    const element = box.current;
    if (!element) return;
    const { width, height } = element.getBoundingClientRect();
    const left = Math.min(x, Math.max(EDGE, window.innerWidth - width - EDGE));
    const top = Math.min(y, Math.max(EDGE, window.innerHeight - height - EDGE));
    element.style.left = `${Math.max(EDGE, left)}px`;
    element.style.top = `${Math.max(EDGE, top)}px`;
  }, [x, y, rows]);

  return (
    <div
      ref={box}
      className={styles.menu}
      style={{ left: x, top: y }}
      role="menu"
      aria-label={label}
    >
      {rows.map((row, index) =>
        row === SEPARATOR ? (
          // Its position is the only thing a separator has to be keyed by, and
          // the rows are a constant: they never reorder under it.
          <div key={`sep-${index}`} className={styles.separator} role="separator" />
        ) : (
          <button
            key={row.label}
            type="button"
            role="menuitem"
            className={styles.item}
            disabled={!enabled(row, context)}
            onClick={() => {
              if (row.action !== null) onChoose(row.action);
              onClose();
            }}
          >
            <span className={styles.label}>{row.label}</span>
            {row.submenu === true ? <span className={styles.arrow} aria-hidden /> : null}
          </button>
        ),
      )}
    </div>
  );
}
