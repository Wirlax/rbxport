/**
 * The browser's column layout, remembered between sessions.
 *
 * rekordbox persists per-context widths in `browseSetting.xml`; this is the
 * same idea in the one place a browser-only build can keep it. The rules
 * themselves are pure functions in `@/lib/columns` — this only holds the
 * state and writes it back.
 */
import { useCallback, useEffect, useMemo, useState } from "react";

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

const STORAGE_KEY = "rbl.columns.v1";

function load(): Layout {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
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

export function useColumns(): Columns {
  const [layout, setLayout] = useState<Layout>(load);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(layout));
    } catch {
      // Not being able to remember the layout is not a reason to break the
      // session that is running.
    }
  }, [layout]);

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
