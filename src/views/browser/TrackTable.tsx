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

const ROW_H = 25; // --s-row-height

export interface Column {
  key: SortColumn | "preview" | "artwork" | "attr" | "comment";
  label: string;
  width: number;
  align?: "right";
  sortable: boolean;
}

/** Widths are rekordbox's own, from TableHeader-PlaylistTracks. */
export const PLAYLIST_COLUMNS: Column[] = [
  { key: "attr", label: "", width: 67, sortable: false },
  { key: "trackNo", label: "#", width: 57, align: "right", sortable: true },
  { key: "preview", label: "Preview", width: 128, sortable: false },
  { key: "artwork", label: "Artwork", width: 80, sortable: false },
  { key: "title", label: "Track Title", width: 387, sortable: true },
  { key: "key", label: "Key", width: 73, sortable: true },
  { key: "bpm", label: "BPM", width: 80, align: "right", sortable: true },
  { key: "duration", label: "Time", width: 80, align: "right", sortable: true },
  { key: "rating", label: "Rating", width: 101, sortable: true },
  { key: "artist", label: "Artist", width: 301, sortable: true },
  { key: "comment", label: "Comments", width: 210, sortable: false },
  { key: "label", label: "Label", width: 128, sortable: true },
  { key: "dateAdded", label: "Date Added", width: 128, align: "right", sortable: true },
  { key: "releaseDate", label: "Release Date", width: 128, align: "right", sortable: true },
];

const GRID = PLAYLIST_COLUMNS.map((c) => `${c.width}px`).join(" ");
const TOTAL_WIDTH = PLAYLIST_COLUMNS.reduce((a, c) => a + c.width, 0);

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
  row, top, selected, onSelect, index,
}: {
  row: RowDto | undefined;
  top: number;
  selected: boolean;
  index: number;
  onSelect: (index: number, id: string, e: React.MouseEvent) => void;
}) {
  if (!row) {
    // Placeholder keeps the row box the exact height so nothing shifts on arrival.
    return <div className={styles.row} style={{ transform: `translateY(${top}px)` }} aria-hidden />;
  }
  return (
    <div
      className={styles.row}
      data-selected={selected || undefined}
      data-even={index % 2 === 1 || undefined}
      style={{ transform: `translateY(${top}px)` }}
      onMouseDown={(e) => onSelect(index, row.id, e)}
      role="row"
      aria-selected={selected}
    >
      {PLAYLIST_COLUMNS.map((col) => {
        if (col.key === "attr") {
          return (
            <div key={col.key} className={styles.attr} role="gridcell">
              {row.analysed ? <span className={styles.analysed} title="Analyzed" /> : null}
              <span className={styles.cue}>{row.cues ? "CUE" : ""}</span>
            </div>
          );
        }
        if (col.key === "artwork") {
          return (
            <div key={col.key} className={styles.artwork} role="gridcell">
              <span style={{ background: `linear-gradient(135deg, hsl(${row.artworkHue} 40% 26%), hsl(${(row.artworkHue + 70) % 360} 55% 12%))` }} />
            </div>
          );
        }
        if (col.key === "preview") {
          return (
            <div key={col.key} className={styles.preview} role="gridcell">
              {row.analysed ? <WaveformPreview trackId={row.id} width={col.width - 6} height={19} /> : null}
            </div>
          );
        }
        if (col.key === "rating") {
          return (
            <div key={col.key} className={styles.cell} role="gridcell">
              <Stars rating={row.rating} />
            </div>
          );
        }
        return (
          <div
            key={col.key}
            className={col.align === "right" ? `${styles.cell} ${styles.right}` : styles.cell}
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
  /** Lets the keyboard shortcut put the caret here from anywhere. */
  searchRef?: React.RefObject<HTMLInputElement | null>;
}

export function TrackTable({
  spec, onSortChange, onSelectionChange, title, query, onQueryChange, searchRef,
}: TrackTableProps) {
  const view = useTrackView(spec);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [selection, setSelection] = useState<SelectionState>(emptySelection);

  const virtualizer = useVirtualizer({
    count: view.count,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_H,
    overscan: 8,
  });

  const items = virtualizer.getVirtualItems();

  // Ask for the pages covering what is on screen. Cheap and idempotent.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const w = visibleWindow(el.scrollTop, el.clientHeight, ROW_H, view.count);
    view.ensureRange(w.start, w.end);
  }, [items.length, view, view.count, view.token]);

  const handleSelect = useCallback(
    (index: number, id: string, e: React.MouseEvent) => {
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
    [selection.anchorIndex, view],
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
      PLAYLIST_COLUMNS.map((col) => (
        <div
          key={col.key}
          className={col.align === "right" ? `${styles.headCell} ${styles.right}` : styles.headCell}
          data-sorted={col.key === spec.sort || undefined}
          onClick={col.sortable ? () => onSortChange(col.key as SortColumn) : undefined}
          role="columnheader"
        >
          {col.label}
          {sortIndicator(col)}
        </div>
      )),
    [spec.sort, onSortChange, sortIndicator],
  );

  return (
    <div className={styles.browser} style={{ ["--cols" as string]: GRID, ["--table-w" as string]: `${TOTAL_WIDTH}px` }}>
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

      <div className={styles.colHead} role="row">
        {header}
      </div>

      <div className={styles.scroll} ref={scrollRef} data-testid="track-scroll" role="grid" aria-rowcount={view.count}>
        <div className={styles.inner} style={{ height: `${virtualizer.getTotalSize()}px` }}>
          {items.map((item) => {
            const row = view.rowAt(item.index);
            return (
              <TrackRow
                key={row?.id ?? `slot-${item.key}`}
                row={row}
                index={item.index}
                top={item.start}
                selected={row ? selection.ids.has(row.id) : false}
                onSelect={handleSelect}
              />
            );
          })}
        </div>
      </div>
    </div>
  );
}
