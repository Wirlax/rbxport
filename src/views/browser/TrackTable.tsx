/**
 * The track browser.
 *
 * Column model and widths come from rekordbox's own `TableHeader-<Context>`
 * layout (design/measure/column-map.json); geometry comes from the measured
 * tokens. Rows are virtualized and keyed by row id so a re-sort moves DOM nodes
 * instead of rewriting every cell.
 */
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { RowDto, SortColumn, ViewSpec } from "@/ipc/types";
import { useTrackView } from "@/store/useTrackView";
import { formatBpm, formatDuration, formatShortDate } from "@/lib/format";
import { applyClick, emptySelection, modifierFor, type SelectionState } from "@/lib/selection";
import { visibleWindow } from "@/lib/virtual";
import { WaveformPreview } from "./WaveformPreview";
import styles from "./TrackTable.module.css";
import { SortDownIcon, SortUpIcon } from "@/components/icons";
import { artworkUrl } from "@/ipc/artwork";
import type { ColumnKey, ColumnSpec } from "@/lib/columns";
import { ColumnMenu } from "./ColumnMenu";

const ROW_H = 25; // --s-row-height
/// --s-col-header-h. The column header sits inside the scroller so it moves
/// with the rows horizontally, which costs it this much of the vertical scroll.
const COL_HEADER_H = 23;

export type Column = ColumnSpec;

/**
 * The grid track list, with a trailing `1fr` that absorbs whatever the window
 * has spare so the table fills the width instead of stopping at the sum of its
 * columns. Below that sum the row's `min-width` takes over and the scroller
 * does its job.
 */
function gridOf(columns: readonly ColumnSpec[]): string {
  return `${columns.map((c) => `${c.width}px`).join(" ")} 1fr`;
}

function totalWidthOf(columns: readonly ColumnSpec[]): number {
  return columns.reduce((a, c) => a + c.width, 0);
}

/** Which heading sits under an x position, by hit-testing the header row. */
function columnAt(head: HTMLElement | null, x: number): number | null {
  if (!head) return null;
  const cells = [...head.children];
  for (const [at, cell] of cells.entries()) {
    const box = cell.getBoundingClientRect();
    if (x >= box.left && x <= box.right) return at;
  }
  // Past the last heading: the far right.
  return cells.length > 0 ? cells.length - 1 : null;
}

function cellText(row: RowDto, key: Column["key"]): string {
  switch (key) {
    case "trackNo": return String(row.trackNo);
    case "title": return row.title;
    case "artist": return row.artist;
    case "album": return row.album;
    case "genre": return row.genre;
    case "label": return row.label;
    case "comment": return row.comment;
    case "key": return row.key;
    case "bpm": return formatBpm(row.bpmX100);
    case "duration": return formatDuration(row.durationSec);
    case "dateAdded": return formatShortDate(row.dateAdded);
    case "releaseDate": return formatShortDate(row.releaseDate);
    case "rating": return "";
    default: return "";
  }
}

const Stars = memo(function Stars({ rating }: { rating: number }) {
  return (
    <span className={styles.stars} aria-label={`${rating} of 5`}>
      {"★".repeat(rating)}
      {"☆".repeat(5 - rating)}
    </span>
  );
});

const TrackRow = memo(function TrackRow({
  row, top, selected, onSelect, index, columns,
}: {
  row: RowDto | undefined;
  top: number;
  selected: boolean;
  index: number;
  columns: readonly ColumnSpec[];
  onSelect: (index: number, id: string, e: React.MouseEvent) => void;
}) {
  if (!row) {
    // Placeholder keeps the row box the exact height so nothing shifts on arrival.
    return <div className={styles.row} style={{ transform: `translate3d(0, ${top}px, 0)` }} aria-hidden />;
  }
  return (
    <div
      className={styles.row}
      data-selected={selected || undefined}
      data-even={index % 2 === 1 || undefined}
      style={{ transform: `translate3d(0, ${top}px, 0)` }}
      onMouseDown={(e) => onSelect(index, row.id, e)}
      role="row"
      aria-selected={selected}
    >
      {columns.map((col) => {
        if (col.key === "attr") {
          return (
            <div key={col.key} className={styles.attr} data-col={col.key} role="gridcell">
              {row.analysed ? <span className={styles.analysed} title="Analyzed" /> : null}
              <span className={styles.cue}>{row.cues ? "CUE" : ""}</span>
            </div>
          );
        }
        if (col.key === "artwork") {
          return (
            <div key={col.key} className={styles.artwork} data-col={col.key} role="gridcell">
              {/*
                The tint sits underneath as the fallback: a little under half
                the reference library has no artwork, and it also covers the
                gap while the image decodes. `loading="lazy"` keeps a fast
                scroll from queueing a fetch for every row it passes.
              */}
              <span
                style={{
                  background: `linear-gradient(135deg, hsl(${row.artworkHue} 40% 26%), hsl(${(row.artworkHue + 70) % 360} 55% 12%))`,
                }}
              />
              {row.hasArtwork ? (
                <img
                  className={styles.artworkImage}
                  src={artworkUrl(row.id)}
                  alt=""
                  loading="lazy"
                  decoding="async"
                  draggable={false}
                />
              ) : null}
            </div>
          );
        }
        if (col.key === "preview") {
          return (
            <div key={col.key} className={styles.preview} data-col={col.key} role="gridcell">
              {row.analysed ? <WaveformPreview trackId={row.id} width={col.width - 6} height={19} /> : null}
            </div>
          );
        }
        if (col.key === "rating") {
          return (
            <div key={col.key} className={styles.cell} data-col={col.key} role="gridcell">
              <Stars rating={row.rating} />
            </div>
          );
        }
        return (
          <div
            key={col.key}
            className={col.align === "right" ? `${styles.cell} ${styles.right}` : styles.cell}
            data-col={col.key}
            role="gridcell"
          >
            {cellText(row, col.key)}
          </div>
        );
      })}
    </div>
  );
});

export interface TrackTableProps {
  spec: ViewSpec;
  onSortChange: (column: SortColumn) => void;
  onSelectionChange?: (count: number) => void;
  title: string;
  /** The live search text. Rust does the filtering; this is only the box. */
  query: string;
  onQueryChange: (query: string) => void;
  /** Visible columns, in order, at their current widths. */
  columns: readonly ColumnSpec[];
  onColumnMove: (key: ColumnKey, to: number) => void;
  onColumnResize: (key: ColumnKey, width: number) => void;
  onColumnToggle: (key: ColumnKey) => void;
  onColumnAutoSize: (key: ColumnKey) => void;
  onColumnAutoSizeAll: () => void;
  /** The row the player should show, as the selection moves. */
  onFocusedRow?: (row: RowDto | null) => void;
  /** Lets the keyboard shortcut put the caret here from anywhere. */
  searchRef?: React.RefObject<HTMLInputElement | null>;
}

export function TrackTable({
  spec, onSortChange, onSelectionChange, title, query, onQueryChange, searchRef,
  columns, onColumnMove, onColumnResize, onColumnToggle, onColumnAutoSize,
  onColumnAutoSizeAll, onFocusedRow,
}: TrackTableProps) {
  const view = useTrackView(spec);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [selection, setSelection] = useState<SelectionState>(emptySelection);
  const [dragKey, setDragKey] = useState<ColumnKey | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; target: ColumnKey } | null>(null);
  // Live during a header-edge drag. A ref, not state: this updates per
  // mousemove and re-rendering the table on each would be a frame's work.
  const resizing = useRef<{ key: ColumnKey; x: number; width: number } | null>(null);
  // A heading drag in progress, and whether it passed the threshold.
  const reorder = useRef<{ key: ColumnKey; from: number; x: number; moved: boolean } | null>(null);
  // Set when a drag finishes, so the click that follows does not also sort.
  const draggedRef = useRef(false);
  const headRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      const drag = resizing.current;
      if (drag) {
        onColumnResize(drag.key, drag.width + (e.clientX - drag.x));
        return;
      }
      const move = reorder.current;
      if (!move) return;
      // A few pixels of slop, so a slightly imprecise click still sorts.
      if (!move.moved && Math.abs(e.clientX - move.x) < 5) return;
      move.moved = true;
      setDragKey(move.key);
    };
    const onUp = (e: MouseEvent) => {
      resizing.current = null;
      const move = reorder.current;
      reorder.current = null;
      setDragKey(null);
      if (!move?.moved) return;
      draggedRef.current = true;
      const to = columnAt(headRef.current, e.clientX);
      if (to !== null && to !== move.from) onColumnMove(move.key, to);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [onColumnResize, onColumnMove]);

  const virtualizer = useVirtualizer({
    count: view.count,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_H,
    overscan: 8,
    // The sticky column header is in the scroller's flow, so the list starts
    // this far down it. Without this the virtualizer's idea of which rows are
    // visible is a header's worth out.
    scrollMargin: COL_HEADER_H,
  });

  const items = virtualizer.getVirtualItems();

  // Ask for the pages covering what is on screen. Cheap and idempotent.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    // scrollTop counts the sticky header; the list's own offset does not.
    const listTop = Math.max(0, el.scrollTop - COL_HEADER_H);
    const w = visibleWindow(listTop, el.clientHeight, ROW_H, view.count);
    view.ensureRange(w.start, w.end);
  }, [items.length, view, view.count, view.token]);

  const handleSelect = useCallback(
    (index: number, id: string, e: React.MouseEvent) => {
      // The player follows the row just clicked, whatever the modifier does to
      // the rest of the selection.
      onFocusedRow?.(view.rowAt(index) ?? null);
      const modifier = modifierFor(e);
      if (modifier === "range" && selection.anchorIndex !== null) {
        const anchor = selection.anchorIndex;
        void view.idsInRange(anchor, index).then((ids) => {
          setSelection((s) => applyClick(s, { id, index }, "range", ids));
        });
        return;
      }
      setSelection((s) => applyClick(s, { id, index }, modifier));
    },
    [selection.anchorIndex, view, onFocusedRow],
  );

  useEffect(() => {
    onSelectionChange?.(selection.ids.size);
  }, [selection.ids, onSelectionChange]);

  // An arrow drawn to rekordbox's geometry rather than the text arrows that
  // stood in for it: those render in the body font and sit off the baseline.
  const sortIndicator = useCallback(
    (col: Column) =>
      col.sortable && col.key === spec.sort ? (
        spec.descending ? (
          <SortDownIcon className={styles.sortArrow} />
        ) : (
          <SortUpIcon className={styles.sortArrow} />
        )
      ) : null,
    [spec.sort, spec.descending],
  );

  const header = useMemo(
    () =>
      columns.map((col, at) => (
        <div
          key={col.key}
          className={col.align === "right" ? `${styles.headCell} ${styles.right}` : styles.headCell}
          data-sorted={col.key === spec.sort || undefined}
          data-dragging={dragKey === col.key || undefined}
          onMouseDown={(e) => {
            // Reordering is a pointer drag with a threshold, not HTML5
            // drag-and-drop: marking the heading `draggable` makes the browser
            // treat a plain click as the start of a drag and swallow it, which
            // stopped the heading sorting at all.
            if (e.button !== 0) return;
            reorder.current = { key: col.key, from: at, x: e.clientX, moved: false };
          }}
          onClick={
            col.sortable
              ? () => {
                  // A drag ends in a click too; that one must not also sort.
                  if (draggedRef.current) {
                    draggedRef.current = false;
                    return;
                  }
                  onSortChange(col.key as SortColumn);
                }
              : undefined
          }
          onContextMenu={(e) => {
            e.preventDefault();
            setMenu({ x: e.clientX, y: e.clientY, target: col.key });
          }}
          role="columnheader"
        >
          {col.label}
          {sortIndicator(col)}
          {/*
            The resize grip. Its own mousedown stops the header's click, so
            dragging an edge never also re-sorts the table.
          */}
          <span
            className={styles.grip}
            onMouseDown={(e) => {
              e.stopPropagation();
              e.preventDefault();
              resizing.current = { key: col.key, x: e.clientX, width: col.width };
            }}
            onClick={(e) => e.stopPropagation()}
            role="separator"
            aria-orientation="vertical"
            aria-label={`Resize ${col.label}`}
          />
        </div>
      )),
    [columns, spec.sort, onSortChange, sortIndicator, dragKey],
  );

  return (
    <div
      className={styles.browser}
      style={{
        ["--cols" as string]: gridOf(columns),
        ["--table-w" as string]: `${totalWidthOf(columns)}px`,
      }}
    >
      <div className={styles.browserHead}>
        <span className={styles.title} data-testid="browser-title">
          {/*
            The count belongs to the list, so it appears only once the list
            has settled. While a view is opening, `view.count` is still the
            previous view's, and pairing it with the new title showed a
            playlist's name beside the whole collection's count.
          */}
          {view.loading ? title : `${title} (${view.count} Tracks)`}
        </span>
        <div className={styles.search} role="search">
          <span className={styles.searchIcon} aria-hidden />
          <input
            ref={searchRef}
            className={styles.searchInput}
            type="search"
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            placeholder="Search within this track list"
            aria-label="Search within this track list"
            // The browser's own clear button and history dropdown do not
            // belong in an application window.
            autoComplete="off"
            spellCheck={false}
          />
        </div>
      </div>

      <div className={styles.scroll} ref={scrollRef} data-testid="track-scroll" role="grid" aria-rowcount={view.count}>
        {/*
          Inside the scroller, not above it. As a sibling it stayed put while
          the rows moved sideways, so every column sheared away from its own
          heading; sticky keeps it pinned vertically while it scrolls
          horizontally with them.
        */}
        <div className={styles.colHead} role="row" ref={headRef}>
          {header}
        </div>

        <div className={styles.inner} style={{ height: `${virtualizer.getTotalSize()}px` }}>
          {items.map((item) => {
            const row = view.rowAt(item.index);
            return (
              <TrackRow
                key={row?.id ?? `slot-${item.key}`}
                row={row}
                index={item.index}
                columns={columns}
                top={item.start - COL_HEADER_H}
                selected={row ? selection.ids.has(row.id) : false}
                onSelect={handleSelect}
              />
            );
          })}
        </div>
      </div>

      {menu ? (
        <ColumnMenu
          x={menu.x}
          y={menu.y}
          target={menu.target}
          visible={columns.map((c) => c.key)}
          onToggle={onColumnToggle}
          onAutoSize={onColumnAutoSize}
          onAutoSizeAll={onColumnAutoSizeAll}
          onClose={() => setMenu(null)}
        />
      ) : null}
    </div>
  );
}
