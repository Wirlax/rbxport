/**
 * The preview player.
 *
 * Every size here is a token measured off a 2x capture of rekordbox 7.2.11
 * running: the player spans y 56..335pt, its transport column is 80pt wide and
 * its memory panel 209pt, and the five bands inside it (title, overview,
 * phrase, detail, pads) have their measured heights recorded as token sources.
 *
 * Playback is the Rust engine behind the `deck_*` commands — see
 * `usePlayback` and `crates/rbl-deck`. Outside Tauri there is no engine, so
 * the transport is drawn and disabled: a player waiting for a backend, rather
 * than an unfinished panel. Controls with nothing behind them yet are drawn
 * the same way, for the same reason.
 */
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import type { Cue, DeckId, Phrase, RowDto } from "@/ipc/types";
import { getBackend } from "@/ipc/client";
import { useElementSize } from "@/store/useElementSize";
import { Artwork } from "@/components/Artwork";
import { CutIcon, DiscIcon, EjectIcon, LockIcon, MetronomeIcon } from "@/components/icons";
import { formatBpm } from "@/lib/format";
import {
  DETAIL_BARS,
  NO_BEATS,
  ZOOM_STEPS,
  showsEveryBeat,
  beatsIn,
  cuesFor,
  detailSpan,
  dragSeconds,
  JUMP_SIZE_ID,
  jumpSizeById,
  jumpStepSeconds,
  zoomBy,
  needsRedraw,
  OVERDRAW,
  scrollOffset,
  pressCue,
  releaseCue,
  headPercent,
  parseBeatGrid,
  phraseSpans,
  memoryTime,
  splitTime,
  windowAround,
  type BeatGrid as BeatGridData,
  type CuePanel,
  type PadMode,
} from "@/lib/player";
import { usePlayback } from "@/store/usePlayback";
import { syncTo, type Deck as SyncDeck } from "@/lib/sync";
import { actionFor, detectPlatform, dispatch } from "@/lib/shortcuts";
import { WaveformDetail } from "./WaveformDetail";
import { SimplePlayer } from "./SimplePlayer";
import { JumpMenu } from "./JumpMenu";
import { VocalStrip } from "./VocalStrip";
import { useTrackCues } from "./useTrackCues";
import { READ_ONLY_REASON, useMemoryCues } from "./useMemoryCues";
import styles from "./Player.module.css";

export interface PlayerProps {
  /** The row the browser has selected, or `null` when nothing is. */
  track: RowDto | null;
  /** Which engine deck this drives. The 2-player layout adds a second. */
  deck?: DeckId;
  /**
   * The simple player: one strip — PLAY, the sleeve, the readouts over the
   * overview, the rating — drawn by `SimplePlayer` from this deck's state, so
   * the layout switch changes what is on screen and not what is playing.
   */
  simple?: boolean;
  /**
   * Take the track out of the deck.
   *
   * The artwork is the eject button, as it is on a CDJ's screen: clicking the
   * sleeve is how you get a track out without loading another over it.
   */
  onEject?: () => void;
  /**
   * Something the deck could not do.
   *
   * Reported rather than drawn: the message used to print across the pad row
   * and the tree underneath it. It belongs in the status bar, with everything
   * else the app has to say.
   */
  onError?: (message: string | null) => void;
  /**
   * A track dropped onto the deck.
   *
   * The whole deck takes the drop, not only the sleeve: rekordbox loads a
   * track dropped anywhere on a player, and a 79-pixel square is a small
   * target for a hand that is already carrying something. The sleeve is what
   * lights up, because that is where the track lands.
   *
   * What was dropped is not passed back: the shell started the drag and knows
   * what is in it, and a deck takes one track whoever is holding it.
   */
  onDropTrack?: () => void;
  /** A track is being dragged, so the deck can offer itself as a target. */
  dragging?: boolean;
  /**
   * Where the transport column is drawn, when it is not drawn here.
   *
   * The two-deck layouts share one transport column between the decks — deck A
   * down from the top, deck B up from the bottom, with the mixer strip beside
   * it — so the two halves are siblings in the shell's grid rather than each
   * inside its own deck.
   *
   * A portal rather than lifted state: everything the transport touches — the
   * playhead, the cue, the beat-jump size — belongs to this deck and is held
   * here, and moving all of that up to the shell to move a column would be a
   * great deal of state travelling for a layout change.
   */
  transportSlot?: HTMLElement | null;
  /** Deck B's transport reads bottom-up, mirroring deck A's. */
  flipped?: boolean;
  /**
   * The waveform zoom and the beat-jump size, when something outside is
   * driving them.
   *
   * DUAL CONTROL: with it on the shell holds one of each and hands the same
   * value to both decks, so zooming one zooms the other. Left out, the deck
   * keeps its own — a deck on its own has nothing to link to.
   */
  bars?: number;
  onBars?: (bars: number) => void;
  jumpSize?: string;
  onJumpSize?: (id: string) => void;
  /**
   * Sync, which needs both decks and so is arranged by the shell.
   *
   * `publishSync` registers a getter the *other* deck reads when its BEAT SYNC
   * is pressed, and `peerSync` is that other deck's. Getters rather than
   * state: a deck's position changes every frame and sync reads it once, at
   * the moment the button goes down.
   */
  publishSync?: ((get: () => SyncDeck | null) => void) | undefined;
  peerSync?: (() => SyncDeck | null) | undefined;
  /** Whether this deck is the one the other syncs to. */
  isMaster?: boolean;
  onMaster?: () => void;
  /**
   * Load whatever the browser has selected.
   *
   * The third load gesture, and the sleeve is where it lives: loaded, the
   * sleeve ejects, and empty it takes the selection. Absent — nothing
   * selected, or several things — the empty deck is inert.
   */
  onLoadSelected?: (() => void) | undefined;
  /**
   * rekordbox holds the database. The MEMORY cluster and the list's ✕ are
   * drawn and disabled, with the same reason the menus give.
   */
  readOnly?: boolean;
}

/**
 * Cue points on a waveform.
 *
 * A hot cue is a lettered badge, not a line: measured off `docs/screenshots`,
 * 11pt square, `#77E866`, black letter, its left edge on the cue. The overview
 * draws no line through the waveform at all — four hot cues, four badges, and
 * the waveform under them unbroken.
 *
 * A memory cue is a small red head at its position. The capture has one beside
 * each badge, which looked at first like part of the hot cue marker; the live
 * `djmdCue` says otherwise — the measured track carries a `Kind` 0 cue at the
 * same `InMsec` as each of its four hot cues, so the red belongs to those.
 *
 * One green for every slot, because that is what the data says rather than a
 * fallback: all four cues of the measured track carry `ColorTableIndex` 21,
 * and so do 735,427 of the library's 850,000 hot cues. The rest of the palette
 * stays unmapped — `cue_colours` found it in neither the database, the skins
 * nor the analysis files — so an index this has not seen still draws green.
 *
 * Exported for the simple player's overview, which is the same strip.
 */
export const CueMarkers = memo(function CueMarkers({
  cues, totalMs, band = "overview", window,
}: {
  cues: readonly Cue[];
  totalMs: number;
  /**
   * Which waveform this is drawn over.
   *
   * The overview hangs its badges from the top of the strip. No export-mode
   * capture has a cue inside the detail's twelve-bar window, so the detail
   * follows the performance deck, which sits its badge at the foot of the band
   * and keeps a line up through the waveform — the thing that makes a cue
   * placeable while the grid is being edited.
   */
  band?: "overview" | "detail";
  /** The slice of the track being shown, for the zoomed detail waveform. */
  window?: { from: number; to: number };
}) {
  if (totalMs <= 0) return null;
  const from = window?.from ?? 0;
  const to = window?.to ?? 1;
  const span = Math.max(to - from, 1e-6);
  return (
    <>
      {cues.map((cue) => {
        const at = cue.positionMs / totalMs;
        // A cue outside the window is not drawn at the edge — a marker pinned
        // to the edge reads as a cue that is there.
        if (at < from || at > to) return null;
        const left = `${((at - from) / span) * 100}%`;
        // The detail marks every cue the same way: a red triangle at the top
        // of the band. The overview tells them apart, because there is room to.
        const head =
          band === "detail" ? (
            <i className={styles.cueTriangle} />
          ) : cue.memory ? (
            <i className={styles.cueHead} />
          ) : (
            <b className={styles.hotCueBadge}>{cue.letter}</b>
          );
        return (
          <span
            key={cue.id || (cue.memory ? `m-${cue.positionMs}` : `h-${cue.letter}-${cue.positionMs}`)}
            className={cue.memory ? styles.memoryCue : styles.hotCue}
            data-band={band}
            data-cue={cue.memory ? "" : cue.letter}
            style={{ left }}
            title={cue.memory ? "Memory cue" : `Hot cue ${cue.letter}`}
            aria-hidden
          >
            {head}
          </span>
        );
      })}
    </>
  );
});

/**
 * The beat grid over the detail waveform.
 *
 * Downbeats are drawn heavier than the beats between them, which is what makes
 * a grid readable at a glance rather than a picket fence.
 */
const BeatGrid = memo(function BeatGrid({
  beats, totalMs, window, everyBeat = true,
}: {
  beats: readonly { timeMs: number; downbeat: boolean }[];
  totalMs: number;
  window: { from: number; to: number };
  /** False at the widest zoom, where only the bar lines are drawn. */
  everyBeat?: boolean;
}) {
  if (totalMs <= 0 || beats.length === 0) return null;
  const span = Math.max(window.to - window.from, 1e-6);
  return (
    <>
      {beats.map((beat) => {
        if (!everyBeat && !beat.downbeat) return null;
        const at = beat.timeMs / totalMs;
        if (at < window.from || at > window.to) return null;
        return (
          <span
            key={beat.timeMs}
            className={beat.downbeat ? styles.downbeat : styles.beat}
            style={{ left: `${((at - window.from) / span) * 100}%` }}
            aria-hidden
          />
        );
      })}
    </>
  );
});

/**
 * The phrase bar: the track's structure, from the `PSSI` tag.
 *
 * Labelled where a block is wide enough to hold its label and left as bare
 * colour where it is not, which is what rekordbox does — a clipped "CHORU"
 * is worse than a coloured block whose shape already says what it is.
 */
const PhraseBar = memo(function PhraseBar({
  phrases, totalMs, beatMs,
}: {
  phrases: readonly Phrase[];
  totalMs: number;
  /** Milliseconds a beat lasts, for phrases the beat grid did not reach. */
  beatMs: number;
}) {
  const spans = phraseSpans(phrases, totalMs, beatMs);
  return (
    <div className={styles.phrase} aria-label="Phrase" data-testid="player-phrase">
      {spans.map((span) => (
        <span
          key={`${span.from}-${span.label}`}
          className={styles.phraseBlock}
          data-kind={span.kind}
          style={{ left: `${span.from * 100}%`, width: `${(span.to - span.from) * 100}%` }}
          title={span.label}
        >
          {span.label}
        </span>
      ))}
    </div>
  );
});

/** Transport buttons, in the order and pairing rekordbox has them. */
const SKIPS = [
  { id: "previous", label: "Previous track", glyph: "❘◀" },
  { id: "next", label: "Next track", glyph: "▶❘" },
] as const;

const JUMPS = [
  { id: "jump-back", label: "Beat jump back", glyph: "‹" },
  { id: "jump-forward", label: "Beat jump forward", glyph: "›" },
] as const;

/**
 * What the waveform leaves clear at the top and bottom of its band.
 *
 * Measured: rekordbox's detail waveform paints y 374..647 inside a band that
 * runs 358..650. The strip above carries the bar count and the heads of the
 * cue markers, which is why it is the larger of the two. The overview does not
 * take it: its own grid row is already the 30pt the capture paints.
 */
const WAVE_INSET = { top: 11, bottom: 2 };

/** Hot cue slots, as the pad row lays them out. */
const PADS = ["A", "B", "C", "D", "E", "F", "G", "H"] as const;

/**
 * What GRID puts in the pad row, read off `docs/screenshots`.
 *
 * Two labelled sections, GRID EDIT and PHRASE EDIT, with the buttons grouped
 * in pairs. The glyphs are ours — Pioneer's own are reference for geometry
 * only — so each carries an `aria-label` saying what it does.
 *
 * Every one is inert. Saving a grid edit needs the `PQT2` tag, whose per-beat
 * payload is not understood, and an editor that cannot save is a trap.
 */
const GRID_EDITS: readonly (readonly { id: string; label: string; text: string }[])[] = [
  [{ id: "mark", label: "Mark the downbeat here", text: "▌" }],
  [{ id: "tap", label: "Tap the tempo", text: "TAP" }],
  [
    { id: "shift-back", label: "Shift the grid earlier", text: "◀|||" },
    { id: "shift-forward", label: "Shift the grid later", text: "|||▶" },
  ],
  [
    { id: "widen", label: "Slow the grid", text: "◀|▶" },
    { id: "narrow", label: "Speed the grid up", text: "▶|◀" },
  ],
  [
    { id: "double", label: "Double the tempo", text: "×2" },
    { id: "halve", label: "Halve the tempo", text: "×\u00bd" },
  ],
  [
    { id: "snap-start", label: "Snap the grid to the start", text: "|↓|" },
    { id: "snap-here", label: "Snap the grid here", text: "||↓" },
  ],
  [
    { id: "undo", label: "Undo the last grid edit", text: "↺" },
    { id: "redo", label: "Redo the last grid edit", text: "↻" },
  ],
  [
    { id: "cut-grid", label: "Cut the grid here", text: "" },
    { id: "metronome", label: "Metronome", text: "" },
    { id: "lock", label: "Lock the grid", text: "" },
  ],
];

/** Buttons whose face is one of our icons rather than a glyph. */
const EDIT_ICONS: Record<string, (props: { className?: string | undefined }) => React.ReactElement> = {
  "cut-grid": CutIcon,
  metronome: MetronomeIcon,
  lock: LockIcon,
};

/** The phrase-editing controls, to the right of the grid ones. */
const PHRASE_EDITS = [
  { id: "phrase-cut", label: "Cut the phrase here", text: "CUT" },
  { id: "phrase-clear", label: "Clear the phrase", text: "CLEAR" },
] as const;

/** The file's kind, from its name. Nothing else about the file is indexed. */
function fileKind(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? `${name.slice(dot + 1).toUpperCase()} File` : "Audio File";
}

/** The panel tabs beside the deck. */
const PANELS = [
  { id: "memory", label: "MEMORY" },
  { id: "hotCue", label: "HOT CUE" },
  { id: "info", label: "INFO" },
] as const;

export const Player = memo(function Player({
  track, onEject, onError, onDropTrack, onLoadSelected, dragging = false, deck = "a",
  simple = false, transportSlot, flipped = false,
  bars: linkedBars, onBars, jumpSize: linkedJump, onJumpSize,
  publishSync, peerSync, isMaster = false, onMaster, readOnly = false,
}: PlayerProps) {
  const playback = usePlayback(track?.id ?? null, deck);
  // The waveforms follow their containers, which change with the window and
  // with the tree splitter — a fixed-width canvas stretched by CSS is blurry
  // on a wide window and wasted resolution on a narrow one.
  const [overviewRef, overview] = useElementSize<HTMLDivElement>();
  const [detailRef, detail] = useElementSize<HTMLDivElement>();
  // Written to by the frame loop below rather than rendered: see the effect.
  const overviewHead = useRef<HTMLSpanElement>(null);
  const detailHead = useRef<HTMLSpanElement>(null);
  const scrubFill = useRef<HTMLDivElement>(null);
  const barsLabel = useRef<HTMLSpanElement>(null);
  const [grid, setGrid] = useState<BeatGridData>(NO_BEATS);
  const [phrases, setPhrases] = useState<Phrase[]>([]);
  const [ownBars, setOwnBars] = useState<number>(DETAIL_BARS);
  // Linked or its own, and the setter follows whichever it is: a controlled
  // zoom that kept updating a local copy would fight the link on every change.
  const bars = linkedBars ?? ownBars;
  const setBars = useCallback(
    (next: number | ((current: number) => number)) => {
      const resolve = (current: number) =>
        typeof next === "function" ? next(current) : next;
      if (onBars) onBars(resolve(linkedBars ?? ownBars));
      else setOwnBars(resolve);
    },
    [onBars, linkedBars, ownBars],
  );
  const [padMode, setPadMode] = useState<PadMode>("cue");
  const [panel, setPanel] = useState<CuePanel>("memory");
  /**
   * Where CUE returns to. A track opens on its first memory cue, which is
   * where rekordbox and a CDJ both put the playhead, and CUE moves it from
   * there the way the deck does.
   */
  const [cuePoint, setCuePoint] = useState(0);
  // Kept current by the backend: an edit from any deck refetches. The cue
  // point is settled from the first fetch only — see `useTrackCues`.
  const cues = useTrackCues(track, (loaded) => {
    const first = cuesFor(loaded, "memory")[0];
    setCuePoint(first ? first.positionMs / 1000 : 0);
  });
  /**
   * Quantize — the Q button at the end of the pad row. On by default, as a CDJ
   * ships: a cue set by hand lands tens of milliseconds off the beat, and every
   * loop and mix taken from it inherits that.
   */
  const [quantize, setQuantize] = useState(true);
  /** How far a jump moves, chosen from the size menu. */
  const [ownJumpSizeId, setOwnJumpSizeId] = useState<string>(JUMP_SIZE_ID);
  const jumpSizeId = linkedJump ?? ownJumpSizeId;
  const setJumpSizeId = onJumpSize ?? setOwnJumpSizeId;
  /** Where the size menu is open, in client coordinates, or closed. */
  const [jumpMenu, setJumpMenu] = useState<{ x: number; y: number } | null>(null);
  const jumpButton = useRef<HTMLButtonElement>(null);
  /**
   * Whether the deck has the keyboard.
   *
   * Armed by clicking it and dropped by clicking anything else, which is how a
   * CDJ's deck behaves and what makes the arrow keys unambiguous: the same
   * keys move the browser's cursor when the browser has it.
   */
  const [armed, setArmed] = useState(false);
  const platform = useMemo(detectPlatform, []);

  // Handed up as it changes, so the status bar owns the only place the app
  // says something went wrong.
  const { error: deckError } = playback;
  useEffect(() => {
    onError?.(deckError);
  }, [deckError, onError]);
  /**
   * Where the scrolling layer is drawn from. Not the playhead: the layer is
   * drawn once across `OVERDRAW` spans and slid by a transform, and it is
   * redrawn only when the head has travelled far enough to see its edge.
   */
  const [anchor, setAnchor] = useState(0);
  /** The anchor the layer is actually showing, so the slide never leads it. */
  const drawn = useRef(0);
  const scroller = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!track) {
      setPhrases([]);
      return;
    }
    let live = true;
    void (async () => {
      const backend = await getBackend();
      const found = await backend.trackPhrases(track.id);
      // The track may have changed while this was in flight.
      if (live) setPhrases(found);
    })();
    return () => {
      live = false;
    };
  }, [track]);

  // Falls back to the track's own length before the file's metadata has
  // loaded, so nothing jumps when it arrives.
  const total = playback.duration || track?.durationSec || 0;

  // Bars rather than a fraction: twelve bars is twelve bars whether the track
  // is three minutes or ninety.
  const span = detailSpan(bars, track?.bpmX100 ?? 0, total);
  // Memoised, not rebuilt each render: `BeatGrid` and `CueMarkers` are
  // `memo()` components taking this object, and a fresh one every render means
  // neither ever hits its memo.
  const window = useMemo(() => windowAround(anchor, span * OVERDRAW), [anchor, span]);

  // The whole grid, once per track, as raw bytes. Fetching a window at a time
  // still re-read and re-parsed the entire analysis file on every fetch —
  // several times a second while playing — which is the expensive part however
  // small the slice that comes back.
  useEffect(() => {
    if (!track || !track.analysed) {
      setGrid(NO_BEATS);
      return;
    }
    let live = true;
    void (async () => {
      const backend = await getBackend();
      const bytes = await backend.trackBeats(track.id);
      if (live) setGrid(parseBeatGrid(bytes));
    })();
    return () => {
      live = false;
    };
  }, [track]);

  // The beats the detail window actually draws, found by binary search rather
  // than by filtering the whole grid on every tick.
  const beats = useMemo(
    () => (total > 0 ? beatsIn(grid, window.from * total * 1000, window.to * total * 1000) : []),
    [grid, window, total],
  );

  // The tempo, for the bar count the frame loop prints.
  const bpm = (track?.bpmX100 ?? 0) / 100;
  const { positionRef, subscribe } = playback;

  /*
   * The playhead, written straight to its elements every frame.
   *
   * Not React state: `position` feeds this whole subtree, so ticking it faster
   * only re-renders the player faster — the head still steps, it just steps
   * more often. A transform through a ref moves it on the compositor without
   * laying anything out, and the state below stays at ten a second for the
   * readouts and the waveform.
   */
  useEffect(() => {
    const apply = (seconds: number) => {
      const at = total > 0 ? Math.min(seconds / total, 1) : 0;
      if (overviewHead.current) {
        overviewHead.current.style.transform = `translateX(${at * overview.width}px)`;
      }
      if (scrubFill.current) scrubFill.current.style.transform = `scaleX(${at})`;
      // The detail head does not move at all: the layer under it does, by a
      // transform on the compositor rather than a redraw. Redrawing the canvas
      // from React state stepped it at the tick rate — ten times a second,
      // which is what made a scrolling waveform look like a slideshow.
      if (scroller.current) {
        const dx = scrollOffset(at, drawn.current, span, detail.width);
        scroller.current.style.transform = `translateX(${dx}px)`;
      }
      if (needsRedraw(at, drawn.current, span)) setAnchor(at);
      const x = (headPercent() / 100) * detail.width;
      if (detailHead.current) detailHead.current.style.transform = `translateX(${x}px)`;
      if (barsLabel.current) {
        barsLabel.current.style.transform = `translateX(${x}px)`;
        barsLabel.current.textContent = bpm > 0 ? `${((seconds * bpm) / 60 / 4).toFixed(1)}Bars` : "";
      }
    };
    // At once as well as on every frame: a paused player schedules no frames,
    // and the head would otherwise sit where the last track left it.
    apply(positionRef.current);
    return subscribe(apply);
  }, [total, bpm, span, overview.width, detail.width, positionRef, subscribe]);

  /*
   * The layer's new anchor, taken only once it is on screen.
   *
   * A layout effect, and after the canvas's own: children run first, so by the
   * time this slides the layer back the redraw it is sliding for has already
   * happened. Updating the anchor in the frame loop instead moved the layer a
   * frame before its contents caught up, which showed as a jump.
   */
  useLayoutEffect(() => {
    drawn.current = anchor;
    if (scroller.current) {
      const at = total > 0 ? Math.min(positionRef.current / total, 1) : 0;
      scroller.current.style.transform = `translateX(${scrollOffset(at, anchor, span, detail.width)}px)`;
    }
  }, [anchor, span, total, detail.width, positionRef]);

  /**
   * The wheel zooms, over the waveform it is pointing at.
   *
   * One step per gesture whatever the device: a mouse notch arrives as about a
   * hundred pixels and a trackpad as a stream of ones, so the size of the
   * delta says nothing useful and only its sign is read.
   */
  const wheelZoom = useCallback((event: React.WheelEvent<HTMLDivElement>) => {
    if (event.deltaY === 0) return;
    event.preventDefault();
    setBars((current) => zoomBy(current, event.deltaY > 0 ? 1 : -1));
  }, [setBars]);

  const zoom = useCallback((by: number) => {
    setBars((current) => {
      const at = ZOOM_STEPS.indexOf(current as (typeof ZOOM_STEPS)[number]);
      // An unrecognised value snaps back to the default rather than sticking.
      const from = at === -1 ? ZOOM_STEPS.indexOf(DETAIL_BARS) : at;
      const to = Math.min(Math.max(from + by, 0), ZOOM_STEPS.length - 1);
      return ZOOM_STEPS[to] ?? DETAIL_BARS;
    });
  }, [setBars]);

  /*
   * The deck takes the keyboard when it is clicked and gives it up when
   * anything else is. A document listener rather than `onBlur`: the browser
   * and the tree are not focusable containers, so there is nothing to blur to
   * — what matters is that the pointer went down somewhere that is not here.
   */
  const shell = useRef<HTMLElement>(null);
  useEffect(() => {
    const elsewhere = (event: PointerEvent) => {
      const box = shell.current;
      if (!box) return;
      setArmed(box.contains(event.target as Node));
    };
    document.addEventListener("pointerdown", elsewhere, true);
    return () => document.removeEventListener("pointerdown", elsewhere, true);
  }, []);

  /** Moves by the chosen size: a number of beats, or the fine nudge. */
  const jump = useCallback(
    (direction: number) => {
      const step = jumpStepSeconds(jumpSizeById(jumpSizeId), track?.bpmX100 ?? 0);
      if (step === 0) return;
      playback.seek(playback.positionRef.current + step * direction);
    },
    [jumpSizeId, track, playback],
  );


  /*
   * CUE, as a CDJ does it: stop and rewind while playing, preview while held
   * on the cue point, set the cue point anywhere else. `pressCue` decides
   * which; this only carries it out and remembers whether a preview is running.
   */
  const previewing = useRef(false);

  const holdCue = useCallback(() => {
    if (playback.idle) return;
    const action = pressCue(
      playback.positionRef.current,
      cuePoint,
      playback.playing,
      quantize ? grid : null,
    );
    previewing.current = action.playing;
    if (action.cuePoint !== cuePoint) setCuePoint(action.cuePoint);
    if (action.seekTo !== null) playback.seek(action.seekTo);
    if (action.playing !== playback.playing) playback.toggle();
  }, [playback, cuePoint, quantize, grid]);

  const dropCue = useCallback(() => {
    const action = releaseCue(previewing.current, cuePoint);
    previewing.current = false;
    if (!action) return;
    playback.seek(action.seekTo ?? cuePoint);
    if (playback.playing) playback.toggle();
  }, [playback, cuePoint]);

  const { seek } = playback;
  const positionSeconds = useCallback(() => positionRef.current, [positionRef]);
  const memory = useMemoryCues({
    trackId: playback.idle ? null : track?.id ?? null,
    cues, positionSeconds, seek, cuePoint, setCuePoint, readOnly, onError,
  });

  /*
   * The deck's keys, from rekordbox's own Export key map — see `shortcuts.ts`.
   *
   * Space, C, Q and F10-F12 belong to the deck whenever nothing is being typed
   * into, as they do in rekordbox. The arrows are the exception: they move the
   * browser's cursor as readily as the track, so they wait until the deck has
   * been clicked, which is what turns the playhead red.
   */
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const action = dispatch(event, platform, event.target as HTMLElement | null);
      if (action === null) return;
      if (action === "jumpBack" || action === "jumpForward") {
        if (!armed) return;
        // Otherwise the list scrolls under the deck at the same time.
        event.preventDefault();
        jump(action === "jumpForward" ? 1 : -1);
        return;
      }
      if (playback.idle && action !== "showMemory" && action !== "showHotCues"
        && action !== "showInfo") {
        return;
      }
      switch (action) {
        case "playPause":
          // Space scrolls the page otherwise.
          event.preventDefault();
          playback.toggle();
          break;
        case "cue":
          // Pressed, not tapped. CUE is a held control on the hardware and in
          // rekordbox: on the cue point it plays for as long as it is down and
          // snaps back when it comes up, which is how a preview works. Doing
          // both on the key down made the key the one control that could not
          // preview — and auto-repeat then ran the pair thirty times a second
          // for as long as the key was held. `keyup` below lets go.
          if (!event.repeat) holdCue();
          break;
        case "quantize":
          setQuantize((on) => !on);
          break;
        case "showMemory":
          event.preventDefault();
          setPanel("memory");
          break;
        case "showHotCues":
          event.preventDefault();
          setPanel("hotCue");
          break;
        case "showInfo":
          event.preventDefault();
          setPanel("info");
          break;
        case "memoryCue":
          if (!event.repeat) memory.store();
          break;
        case "previousMemoryCue":
          memory.callPrevious();
          break;
        case "nextMemoryCue":
          memory.callNext();
          break;
        case "deleteMemoryCue":
          if (!event.repeat) memory.deleteAtHead();
          break;
        default:
          break;
      }
    };
    /**
     * Letting go of CUE.
     *
     * Mapped without the typing guard `dispatch` applies, on purpose: a key
     * released while the search box has the focus still has to end a preview
     * that is running, and `dropCue` does nothing when none is. The same
     * reasoning covers the window losing focus altogether — a preview that
     * outlives the key would play on with nothing able to stop it.
     */
    const onKeyUp = (event: KeyboardEvent) => {
      if (actionFor({ key: event.key, metaKey: event.metaKey, ctrlKey: event.ctrlKey,
        shiftKey: event.shiftKey, altKey: event.altKey }, platform) === "cue") {
        dropCue();
      }
    };
    const onBlur = () => dropCue();

    // `globalThis`, because `window` here is the slice of the track on screen.
    globalThis.addEventListener("keydown", onKey);
    globalThis.addEventListener("keyup", onKeyUp);
    globalThis.addEventListener("blur", onBlur);
    return () => {
      globalThis.removeEventListener("keydown", onKey);
      globalThis.removeEventListener("keyup", onKeyUp);
      globalThis.removeEventListener("blur", onBlur);
    };
  }, [armed, jump, platform, playback, holdCue, dropCue, memory]);

  /**
   * The overview is a scrubber: the pointer goes where you put it, and holding
   * it down drags the playhead along the track.
   */
  const scrubOverview = (event: React.PointerEvent<HTMLDivElement>) => {
    const element = event.currentTarget;
    const box = element.getBoundingClientRect();
    if (box.width <= 0) return;
    element.setPointerCapture(event.pointerId);
    playback.scrubBegin();
    playback.scrubTo(((event.clientX - box.left) / box.width) * total);
  };

  const dragOverview = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!event.currentTarget.hasPointerCapture(event.pointerId)) return;
    const box = event.currentTarget.getBoundingClientRect();
    if (box.width <= 0) return;
    playback.scrubTo(((event.clientX - box.left) / box.width) * total);
  };

  /**
   * The detail is the record, not a scrubber: it moves *with* the pointer, so
   * dragging right pulls earlier music into view. Absolute seeking here would
   * jump the track by half a window on the first pixel of movement, because
   * the head sits in the middle whatever it is pointing at.
   */
  const grab = useRef<{ x: number; at: number } | null>(null);

  const startDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    grab.current = { x: event.clientX, at: playback.positionRef.current };
    playback.scrubBegin();
  };

  const dragDetail = (event: React.PointerEvent<HTMLDivElement>) => {
    const held = grab.current;
    if (!held || !event.currentTarget.hasPointerCapture(event.pointerId)) return;
    const box = event.currentTarget.getBoundingClientRect();
    playback.scrubTo(held.at + dragSeconds(event.clientX - held.x, box.width, span, total));
  };

  const endDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    grab.current = null;
    playback.scrubEnd();
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  // A beat's length, for phrases whose time the grid did not resolve.
  const beatMs = (track?.bpmX100 ?? 0) > 0 ? 6_000_000 / (track?.bpmX100 ?? 1) : 0;
  const remaining = splitTime(Math.max(total - playback.position, 0));
  const elapsed = splitTime(playback.position);

  // What the other deck reads when its BEAT SYNC is pressed. A ref holding a
  // closure over the current render, registered once: the shell keeps the
  // getter, not the values, so nothing here re-renders anything there.
  const syncState = useRef<() => SyncDeck | null>(() => null);
  syncState.current = () =>
    track
      ? { bpmX100: track.bpmX100, position: playback.positionRef.current, grid }
      : null;
  useEffect(() => {
    publishSync?.(() => syncState.current());
  }, [publishSync]);

  /** Match this deck to the other one: its tempo, then its bar. */
  const beatSync = useCallback(() => {
    const leader = peerSync?.();
    const follower = syncState.current();
    if (!leader || !follower) return;
    const { tempo, nudge } = syncTo(leader, follower);
    playback.setTempo(tempo);
    // The nudge second: it is measured against where the follower is now, and
    // the tempo does not move the playhead.
    if (Math.abs(nudge) > 0.001) playback.seek(follower.position + nudge);
  }, [peerSync, playback]);

  const takesDrop = dragging && Boolean(onDropTrack);

  const dragOver = (event: React.DragEvent<HTMLElement>) => {
    if (!takesDrop) return;
    // Without the preventDefault the browser refuses the drop and the
    // cursor says so, whatever the handler below would have done.
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
  };
  const drop = (event: React.DragEvent<HTMLElement>) => {
    if (!takesDrop) return;
    event.preventDefault();
    onDropTrack?.();
  };

  // The simple player is this deck drawn as one strip. Everything above —
  // the engine, the cues, the frame loop, the keys — is still this
  // component's, which is what keeps the track playing across the switch.
  if (simple) {
    return (
      <SimplePlayer
        track={track}
        deck={deck}
        shell={shell}
        armed={armed}
        droppable={takesDrop}
        onDragOver={dragOver}
        onDrop={drop}
        playing={playback.playing}
        idle={playback.idle}
        onToggle={playback.toggle}
        position={playback.position}
        total={total}
        cues={cues}
        cuePoint={cuePoint}
        overviewRef={overviewRef}
        overview={overview}
        overviewHead={overviewHead}
        scrubFill={scrubFill}
        onScrubStart={scrubOverview}
        onScrubMove={dragOverview}
        onScrubEnd={endDrag}
        onEject={onEject}
        onLoadSelected={onLoadSelected}
      />
    );
  }

  // The track skips, which the two-deck column does not draw: rekordbox
  // drops them there, and half a deck's height has no room for them.
  const skips = (
        <div className={styles.pair}>
          {SKIPS.map((button) => (
            <button
              key={button.id}
              type="button"
              className={styles.square}
              aria-label={button.label}
              // No playlist cursor behind these yet, so they are drawn and
              // inert rather than absent.
              disabled
            >
              {button.glyph}
            </button>
          ))}
        </div>
  );

  const jumps = (
        <div className={styles.pair}>
          {JUMPS.map((button) => (
            <button
              key={button.id}
              type="button"
              className={styles.square}
              aria-label={button.label}
              disabled={playback.idle}
              onClick={() => jump(button.id === "jump-back" ? -1 : 1)}
            >
              {button.glyph}
            </button>
          ))}
        </div>
  );

  const beatSize = (
        <button
          ref={jumpButton}
          type="button"
          className={styles.beats}
          aria-label="Beat jump size"
          aria-haspopup="menu"
          aria-expanded={jumpMenu !== null}
          // Transcribed from rekordbox: "Select the beat/bar length jumping
          // from the current position."
          title="Select the beat/bar length jumping from the current position."
          onClick={(event) => {
            const box = event.currentTarget.getBoundingClientRect();
            // Opened beside the button rather than under it: the deck sits at
            // the bottom of the window and a menu below would be off screen.
            setJumpMenu((open) => (open ? null : { x: box.right + 6, y: box.top }));
          }}
        >
          {jumpSizeById(jumpSizeId).label}
          <span className={styles.chevron} aria-hidden />
        </button>
  );

  const cueButton = (
        <button
          type="button"
          className={styles.cue}
          aria-label="Cue"
          // Held, not clicked: on the cue point the deck plays for as long as
          // the button is down and snaps back when it comes up.
          onPointerDown={holdCue}
          onPointerUp={dropCue}
          onPointerCancel={dropCue}
          onPointerLeave={dropCue}
          disabled={playback.idle}
        >
          CUE
        </button>
  );

  const playButton = (
        <button
          type="button"
          className={styles.play}
          data-on={playback.playing ? "" : undefined}
          aria-label={playback.playing ? "Pause" : "Play"}
          onClick={playback.toggle}
          disabled={playback.idle}
        >
          {/* Both are here so hover swaps them in CSS: a state change for a
              pointer moving over a button is a re-render the frame loop does
              not need to share the frame with. */}
          <span className={styles.playGlyph} aria-hidden />
          <span className={styles.pauseGlyph} aria-hidden />
        </button>
  );

  // The transport column, as a value: it is drawn here in the one-deck
  // layouts and portalled into the column the two decks share in the others.
  //
  // The order differs between them, and follows the capture. One deck reads
  // jumps, size, CUE, PLAY down the column. Two decks put CUE and PLAY at the
  // outside of each half and the jump controls towards the middle, so the two
  // halves mirror as groups — within a group the order is the same either way,
  // which is what the capture shows.
  const transport = (
    <div
      className={styles.transport}
      data-flipped={flipped || undefined}
      data-shared={transportSlot ? "" : undefined}
      // Named, because in the two-deck layouts it is drawn outside the deck it
      // belongs to: without this the CUE and PLAY of a deck would be a pair of
      // unattached buttons to anything reading the page.
      role="group"
      aria-label={deck === "b" ? "Deck B transport" : "Deck A transport"}
    >
      {transportSlot ? null : skips}
      {transportSlot && !flipped ? (
        <>
          {cueButton}
          {playButton}
          {jumps}
          {beatSize}
        </>
      ) : (
        <>
          {jumps}
          {beatSize}
          {cueButton}
          {playButton}
        </>
      )}
    </div>
  );

  return (
    <section
      ref={shell}
      className={styles.player}
      aria-label={deck === "b" ? "Preview player B" : "Preview player"}
      data-armed={armed ? "" : undefined}
      data-empty={track ? undefined : ""}
      // The transport is drawn elsewhere, so the deck is two columns wide
      // rather than three.
      data-shared={transportSlot ? "" : undefined}
      // Deck B, which reads bottom-up so the two decks' waveforms meet at the
      // line between them.
      data-flipped={flipped || undefined}
      data-droppable={takesDrop || undefined}
      onDragOver={dragOver}
      onDrop={drop}
    >
      {transportSlot ? createPortal(transport, transportSlot) : transport}

      <div className={styles.main}>
        <div className={styles.head}>
          <span className={styles.title} data-testid="player-title">
            {track ? track.title : ""}
          </span>
          {track ? (
            <>
              <span className={styles.artist}>{track.artist}</span>
              <span className={styles.remaining} data-testid="player-time">
                -{remaining.main}
                <i className={styles.tenths}>.{remaining.tenths}</i>
              </span>
              <span className={styles.elapsed}>
                {elapsed.main}
                <i className={styles.tenths}>.{elapsed.tenths}</i>
              </span>
              <span className={styles.readout}>{track.key}</span>
              <span className={styles.readout}>{formatBpm(track.bpmX100)}</span>
            </>
          ) : null}
          {/* Sync belongs to the two-deck layouts and to nothing else: one
              deck has nothing to sync to and nothing to be master of. */}
          {peerSync ? (
            <div className={styles.sync} role="group" aria-label="Sync">
              <button
                type="button"
                className={styles.chip}
                aria-label="Beat sync"
                disabled={!track || isMaster}
                title={
                  isMaster
                    ? "This deck is the master; sync the other one to it."
                    : "Match this deck to the master's tempo and bar."
                }
                onClick={beatSync}
              >
                BEAT SYNC
              </button>
              <button
                type="button"
                className={styles.chip}
                aria-label="Sync master"
                aria-pressed={isMaster}
                data-on={isMaster ? "" : undefined}
                onClick={onMaster}
              >
                MASTER
              </button>
            </div>
          ) : null}
        </div>

        <div className={styles.overviewRow}>
          <button
            type="button"
            className={styles.artwork}
            // Loaded, the sleeve ejects — as it does on a CDJ's screen. Empty,
            // it takes whatever the browser has selected, which is the third
            // way a track reaches a deck alongside the drop and the menu.
            aria-label={track ? "Eject" : "Load the selected track"}
            title={track ? "Eject" : "Load the selected track"}
            onClick={track ? onEject : onLoadSelected}
            disabled={track ? !onEject : !onLoadSelected}
          >
            {track?.hasArtwork ? (
              <Artwork trackId={track.id} className={styles.sleeve} />
            ) : (
              <DiscIcon className={styles.disc} />
            )}
            {/* Shown on hover, over a scrim: what the sleeve does when clicked
                is not otherwise guessable from a sleeve. */}
            {track ? <EjectIcon className={styles.eject} /> : null}
          </button>
          <div className={styles.overviewStack}>
            {/* Where the vocals are, from the analysis. */}
            <VocalStrip trackId={track && track.analysed ? track.id : null} />
            {/*
              Clicking either waveform seeks, which is what they are for.

              Reported, not operated: the overview scrubs with the pointer and
              has no keys of its own — the arrows already belong to the deck. A
              tab stop here would only park the focus somewhere the keyboard can
              do nothing, and then paint a ring around the waveform the next
              time any key went down.
            */}
            <div
              ref={overviewRef}
              className={styles.overview}
              data-testid="player-overview"
              onPointerDown={scrubOverview}
              onPointerMove={dragOverview}
              onPointerUp={endDrag}
              onPointerCancel={endDrag}
              role="progressbar"
              aria-label="Position"
              aria-valuemin={0}
              aria-valuemax={Math.round(total)}
              aria-valuenow={Math.round(playback.position)}
            >
              {track && track.analysed ? (
                <WaveformDetail
                  trackId={track.id}
                  progress={0.5}
                  span={1}
                  width={overview.width}
                  height={overview.height}
                  half
                />
              ) : null}
              <CueMarkers cues={cues} totalMs={total * 1000} />
              <span
                ref={overviewHead}
                className={styles.playhead}
                data-testid="player-head"
                aria-hidden
              />
            </div>
            {/* How far through the track the head is, under the overview. */}
            <div className={styles.scrub} aria-hidden>
              <div ref={scrubFill} className={styles.scrubFill} />
            </div>
          </div>
        </div>

        <PhraseBar phrases={phrases} totalMs={total * 1000} beatMs={beatMs} />

        <div className={styles.detailRow}>
          <div className={styles.zoom}>
            <button type="button" aria-label="Zoom in" onClick={() => zoom(-1)}>+</button>
            <span className={styles.rst} aria-hidden>RST</span>
            <button type="button" aria-label="Zoom out" onClick={() => zoom(1)}>−</button>
          </div>
          <div
            ref={detailRef}
            className={styles.detail}
            data-testid="player-detail"
            onWheel={wheelZoom}
            onPointerDown={startDrag}
            onPointerMove={dragDetail}
            onPointerUp={endDrag}
            onPointerCancel={endDrag}
          >
            <div ref={scroller} className={styles.scroller}>
              {track && track.analysed ? (
                <WaveformDetail
                  trackId={track.id}
                  progress={anchor}
                  span={span * OVERDRAW}
                  width={detail.width * OVERDRAW}
                  height={detail.height}
                  detail
                  inset={WAVE_INSET}
                />
              ) : null}
              <BeatGrid
                beats={beats}
                totalMs={total * 1000}
                window={window}
                everyBeat={showsEveryBeat(bars)}
              />
              <CueMarkers cues={cues} totalMs={total * 1000} band="detail" window={window} />
            </div>
            {/* Bars elapsed, printed to the left of the playhead. Its text and
                its position are both the frame loop's, so React renders it
                empty and never touches it again. */}
            {track && track.bpmX100 > 0 ? (
              <span ref={barsLabel} className={styles.bars} data-testid="player-bars" />
            ) : null}
            {/* Fixed in the middle; the layer above scrolls under it. */}
            <span
              ref={detailHead}
              className={styles.playhead}
              data-testid="player-detail-head"
              aria-hidden
            />
          </div>
        </div>

        <div className={styles.pads}>
          {/*
            Two stacked tabs, not a pair of pills. The selected one takes the
            row's own colour and the other is cut out in black, which is what
            the capture shows and the opposite of the usual convention.
          */}
          <div className={styles.modes} role="tablist" aria-label="Pad mode">
            {([["cue", "CUE/LOOP"], ["grid", "GRID"]] as const).map(([id, label]) => (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={padMode === id}
                className={styles.mode}
                data-on={padMode === id || undefined}
                onClick={() => setPadMode(id)}
              >
                {label}
              </button>
            ))}
          </div>

          {padMode === "grid" ? (
            <div className={styles.gridRow}>
              <section className={styles.editGroup} aria-label="Beat grid">
                <span className={styles.sectionLabel}>GRID EDIT</span>
                <div className={styles.editButtons}>
                  {GRID_EDITS.map((group, at) => (
                    <div key={group[0]?.id ?? at} className={styles.editPair}>
                      {at === 1 ? <span className={styles.bpmField}>{formatBpm(track?.bpmX100 ?? 0)}</span> : null}
                      {group.map((edit) => (
                        <button
                          key={edit.id}
                          type="button"
                          className={styles.editButton}
                          data-mark={edit.id === "mark" || undefined}
                          aria-label={edit.label}
                          // Saving needs the PQT2 tag, whose payload is not
                          // understood; an editor that cannot save is a trap.
                          disabled
                          title="Grid editing needs the PQT2 tag, which is not yet understood"
                        >
                          {EDIT_ICONS[edit.id]
                            ? // Our own icons: Pioneer's are reference for
                              // geometry only, and an emoji renders in colour.
                              (() => {
                                const Icon = EDIT_ICONS[edit.id];
                                return Icon ? <Icon className={styles.editIcon} /> : null;
                              })()
                            : edit.text}
                        </button>
                      ))}
                    </div>
                  ))}
                </div>
              </section>

              <span className={styles.padSpacer} />

              <section className={styles.editGroup} aria-label="Phrase">
                <span className={styles.sectionLabel}>PHRASE EDIT</span>
                <div className={styles.editButtons}>
                  <div className={styles.editPair}>
                    <button type="button" className={styles.wideButton} aria-label={PHRASE_EDITS[0].label} disabled>
                      {PHRASE_EDITS[0].text}
                    </button>
                    <span className={styles.phraseField} aria-hidden />
                    <button type="button" className={styles.wideButton} aria-label={PHRASE_EDITS[1].label} disabled>
                      {PHRASE_EDITS[1].text}
                    </button>
                  </div>
                </div>
              </section>
            </div>
          ) : (
          <div className={styles.padCluster}>
            <div className={styles.hotCues} aria-label="Hot cues">
              {PADS.map((letter) => {
                const set = cues.some((cue) => !cue.memory && cue.letter === letter);
                return (
                  <button
                    key={letter}
                    type="button"
                    className={styles.pad}
                    data-set={set || undefined}
                    aria-label={`Hot cue ${letter}`}
                    aria-pressed={set}
                    onClick={() => {
                      const cue = cues.find((c) => !c.memory && c.letter === letter);
                      if (cue) playback.seek(cue.positionMs / 1000);
                    }}
                  >
                    <span className={styles.padInner}>{letter}</span>
                  </button>
                );
              })}
            </div>

            {/* MEMORY stores the cue point, ◀ ▶ call the memory cue either
                side of the playhead, ✕ deletes the one it is on — `Set Memory
                Cue`, `Call Previous/Next Memory Cue`, `Delete Memory Cue` in
                german.lang, on M, B, N and X in the Export key map. Calling
                needs no write and works read-only; the rest is disabled with
                the reason the menus give. */}
            <div className={styles.memory} aria-label="Memory cues">
              <button
                type="button"
                className={styles.memoryLabel}
                aria-label="Set memory cue"
                title={readOnly ? READ_ONLY_REASON : "Set Memory Cue (M)"}
                disabled={!memory.canEdit}
                onClick={memory.store}
              >
                MEMORY
              </button>
              <button
                type="button"
                className={styles.step}
                aria-label="Previous memory cue"
                title="Call Previous Memory Cue (B)"
                disabled={playback.idle}
                onClick={memory.callPrevious}
              >
                ◀
              </button>
              <button
                type="button"
                className={styles.step}
                aria-label="Next memory cue"
                title="Call Next Memory Cue (N)"
                disabled={playback.idle}
                onClick={memory.callNext}
              >
                ▶
              </button>
              <button
                type="button"
                className={styles.step}
                aria-label="Delete memory cue"
                title={readOnly ? READ_ONLY_REASON : "Delete Memory Cue (X)"}
                disabled={!memory.canEdit}
                onClick={memory.deleteAtHead}
              >
                ✕
              </button>
            </div>

            <div className={styles.auto} role="group" aria-label="Cue mode">
              <button type="button" className={styles.chip} data-on aria-pressed>AU</button>
              <button type="button" className={styles.chip} aria-pressed={false}>MA</button>
            </div>

            <div className={styles.page} aria-label="Pad page">
              <button type="button" className={styles.step} aria-label="Previous page" disabled>‹</button>
              <span className={styles.pageNumber}>2</span>
              <button type="button" className={styles.step} aria-label="Next page" disabled>›</button>
            </div>
          </div>
          )}

          {/* The tempo cluster, as the capture has it: the BPM the deck is
              playing at with a step either side, the key lock, and a reset.
              rekordbox puts them between the pads and Q. */}
          <div className={styles.tempo} role="group" aria-label="Tempo">
            <button
              type="button"
              className={styles.step}
              aria-label="Slower"
              disabled={playback.idle}
              onClick={() => playback.nudgeTempo(-1)}
            >
              −
            </button>
            <span className={styles.bpmField} data-testid="player-bpm">
              {formatBpm(Math.round((track?.bpmX100 ?? 0) * playback.tempo))}
            </span>
            <button
              type="button"
              className={styles.step}
              aria-label="Faster"
              disabled={playback.idle}
              onClick={() => playback.nudgeTempo(1)}
            >
              +
            </button>
            <button
              type="button"
              className={styles.chip}
              // Master Tempo, which rekordbox labels MT: the key stays put
              // while the speed changes.
              aria-label="Master tempo"
              aria-pressed={playback.masterTempo}
              data-on={playback.masterTempo ? "" : undefined}
              disabled={playback.idle}
              onClick={() => playback.setMasterTempo(!playback.masterTempo)}
            >
              MT
            </button>
            <button
              type="button"
              className={styles.chip}
              aria-label="Reset tempo"
              disabled={playback.idle || playback.tempo === 1}
              onClick={() => playback.setTempo(1)}
            >
              RST
            </button>
          </div>

          <button
            type="button"
            className={styles.chip}
            aria-label="Quantize"
            aria-pressed={quantize}
            data-on={quantize ? "" : undefined}
            onClick={() => setQuantize((on) => !on)}
          >
            Q
          </button>
          <button type="button" className={styles.padMenu} aria-label="Pad settings">≡</button>
        </div>

      </div>

      <aside className={styles.side} aria-label="Cue list">
        {panel === "info" ? (
          <div className={styles.info}>
            <div className={styles.infoBlock}>
              <span className={styles.infoStars}>
                {[1, 2, 3, 4, 5].map((star) => (
                  <span key={star}>{(track?.rating ?? 0) >= star ? "★" : "☆"}</span>
                ))}
              </span>
            </div>
            <div className={styles.infoBlock} />
            <div className={styles.infoBlock}>
              <span className={styles.infoIcon} aria-hidden>▬</span>
              <span className={styles.infoLine}>{track?.comment || "—"}</span>
            </div>
            <div className={styles.infoBlock}>
              {/*
                Only what the index actually holds. rekordbox also lists the
                file's size, sample rate and bit rate; those columns are not
                read yet, and inventing them would be worse than their absence.
              */}
              <span className={styles.infoLine}>{track ? fileKind(track.title) : "—"}</span>
              <span className={styles.infoLine}>{track ? formatBpm(track.bpmX100) : "—"} BPM</span>
              <span className={styles.infoLine}>{track?.key || "—"}</span>
              <span className={styles.infoLine}>{track ? splitTime(track.durationSec).main : "—"}</span>
            </div>
          </div>
        ) : panel === "hotCue" ? (
          /* Eight slots, always: an empty one is a slot you can fill, and
             hiding it makes the list read as a shorter track. */
          <div className={styles.cueList}>
            {PADS.map((letter) => {
              const cue = cues.find((c) => !c.memory && c.letter === letter);
              return (
                <button
                  key={letter}
                  type="button"
                  className={styles.cueRow}
                  data-empty={cue ? undefined : ""}
                  disabled={!cue}
                  onClick={() => cue && playback.seek(cue.positionMs / 1000)}
                >
                  <span className={styles.cueChip} data-set={cue ? "" : undefined}>{letter}</span>
                  {cue ? (
                    <>
                      <span className={styles.cueTime}>{splitTime(cue.positionMs / 1000).main}</span>
                      <span className={styles.cueName}>CUE(Auto)</span>
                      <span className={styles.cueDelete} aria-hidden>✕</span>
                    </>
                  ) : null}
                </button>
              );
            })}
          </div>
        ) : (
          <div className={styles.cueList}>
            {cuesFor(cues, panel).map((cue) => (
              /* A row rather than a button, because the ✕ inside it is one:
                 the row seeks, the ✕ deletes, and a button cannot hold a
                 button. Keyed by the cue's id so a deleted row leaves rather
                 than the one after it re-rendering as it. */
              <div
                key={cue.id || `m-${cue.positionMs}`}
                role="button"
                tabIndex={0}
                className={styles.cueRow}
                data-loop={cue.outMs > 0 ? "" : undefined}
                onClick={() => playback.seek(cue.positionMs / 1000)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    playback.seek(cue.positionMs / 1000);
                  }
                }}
              >
                <span className={styles.cueTime}>{memoryTime(cue.positionMs)}</span>
                {/* A loop is listed the same way as a cue. [UNKNOWN] How
                    rekordbox labels a loop row — no capture holds one, and
                    german.lang has only "CUE(Auto)" — so nothing is invented;
                    `data-loop` marks the row for when one is measured. */}
                <span className={styles.cueName}>CUE(Auto)</span>
                <button
                  type="button"
                  className={styles.cueDelete}
                  aria-label={`Delete memory cue ${memoryTime(cue.positionMs)}`}
                  title={readOnly ? READ_ONLY_REASON : "Delete Memory Cue"}
                  disabled={!memory.canEdit || cue.id === ""}
                  onClick={(event) => {
                    // The row underneath seeks; a delete is not also a jump.
                    event.stopPropagation();
                    memory.remove(cue);
                  }}
                >
                  ✕
                </button>
              </div>
            ))}
          </div>
        )}
        <div className={styles.sideTabs} role="tablist" aria-label="Cue list view">
          {PANELS.map((tab) => (
            <button
              key={tab.id}
              type="button"
              role="tab"
              aria-selected={panel === tab.id}
              className={styles.sideTab}
              data-on={panel === tab.id || undefined}
              onClick={() => setPanel(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </div>
      </aside>

      {jumpMenu ? (
        <JumpMenu
          x={jumpMenu.x}
          y={jumpMenu.y}
          current={jumpSizeId}
          anchor={jumpButton.current}
          onPick={setJumpSizeId}
          onClose={() => setJumpMenu(null)}
        />
      ) : null}
    </section>
  );
});
