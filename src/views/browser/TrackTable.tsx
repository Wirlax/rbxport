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
import { FilterIcon, SortDownIcon, SortUpIcon } from "@/components/icons";
import { Artwork } from "@/components/Artwork";
import { RecordIcon } from "@/components/icons";
import type { ColumnKey, ColumnSpec } from "@/lib/columns";
import { browseScale, formatKey } from "@/lib/preferences";
import { trafficLightLit, type TrafficLightReach } from "@/lib/camelot";
import { TickIcon } from "@/components/icons";
import type { TrafficLightSource } from "@/lib/session";
import { usePreferences, useTooltip } from "@/store/usePreferences";
import type { KeyDisplay } from "@/ipc/types";
import { ColumnMenu } from "./ColumnMenu";
import { setRowDragImage } from "./dragGhost";

const ROW_H = 25; // --s-row-height
/** One frozen empty list, so a row without cues does not re-render for a new one. */
const NO_CUES: RowDto["hotCues"] = [];
/// --s-col-header-h. The column header sits inside the scroller so it moves
/// with the rows horizontally, which costs it this much of the vertical scroll.
const COL_HEADER_H = 24;
/// --s-preview-band-h: the strip the row's waveform and its cue badges share.
/// The canvas needs the number for its backing store; the cell's CSS places it.
const PREVIEW_BAND_H = 15;

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
    case "fileName": return row.fileName ?? "";
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
  value, label, onCommit, onClick, tip,
}: {
  value: string;
  label: string;
  onCommit: (next: string) => void;
  /**
   * Edit Library › Double-click to edit is off: a click on this cell of a
   * row that is already selected opens it, as in rekordbox. On, and only a
   * double click does.
   */
  onClick: boolean;
  tip: string | undefined;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);

  if (!editing) {
    const begin = () => {
      setDraft(value);
      setEditing(true);
    };
    return (
      <div
        className={styles.cell}
        data-col="comment"
        role="gridcell"
        onClick={onClick ? begin : undefined}
        onDoubleClick={(e) => {
          // Editing a comment is not asking to play the track: without this
          // the row's own double-click loads it into the player as well.
          e.stopPropagation();
          if (!onClick) begin();
        }}
        title={tip}
      >
        {value}
      </div>
    );
  }
  return (
    <div
      className={styles.cell}
      data-col="comment"
      role="gridcell"
      // A second click on a cell that has just opened is not a request to
      // play the track either.
      onDoubleClick={(e) => e.stopPropagation()}
    >
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
  onComment, onMenu, keyDisplay, previewCues, clickToEdit, tooltips, trafficKey, trafficReach,
}: {
  row: RowDto | undefined;
  top: number;
  selected: boolean;
  index: number;
  columns: readonly ColumnSpec[];
  /** Preferences: `Ebm` or `2A`. */
  keyDisplay: KeyDisplay;
  /** Preferences: the hot cue badges over the row's waveform. */
  previewCues: boolean;
  /** Preferences: a comment opens on a click rather than a double click. */
  clickToEdit: boolean;
  tooltips: boolean;
  /** The Traffic Light: the key rows light against, and how far around it. Null lights nothing. */
  trafficKey: string | null;
  trafficReach: TrafficLightReach;
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
        // A faded copy of the row travels with the hand, every time: the
        // browser's own snapshot of a virtualised row does not — see
        // `dragGhost.ts`.
        setRowDragImage(e.currentTarget, e.dataTransfer, { x: e.clientX, y: e.clientY });
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
              {row.analysed ? (
                <span className={styles.analysed} title={tooltips ? "Analyzed" : undefined} />
              ) : null}
              <span className={styles.cue}>{row.hotCues.length > 0 ? "CUE" : ""}</span>
            </div>
          );
        }
        if (col.key === "artwork") {
          return (
            <div key={col.key} className={styles.artwork} data-col={col.key} role="gridcell">
              {/*
                The record sits underneath as the fallback, the way rekordbox
                draws a cell without artwork: a little under half the
                reference library has none, and it also covers the gap while
                the image decodes. `loading="lazy"` keeps a fast scroll from
                queueing a fetch for every row it passes.
              */}
              <RecordIcon className={styles.record} aria-hidden />
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
              // A click edits only a selected row's comment; on any other
              // row the click is a selection, as it has to be.
              onClick={clickToEdit && selected}
              tip={
                tooltips
                  ? clickToEdit
                    ? "Comment — click to edit"
                    : "Comment — double-click to edit"
                  : undefined
              }
            />
          );
        }
        if (col.key === "preview") {
          return (
            <div key={col.key} className={styles.preview} data-col={col.key} role="gridcell">
              {row.analysed ? (
                <WaveformPreview
                  trackId={row.id}
                  width={col.width - 6}
                  height={PREVIEW_BAND_H}
                  hotCues={previewCues ? row.hotCues : NO_CUES}
                  durationSec={row.durationSec}
                />
              ) : null}
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
        if (col.key === "key") {
          // The Traffic Light: a key that goes with the loaded track's is lit.
          const lit = trafficKey !== null && trafficLightLit(row.key, trafficKey, trafficReach);
          return (
            <div
              key={col.key}
              className={styles.cell}
              data-col={col.key}
              data-lit={lit || undefined}
              role="gridcell"
            >
              {formatKey(row.key, keyDisplay)}
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
  /**
   * The Track Filter, opt-in: the header's filter button and the bar it drops
   * down. The main browser passes these; the sub-browser has neither.
   */
  filterOpen?: boolean;
  onToggleFilter?: () => void;
  /** The bar itself, drawn between the header and the column header. */
  filterBar?: React.ReactNode;
  /**
   * The Traffic Light: which deck the browser reads, the MASTER menu above
   * the list, and the key of the track on it. The main browser passes these;
   * the sub-browser has no menu and lights nothing.
   */
  trafficLight?: TrafficLightSource;
  onTrafficLight?: (source: TrafficLightSource) => void;
  trafficKey?: string | null;
}

/** The MASTER menu's rows, in rekordbox's wording [OBS]. */
const TRAFFIC_SOURCES: readonly { id: TrafficLightSource; label: string; short: string }[] = [
  { id: "master", label: "MASTER DECK - Traffic Light", short: "MASTER" },
  { id: "a", label: "PLAYER A - Traffic Light", short: "PLAYER A" },
  { id: "b", label: "PLAYER B - Traffic Light", short: "PLAYER B" },
];

export function TrackTable({
  spec, onSortChange, onSelectionChange, title, query, onQueryChange, searchRef,
  columns, onColumnMove, onColumnResize, onColumnToggle, onColumnAutoSize,
  onColumnAutoSizeAll, onFocusedRow, onDragTracks, onRate, onComment, seed, onFirstRows,
  libraryGeneration, pendingEdits, onSelectedTracks, onAnalyse,
  onShowInformation, onShowInFinder, onRemoveFromPlaylist, readOnly = false,
  players = 0, onLoadTrack, onSelectedRow, filterOpen = false, onToggleFilter, filterBar,
  trafficLight, onTrafficLight, trafficKey = null,
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
  const preferences = usePreferences();
  const { keyDisplay, previewCueMarkers, tooltips } = preferences.view;
  const tip = useTooltip();
  // The MASTER menu, open or not. Closed by anything outside it, as every
  // other menu here is.
  const [trafficMenu, setTrafficMenu] = useState(false);
  const trafficBox = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!trafficMenu) return;
    const onDown = (event: MouseEvent) => {
      if (!trafficBox.current?.contains(event.target as Node)) setTrafficMenu(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setTrafficMenu(false);
    };
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [trafficMenu]);
  const clickToEdit = !preferences.advanced.doubleClickToEdit;
  // Browse › FontSize and Line Space scale the measured tokens; the
  // virtualizer has to be told the same height the CSS draws.
  const fontScale = browseScale(preferences.view.browseFontSize);
  const rowH = Math.round(ROW_H * browseScale(preferences.view.browseLineSpace));

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
    estimateSize: () => rowH,
    overscan: 8,
    // The sticky column header is in the scroller's flow, so the list starts
    // this far down it. Without this the virtualizer's idea of which rows are
    // visible is a header's worth out.
    scrollMargin: COL_HEADER_H,
  });

  // A new row height throws away what the virtualizer measured at the old one.
  useEffect(() => {
    virtualizer.measure();
  }, [virtualizer, rowH]);

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
        // Browse › FontSize, Bold and Line Space, scoped to the list: the
        // tokens are the measured sizes, and these are the slider's multiples
        // of them.
        ["--s-row-height" as string]: `${rowH}px`,
        ["--f-size-ui" as string]: `calc(${fontScale} * var(--f-size-ui-base))`,
        ["--browse-weight" as string]: preferences.view.browseBold ? 700 : 400,
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
        {trafficLight && onTrafficLight ? (
          <div ref={trafficBox} className={styles.trafficBox}>
            <button
              type="button"
              className={styles.trafficMaster}
              aria-label="Traffic Light deck"
              aria-haspopup="menu"
              aria-expanded={trafficMenu}
              // Transcribed from rekordbox: "Select the target deck for
              // Traffic Light feature."
              title={tip("Select the target deck for Traffic Light feature.")}
              onClick={() => setTrafficMenu((open) => !open)}
            >
              {TRAFFIC_SOURCES.find((s) => s.id === trafficLight)?.short ?? "MASTER"}
            </button>
            <button
              type="button"
              className={styles.trafficChevron}
              aria-label="Choose the Traffic Light deck"
              aria-haspopup="menu"
              aria-expanded={trafficMenu}
              onClick={() => setTrafficMenu((open) => !open)}
            >
              <span aria-hidden />
            </button>
            {trafficMenu ? (
              <div className={styles.trafficMenu} role="menu" aria-label="Traffic Light deck">
                {TRAFFIC_SOURCES.map((source) => (
                  <button
                    key={source.id}
                    type="button"
                    role="menuitemradio"
                    aria-checked={source.id === trafficLight}
                    className={styles.trafficItem}
                    onClick={() => {
                      onTrafficLight(source.id);
                      setTrafficMenu(false);
                    }}
                  >
                    <span className={styles.trafficTick} aria-hidden>
                      {source.id === trafficLight ? <TickIcon className={styles.trafficTickGlyph} /> : null}
                    </span>
                    {source.label}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
        ) : null}
        {onToggleFilter ? (
          <button
            type="button"
            className={styles.filterToggle}
            data-on={filterOpen || undefined}
            aria-pressed={filterOpen}
            aria-label="Display/Hide Track Filter"
            title={tip("Display/Hide Track Filter")}
            data-testid="filter-toggle"
            onClick={onToggleFilter}
          >
            <FilterIcon className={styles.filterGlyph} />
          </button>
        ) : null}
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

      {filterOpen ? filterBar : null}

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
                keyDisplay={keyDisplay}
                previewCues={previewCueMarkers}
                clickToEdit={clickToEdit}
                tooltips={tooltips}
                trafficKey={trafficKey}
                trafficReach={preferences.view.trafficLight}
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
