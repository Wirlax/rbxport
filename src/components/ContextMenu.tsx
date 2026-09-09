/**
 * A right-click menu, drawn as rekordbox draws its own.
 *
 * The rows come in as data — see `src/lib/contextMenus.ts`, which transcribes
 * rekordbox's lists — so this only decides where the menu goes, what closes
 * it, and how a greyed row differs from a live one.
 */
import { useEffect, useRef, useState } from "react";

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
  /** Which entry's submenu is open, by label. One at a time, as menus are. */
  const [open, setOpen] = useState<string | null>(null);

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
          <div
            key={row.label}
            className={styles.row}
            // Hover opens it and moving to another entry closes it, which is
            // what a menu does; the click is for a pointer that arrives
            // without hovering, and for the keyboard.
            onMouseEnter={() => setOpen(row.items ? row.label : null)}
          >
            <button
              type="button"
              role="menuitem"
              className={styles.item}
              disabled={!enabled(row, context)}
              aria-haspopup={row.items ? "menu" : undefined}
              aria-expanded={row.items ? open === row.label : undefined}
              onClick={() => {
                if (row.items) {
                  setOpen((was) => (was === row.label ? null : row.label));
                  return;
                }
                if (row.action !== null) onChoose(row.action);
                onClose();
              }}
            >
              <span className={styles.label}>{row.label}</span>
              {row.submenu === true || row.items ? (
                <span className={styles.arrow} aria-hidden />
              ) : null}
            </button>
            {row.items && open === row.label ? (
              <div className={styles.submenu} role="menu" aria-label={row.label}>
                {row.items.map((child, at) =>
                  child === SEPARATOR ? (
                    <div key={`sub-${at}`} className={styles.separator} role="separator" />
                  ) : (
                    <button
                      key={child.label}
                      type="button"
                      role="menuitem"
                      className={styles.item}
                      disabled={!enabled(child, context)}
                      onClick={() => {
                        if (child.action !== null) onChoose(child.action);
                        onClose();
                      }}
                    >
                      <span className={styles.label}>{child.label}</span>
                    </button>
                  ),
                )}
              </div>
            ) : null}
          </div>
        ),
      )}
    </div>
  );
}
