/**
 * The browser's column layout, remembered between sessions.
 *
 * rekordbox persists per-context widths in `browseSetting.xml`; this is the
 * same idea in the one place a browser-only build can keep it. The rules
 * themselves are pure functions in `@/lib/columns` — this only holds the
 * state and writes it back.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  autoSizeAll,
  autoSizeColumn,
  defaultLayout,
  moveColumn,
  resizeColumn,
  resolve,
  sanitise,
  toggleColumn,
  type ColumnKey,
  type ColumnSpec,
  type Layout,
} from "@/lib/columns";

/**
 * Which table a layout belongs to.
 *
 * rekordbox keys these by context in `browseSetting.xml` — `TableHeader-<
 * Context>`, thirty of them — and by the *kind* of table rather than by each
 * individual playlist, so browsing a second playlist does not start from
 * scratch.
 */
export type ColumnContext = "collection" | "playlist" | "history";

const STORAGE_PREFIX = "rbl.columns.v2";

function keyFor(context: ColumnContext): string {
  return `${STORAGE_PREFIX}.${context}`;
}

function load(context: ColumnContext): Layout {
  try {
    const raw = localStorage.getItem(keyFor(context));
    // sanitise handles null, so a first run needs no special case.
    return sanitise(raw === null ? null : JSON.parse(raw));
  } catch {
    // Private browsing, a disabled store, or a half-written value: the table
    // must still render, so fall back rather than throw on the way up.
    return defaultLayout();
  }
}

export interface Columns {
  layout: Layout;
  /** Visible columns, in order, at their current widths. */
  columns: ColumnSpec[];
  toggle: (key: ColumnKey) => void;
  move: (key: ColumnKey, to: number) => void;
  resize: (key: ColumnKey, width: number) => void;
  autoSize: (key: ColumnKey) => void;
  autoSizeEvery: () => void;
  reset: () => void;
}

export function useColumns(context: ColumnContext): Columns {
  const [layout, setLayout] = useState<Layout>(() => load(context));

  // Switching context loads that table's own layout. Reading in an effect
  // rather than during render keeps the two stores from crossing over when
  // the context changes and a save is still pending.
  const loaded = useRef(context);
  useEffect(() => {
    if (loaded.current === context) return;
    loaded.current = context;
    setLayout(load(context));
  }, [context]);

  useEffect(() => {
    // Guard against writing the previous context's layout under the new key
    // in the render between the context changing and the load above.
    if (loaded.current !== context) return;
    try {
      localStorage.setItem(keyFor(context), JSON.stringify(layout));
    } catch {
      // Not being able to remember the layout is not a reason to break the
      // session that is running.
    }
  }, [layout, context]);

  const columns = useMemo(() => resolve(layout), [layout]);

  return {
    layout,
    columns,
    toggle: useCallback((key) => setLayout((l) => toggleColumn(l, key)), []),
    move: useCallback((key, to) => setLayout((l) => moveColumn(l, key, to)), []),
    resize: useCallback((key, width) => setLayout((l) => resizeColumn(l, key, width)), []),
    autoSize: useCallback((key) => setLayout((l) => autoSizeColumn(l, key)), []),
    autoSizeEvery: useCallback(() => setLayout(autoSizeAll), []),
    reset: useCallback(() => setLayout(defaultLayout()), []),
  };
}
