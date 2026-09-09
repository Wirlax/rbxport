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
import type { DeckId, RowDto, SortColumn, ViewSpec } from "@/ipc/types";
import { useTrackView, type PendingEdits, type Seed } from "@/store/useTrackView";
import { SEEDED_ROWS } from "@/lib/session";
import { formatBpm, formatDuration, formatShortDate } from "@/lib/format";
import { applyClick, emptySelection, modifierFor, type SelectionState } from "@/lib/selection";
import { ContextMenu } from "@/components/ContextMenu";
import { trackMenuFor } from "@/lib/contextMenus";
import { WaveformPreview } from "./WaveformPreview";
import styles from "./TrackTable.module.css";
import { SortDownIcon, SortUpIcon } from "@/components/icons";
import { Artwork } from "@/components/Artwork";
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

const Stars = memo(function Stars({
  rating, onRate,
}: {
  rating: number;
  onRate?: (stars: number) => void;
}) {
  if (onRate) {
    return (
      <span className={styles.stars} role="radiogroup" aria-label="Rating">
        {[1, 2, 3, 4, 5].map((star) => (
          <button
            key={star}
            type="button"
            className={styles.star}
            role="radio"
            aria-checked={rating === star}
            aria-label={`${star} of 5`}
            // Clicking the star already set clears the rating, which is how
            // rekordbox behaves and the only way to get back to none.
            onMouseDown={(e) => e.stopPropagation()}
            // Two quick clicks on a star are two ratings, not a request to
            // play the track.
            onDoubleClick={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              onRate(rating === star ? 0 : star);
            }}
          >
            {star <= rating ? "★" : "☆"}
          </button>
        ))}
      </span>
    );
  }
  return (
    <span className={styles.stars} aria-label={`${rating} of 5`}>
      {"★".repeat(rating)}
      {"☆".repeat(5 - rating)}
    </span>
  );
});

/**
 * A cell that turns into a text box on a double click.
 *
 * Committing on blur as well as Enter matters: clicking away is how people
 * leave a field, and losing the edit then is the behaviour everyone hates.
 * Escape abandons it, which is the escape hatch that makes committing on blur
 * safe.
 */
const EditableCell = memo(function EditableCell({
  value, label, onCommit,
}: {
  value: string;
  label: string;
  onCommit: (next: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);

  if (!editing) {
    return (
      <div
        className={styles.cell}
        data-col="comment"
        role="gridcell"
        onDoubleClick={(e) => {
          // Editing a comment is not asking to play the track: without this
          // the row's own double-click loads it into the player as well.
          e.stopPropagation();
          setDraft(value);
          setEditing(true);
        }}
        title={`${label} — double-click to edit`}
      >
        {value}
      </div>
    );
  }
  return (
    <div className={styles.cell} data-col="comment" role="gridcell">
      <input
        className={styles.editor}
        value={draft}
        aria-label={label}
        autoFocus
        onMouseDown={(e) => e.stopPropagation()}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => {
          setEditing(false);
          if (draft !== value) onCommit(draft);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.currentTarget.blur();
          } else if (e.key === "Escape") {
            // Abandon: set the draft back first so the blur commits nothing.
            setDraft(value);
            setEditing(false);
          }
          e.stopPropagation();
        }}
      />
    </div>
  );
});

const TrackRow = memo(function TrackRow({
  row, top, selected, onSelect, onOpen, onDragStart, onDragEnd, index, columns, onRate,
  onComment, onMenu,
}: {
  row: RowDto | undefined;
  top: number;
  selected: boolean;
  index: number;
  columns: readonly ColumnSpec[];
  onSelect: (index: number, id: string, e: React.MouseEvent) => void;
  /** Load the track into the player. A double-click, as in rekordbox. */
  onOpen: (index: number) => void;
  onDragStart: (row: RowDto) => void;
  /** Set the track's rating. Absent in a build that cannot write. */
  onRate: ((id: string, stars: number) => void) | undefined;
  /** Set the track's comment. */
  onComment: ((id: string, comment: string) => void) | undefined;
  onMenu: (index: number, row: RowDto, at: { x: number; y: number }) => void;
  onDragEnd: () => void;
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
      onDoubleClick={() => onOpen(index)}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenu(index, row, { x: e.clientX, y: e.clientY });
      }}
      draggable
      onDragStart={(e) => {
        onDragStart(row);
        e.dataTransfer.effectAllowed = "copy";
        // Firefox will not start a drag without payload.
        e.dataTransfer.setData("text/plain", row.id);
      }}
      // A drag that is let go over nothing still ends. Without this the tree
      // kept offering its playlists as targets afterwards.
      onDragEnd={() => onDragEnd()}
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
                <Artwork trackId={row.id} className={styles.artworkImage} lazy />
              ) : null}
            </div>
          );
        }
        if (col.key === "comment" && onComment) {
          return (
            <EditableCell
              key={col.key}
              value={row.comment}
              label="Comment"
              onCommit={(next) => onComment(row.id, next)}
            />
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
              <Stars rating={row.rating} onRate={(stars) => onRate?.(row.id, stars)} />
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

/**
 * A drag out of the track list.
 *
 * `ids` is what a playlist would take — the selection, or the one row grabbed
 * from outside it. `row` is the row under the hand, which is what a deck
 * takes: a deck holds one track, and dropping four onto it has no meaning.
 */
export interface TrackDrag {
  ids: readonly string[];
  row: RowDto;
}

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
  /**
   * The single selected row, or `null` when the selection is not one row.
   *
   * Selecting a track does not load it — arrowing down a playlist would load
   * every track on the way past — but an empty deck can be clicked to take
   * whatever is selected, and this is what it takes.
   */
  onSelectedRow?: (row: RowDto | null) => void;
  /** What is being dragged, so a drop target knows what it would get. */
  onDragTracks?: (drag: TrackDrag | null) => void;
  /** Edit a track's rating or comment. Absent where writes are impossible. */
  onRate?: (id: string, stars: number) => void;
  onComment?: (id: string, comment: string) => void;
  /** Bumped when the library changes, so cached pages are dropped. */
  libraryGeneration?: number;
  /** Edits shown before the backend has caught up. */
  pendingEdits?: PendingEdits;
  /** The rows behind the selection, for queueing analysis. */
  onSelectedTracks?: (tracks: { id: string; title: string }[]) => void;
  /** Analyse whatever is selected. */
  onAnalyse?: () => void;
  /** Right-click actions the table cannot do itself. */
  onShowInformation?: (row: RowDto) => void;
  onShowInFinder?: (row: RowDto) => void;
  onRemoveFromPlaylist?: (ids: readonly string[]) => void;
  /** rekordbox is running, so every write is refused rather than raced. */
  readOnly?: boolean;
  /**
   * How many players the layout is drawing, so the menu offers those and no
   * others: a player that is not on screen has nowhere to put a track.
   */
  players?: number;
  /** Load a track into a deck, from the menu. */
  onLoadTrack?: (deck: DeckId, row: RowDto) => void;
  /** The last run's rows, drawn until the backend answers. */
  seed?: Seed | undefined;
  /**
   * The first rows of the current view, once they exist.
   *
   * The app keeps them only to write the next start's opening screen — it
   * still never holds the library, just the handful of rows that were on it.
   */
  onFirstRows?: (rows: RowDto[], count: number) => void;
  /** Lets the keyboard shortcut put the caret here from anywhere. */
  searchRef?: React.RefObject<HTMLInputElement | null>;
}

export function TrackTable({
  spec, onSortChange, onSelectionChange, title, query, onQueryChange, searchRef,
  columns, onColumnMove, onColumnResize, onColumnToggle, onColumnAutoSize,
  onColumnAutoSizeAll, onFocusedRow, onDragTracks, onRate, onComment, seed, onFirstRows,
  libraryGeneration, pendingEdits, onSelectedTracks, onAnalyse,
  onShowInformation, onShowInFinder, onRemoveFromPlaylist, readOnly = false,
  players = 0, onLoadTrack, onSelectedRow,
}: TrackTableProps) {
  // Analysis is reachable from the keyboard rather than only a menu, since a
  // row context menu does not exist yet.
  useEffect(() => {
    if (!onAnalyse) return;
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.tagName === "INPUT" || target?.tagName === "TEXTAREA") return;
      // Plain letter, no modifier: the table has focus and a selection.
      if (event.key.toLowerCase() === "a" && !event.metaKey && !event.ctrlKey && !event.altKey) {
        onAnalyse();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [onAnalyse]);
  const view = useTrackView(spec, libraryGeneration, pendingEdits, seed);

  // Hand the top of the view up once it is real, for the next start's opening
  // screen. Only the first page, and only when it is filled.
  const reported = useRef("");
  useEffect(() => {
    if (!onFirstRows || view.loading || view.count === 0) return;
    const first = view.rowAt(0);
    if (!first) return;
    const stamp = `${view.token}:${view.count}:${first.id}`;
    if (reported.current === stamp) return;
    reported.current = stamp;
    const rows: RowDto[] = [];
    for (let i = 0; i < Math.min(view.count, SEEDED_ROWS); i++) {
      const row = view.rowAt(i);
      if (!row) break;
      rows.push(row);
    }
    onFirstRows(rows, view.count);
  }, [onFirstRows, view]);
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

  // The rows being drawn, as indices. These are what the fetch below keys on.
  //
  // It used to key on `items.length` — how *many* rows are on screen — which
  // does not change while scrolling. So moving the window asked for nothing,
  // and every row it landed on stayed a placeholder: a list at its full height
  // with a working scrollbar and no content in it.
  //
  // The page chain hid it. Each page that landed re-rendered the table and
  // re-ran the effect, which caught the window up by accident, so it only bit
  // when the scroll outran the fetches or moved after they had all settled.
  // Measured in the app: a drag ended on row 19,329 with the last request made
  // for rows 17,065-17,105, and nothing asked again for eight seconds.
  const firstIndex = items.length > 0 ? (items[0]?.index ?? 0) : -1;
  const lastIndex = items.length > 0 ? (items[items.length - 1]?.index ?? 0) : -1;

  // Ask for the pages covering what is on screen. Cheap and idempotent.
  useEffect(() => {
    if (firstIndex < 0) return;
    // The virtualizer's own window, overscan included, rather than the same
    // arithmetic done twice: it already accounts for `scrollMargin`, so this
    // cannot drift a header's worth out of step with what is rendered.
    view.ensureRange(firstIndex, lastIndex + 1);
  }, [firstIndex, lastIndex, view, view.count, view.token]);

  const startDraggingTracks = useCallback(
    (row: RowDto) => {
      // Whatever is selected, plus the row grabbed if it was not part of it —
      // dragging an unselected row should move that row, not the selection
      // somewhere else on screen.
      const ids = selection.ids.has(row.id) ? [...selection.ids] : [row.id];
      // The grabbed row travels with them, because a deck takes one track and
      // that is the one the hand is on. A playlist takes all of them.
      onDragTracks?.({ ids, row });
    },
    [selection.ids, onDragTracks],
  );

  const endDraggingTracks = useCallback(() => onDragTracks?.(null), [onDragTracks]);

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

  /**
   * Loads a row into the player.
   *
   * A double-click, not a click. Selecting a track and playing it are
   * different intentions — arrowing through a playlist to see what is in it
   * should not load forty tracks on the way past.
   */
  const handleOpen = useCallback(
    (index: number) => {
      onFocusedRow?.(view.rowAt(index) ?? null);
    },
    [view, onFocusedRow],
  );

  useEffect(() => {
    onSelectionChange?.(selection.ids.size);
  }, [selection.ids, onSelectionChange]);

  const reportedRow = useRef<string | null>(null);
  useEffect(() => {
    if (!onSelectedRow) return;
    // One row, or nothing: a deck holds one track, so a selection of four has
    // no answer to which of them clicking a deck would load.
    const only = selection.ids.size === 1 ? [...selection.ids][0] : undefined;
    if (only === reportedRow.current) return;
    reportedRow.current = only ?? null;
    if (only === undefined) {
      onSelectedRow(null);
      return;
    }
    for (let i = 0; i < view.count; i++) {
      const row = view.rowAt(i);
      if (row?.id === only) {
        onSelectedRow(row);
        return;
      }
    }
    // Selected but not fetched — off screen, which a click cannot reach.
    onSelectedRow(null);
  }, [selection.ids, view, view.token, onSelectedRow]);

  // The rows behind the selection, resolved from what is cached. A selection
  // spanning unfetched rows contributes only what is on hand, which is what
  // the user can see anyway.
  /** The track menu: where it is, and which row it was opened on. */
  const [trackMenu, setTrackMenu] = useState<
    { x: number; y: number; row: RowDto } | null
  >(null);

  const openTrackMenu = useCallback(
    (index: number, row: RowDto, at: { x: number; y: number }) => {
      // Right-clicking a row the selection does not hold selects it first, as
      // every list does: otherwise the menu acts on something else.
      setSelection((current) =>
        current.ids.has(row.id) ? current : { ids: new Set([row.id]), anchorIndex: index },
      );
      setTrackMenu({ ...at, row });
    },
    [],
  );

  const reportedSelection = useRef("");
  useEffect(() => {
    if (!onSelectedTracks) return;
    const tracks: { id: string; title: string }[] = [];
    for (let i = 0; i < view.count && tracks.length < selection.ids.size; i++) {
      const row = view.rowAt(i);
      if (row && selection.ids.has(row.id)) tracks.push({ id: row.id, title: row.title });
    }
    // Only when it has actually changed. This hands a new array upwards, and
    // the app holds it in state: sending an equal one re-renders the window,
    // which renders this table, which runs this effect again.
    const stamp = tracks.map((t) => t.id).join(",");
    if (stamp === reportedSelection.current) return;
    reportedSelection.current = stamp;
    onSelectedTracks(tracks);
  }, [selection.ids, view, onSelectedTracks]);

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
                onDragStart={startDraggingTracks}
                onDragEnd={endDraggingTracks}
                onRate={onRate}
                onComment={onComment}
                top={item.start - COL_HEADER_H}
                selected={row ? selection.ids.has(row.id) : false}
                onSelect={handleSelect}
                onOpen={handleOpen}
                onMenu={openTrackMenu}
              />
            );
          })}
        </div>
      </div>

      {trackMenu ? (
        <ContextMenu
          x={trackMenu.x}
          y={trackMenu.y}
          rows={trackMenuFor(players)}
          label="Track"
          context={{
            inPlaylist: spec.source.kind === "playlist",
            hasFile: true,
            readOnly,
          }}
          onChoose={(action) => {
            const ids = [...selection.ids];
            switch (action) {
              case "analyse":
                onAnalyse?.();
                break;
              case "showInformation":
                onShowInformation?.(trackMenu.row);
                break;
              case "showInFinder":
                onShowInFinder?.(trackMenu.row);
                break;
              case "removeFromPlaylist":
                onRemoveFromPlaylist?.(ids);
                break;
              case "loadPlayer1":
                onLoadTrack?.("a", trackMenu.row);
                break;
              case "loadPlayer2":
                onLoadTrack?.("b", trackMenu.row);
                break;
              default:
                break;
            }
          }}
          onClose={() => setTrackMenu(null)}
        />
      ) : null}

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
