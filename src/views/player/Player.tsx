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
import { CutIcon, EjectIcon, LockIcon, MetronomeIcon, RecordIcon } from "@/components/icons";
import { formatBpm } from "@/lib/format";
import {
  DETAIL_BARS,
  NO_BEATS,
  nearestBeatMs,
  subdivideGrid,
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
  beatCountText,
  clickSeconds,
  headPercent,
  isClick,
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
import { usePreferences, usePreferencesContext, useTooltip } from "@/store/usePreferences";
import { ContextMenu } from "@/components/ContextMenu";
import { deckMenu, type DeckAction } from "@/lib/contextMenus";
import { TempoField } from "./TempoField";
import type { HotCueColor } from "@/lib/preferences";
import { formatKey, quantizeFraction } from "@/lib/preferences";
import { beatNudgeFor, beatWait, syncTo, tempoFor, type Deck as SyncDeck } from "@/lib/sync";
import { actionFor, detectPlatform, dispatch, hotCuePad } from "@/lib/shortcuts";
import { WaveformDetail } from "./WaveformDetail";
import { SimplePlayer } from "./SimplePlayer";
import { JumpMenu } from "./JumpMenu";
import { VocalStrip } from "./VocalStrip";
import { useTrackCues } from "./useTrackCues";
import { useTrackDetails } from "./useTrackDetails";
import { DeckInfo } from "./DeckInfo";
import { DualControls, DualHead } from "./DualDeck";
import { READ_ONLY_REASON, useMemoryCues } from "./useMemoryCues";
import { useHotCues } from "./useHotCues";
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
  /** Analyze Track from the deck's ≡ menu: the loaded track goes to the analyser. */
  onAnalyse?: (trackId: string, title: string) => void;
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
   * The two-deck body: rekordbox's 2 PLAYER deck is a different arrangement
   * from its 1 PLAYER deck, not the same one at half height — see `DualDeck`.
   * Its own prop rather than inferred from `transportSlot`, which is null for
   * a render before the slot has mounted.
   */
  dual?: boolean;
  /**
   * The zoom, when the shell draws the cluster.
   *
   * The two-deck layout has one zoom cluster for the pair, over the line
   * where the two detail waveforms meet; pressing it zooms both decks.
   * Registered the way `publishSync` is, for the same reason.
   */
  publishZoom?: ((zoom: (by: number) => void) => void) | undefined;
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
   * BEAT SYNC held on: the deck follows the master's tempo for as long as
   * it is lit, as a CDJ's does, rather than matching once. The shell holds
   * the flag, since the master is the shell's to name.
   */
  synced?: boolean;
  onSyncToggle?: (() => void) | undefined;
  /**
   * The tempo the master is playing at, in hundredths of a BPM, or null
   * with no master track. A synced deck re-matches whenever it changes —
   * a nudge on the master, a reset, a new track.
   */
  leaderBpmX100?: number | null;
  /** What this deck is playing at, for the shell to hand to a synced deck. */
  onPlayingBpm?: ((bpmX100: number | null) => void) | undefined;
  /**
   * Load whatever the browser has selected.
   *
   * The third load gesture, and the sleeve is where it lives: loaded, the
   * sleeve ejects, and empty it takes the selection. Absent — nothing
   * selected, or several things — the empty deck is inert.
   */
  onLoadSelected?: (() => void) | undefined;
  /**
   * The id of the highlighted browser row, so Enter can load it onto Player 1
   * and — when this deck is already playing — carry the sound straight into it.
   */
  selectedTrackId?: string | null;
  /**
   * rekordbox holds the database. The MEMORY cluster and the list's ✕ are
   * drawn and disabled, with the same reason the menus give.
   */
  readOnly?: boolean;
}

/**
 * A cue's colour, as the CSS variable the marker, pad and chip styles read.
 *
 * Only set when the cue has one; the stylesheet's `var(--cue-colour,
 * var(--c-cue-hot))` falls back to the token green otherwise, so the default
 * lives in one place.
 */
export function cueStyle(
  left: string | undefined,
  colour: string | null | undefined,
  /** View › Color › HOT CUE color: CDJ draws every cue in the fallback green. */
  hotCueColor: HotCueColor = "colorful",
): React.CSSProperties {
  const style: Record<string, string> = {};
  if (left !== undefined) style.left = left;
  if (colour && hotCueColor === "colorful") style["--cue-colour"] = colour;
  return style;
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
 * The detail draws the same two things larger, measured off the user's crop of
 * a hot cue there: a 16pt red triangle pointing down from 8.25pt under the
 * band's top for the memory cue, and the 11pt badge centred on the cue 15pt
 * down for the hot cue, over the triangle's point, with a 1pt white line under
 * it through the waveform.
 *
 * A badge takes the colour rekordbox paints for the cue's `ColorTableIndex`,
 * which arrives with the cue from the nine indices measured off the captures.
 * An index outside those arrives without one and draws the token green —
 * index 21's colour, which 735,427 of the library's 850,000 hot cues carry —
 * rather than a guess at a neighbour's.
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
   * The overview hangs its badges from the top of the strip, left edge on the
   * cue. The detail centres them on the cue under the memory cue's triangle,
   * and keeps a line down through the waveform — the thing that makes a cue
   * placeable while the grid is being edited.
   */
  band?: "overview" | "detail";
  /** The slice of the track being shown, for the zoomed detail waveform. */
  window?: { from: number; to: number };
}) {
  const tip = useTooltip();
  const hotCueColor = usePreferences().view.hotCueColor;
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
        // A hot cue is its lettered badge on both waveforms; the stylesheet
        // places it by band. A memory cue's red head is the overview's small
        // one or the detail's 16pt triangle.
        const head = !cue.memory ? (
          <b className={styles.hotCueBadge}>{cue.letter}</b>
        ) : band === "detail" ? (
          <i className={styles.cueMarker} />
        ) : (
          <i className={styles.cueHead} />
        );
        return (
          <span
            key={cue.id || (cue.memory ? `m-${cue.positionMs}` : `h-${cue.letter}-${cue.positionMs}`)}
            className={cue.memory ? styles.memoryCue : styles.hotCue}
            data-band={band}
            data-cue={cue.memory ? "" : cue.letter}
            style={cueStyle(left, cue.colour, hotCueColor)}
            title={tip(cue.memory ? "Memory cue" : `Hot cue ${cue.letter}`)}
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
  phrases, totalMs, beatMs, labels = true,
}: {
  phrases: readonly Phrase[];
  totalMs: number;
  /** Milliseconds a beat lasts, for phrases the beat grid did not reach. */
  beatMs: number;
  /** "Always show types of phrases": the name in each block, or colour alone. */
  labels?: boolean;
}) {
  const spans = phraseSpans(phrases, totalMs, beatMs);
  const tip = useTooltip();
  return (
    <div className={styles.phrase} aria-label="Phrase" data-testid="player-phrase">
      {spans.map((span) => (
        <span
          key={`${span.from}-${span.label}`}
          className={styles.phraseBlock}
          data-kind={span.kind}
          style={{ left: `${span.from * 100}%`, width: `${(span.to - span.from) * 100}%` }}
          title={tip(span.label)}
        >
          {labels ? span.label : ""}
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
 * The key that sets each pad, for its tooltip: the Export preset binds `1`,
 * `2` and `3` to `Set Hot Cue A` to `C` and nothing to the rest.
 */
const HOT_CUE_KEYS: Partial<Record<(typeof PADS)[number], string>> = { A: "1", B: "2", C: "3" };

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

/**
 * How many rows the MEMORY list draws whatever it holds.
 *
 * Ten, counted off the capture (`docs/screenshots` 9.08.22 PM): four cues and
 * six empty boxes below them, each with its dimmed ✕, filling the panel. A
 * track with no memory cues shows the same ten empty boxes rather than a
 * blank panel; a track with more scrolls.
 */
const MEMORY_ROWS = 10;

/** The panel tabs beside the deck. */
const PANELS = [
  { id: "memory", label: "MEMORY" },
  { id: "hotCue", label: "HOT CUE" },
  { id: "info", label: "INFO" },
] as const;

export const Player = memo(function Player({
  track, onEject, onError, onAnalyse, onDropTrack, onLoadSelected, selectedTrackId = null,
  dragging = false, deck = "a",
  simple = false, transportSlot, flipped = false, dual = false, publishZoom,
  bars: linkedBars, onBars, jumpSize: linkedJump, onJumpSize,
  publishSync, peerSync, isMaster = false, onMaster, synced = false, onSyncToggle,
  leaderBpmX100 = null, onPlayingBpm, readOnly = false,
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
  // The INFO tab's record, fetched only while that tab is showing — see
  // `useTrackDetails` for why not on every load.
  const details = useTrackDetails(track?.id ?? null, panel === "info");
  /**
   * Quantize — the Q button at the end of the pad row. On by default, as a CDJ
   * ships: a cue set by hand lands tens of milliseconds off the beat, and every
   * loop and mix taken from it inherits that.
   */
  const [quantize, setQuantize] = useState(true);
  // The metronome: a click on every beat of the grid while the deck plays.
  // Off on every load, as rekordbox's is; the engine holds the grid.
  const [metronome, setMetronome] = useState(false);
  const toggleMetronome = useCallback(() => {
    const on = !metronome;
    setMetronome(on);
    void getBackend()
      .then((backend) => backend.deckMetronome(deck, on))
      .catch((e: unknown) => onError?.(e instanceof Error ? e.message : "The metronome could not be switched."));
  }, [metronome, deck, onError]);
  const { preferences, update: updatePreferences } = usePreferencesContext();
  const { view: viewPrefs, advanced: advancedPrefs } = preferences;
  // The ≡ menu at the foot of the deck.
  const [deckMenuAt, setDeckMenuAt] = useState<{ x: number; y: number } | null>(null);
  const chooseFromDeckMenu = useCallback(
    (action: DeckAction) => {
      switch (action) {
        case "waveformBlue": updatePreferences("view", { waveformColor: "blue" }); break;
        case "waveformRgb": updatePreferences("view", { waveformColor: "rgb" }); break;
        case "waveform3band": updatePreferences("view", { waveformColor: "3band" }); break;
        case "beatPosition": updatePreferences("view", { beatCount: "position" }); break;
        case "beatToMemoryBars": updatePreferences("view", { beatCount: "toMemoryBars" }); break;
        case "beatToMemoryBeats": updatePreferences("view", { beatCount: "toMemoryBeats" }); break;
        case "waveformClickOn": updatePreferences("view", { waveformClick: true }); break;
        case "waveformClickOff": updatePreferences("view", { waveformClick: false }); break;
        case "analyse": if (track) onAnalyse?.(track.id, track.title); break;
        default: break;
      }
    },
    [updatePreferences, track, onAnalyse],
  );
  const tip = useTooltip();
  // QUANTIZE BEAT VALUE in Preferences: the grid every quantized cue snaps
  // to, split as finely as the value asks.
  const quantizeGrid = useMemo(
    () => subdivideGrid(grid, 1 / quantizeFraction(advancedPrefs.quantizeBeat)),
    [grid, advancedPrefs.quantizeBeat],
  );
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
  // The memory cues' positions, for the count-down modes of the beat count.
  const memorySeconds = useMemo(
    () => cues.filter((cue) => cue.memory).map((cue) => cue.positionMs / 1000),
    [cues],
  );
  const beatCount = viewPrefs.beatCount;

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
        barsLabel.current.textContent = beatCountText(seconds, bpm, beatCount, memorySeconds);
      }
    };
    // At once as well as on every frame: a paused player schedules no frames,
    // and the head would otherwise sit where the last track left it.
    apply(positionRef.current);
    return subscribe(apply);
  }, [total, bpm, span, overview.width, detail.width, positionRef, subscribe, beatCount, memorySeconds]);

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
      quantize ? quantizeGrid : null,
    );
    previewing.current = action.playing;
    if (action.cuePoint !== cuePoint) setCuePoint(action.cuePoint);
    if (action.seekTo !== null) playback.seek(action.seekTo);
    if (action.playing !== playback.playing) playback.toggle();
  }, [playback, cuePoint, quantize, quantizeGrid]);

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
  const hot = useHotCues({
    trackId: playback.idle ? null : track?.id ?? null,
    cues, positionSeconds, seek, quantiseTo: quantize ? quantizeGrid : null, readOnly, onError,
  });

  // What the other deck reads when its BEAT SYNC is pressed. A ref holding a
  // closure over the current render, registered once: the shell keeps the
  // getter, not the values, so nothing here re-renders anything there.
  const syncState = useRef<() => SyncDeck | null>(() => null);
  syncState.current = () =>
    track
      ? {
          bpmX100: track.bpmX100,
          tempo: playback.tempo,
          playing: playback.playing,
          position: playback.positionNow(),
          grid,
        }
      : null;
  useEffect(() => {
    publishSync?.(() => syncState.current());
  }, [publishSync]);

  /**
   * PLAY. With BEAT SYNC lit and Q on, a stopped deck starts on the beat, as
   * a CDJ with SYNC and QUANTIZE does: it is put on its own nearest beat and
   * held until the master's next one lands, so the two are on the beat
   * together from the first sound. The wait is the engine's, counted in
   * output frames. A master that is not running has no next beat to wait
   * for, so the deck is lined up with it and started at once. A deck already
   * playing, or one following nothing, simply toggles.
   */
  const togglePlay = useCallback(() => {
    if (!playback.playing && synced && quantize) {
      const leader = peerSync?.();
      const follower = syncState.current();
      if (leader && follower) {
        const wait = leader.playing ? beatWait(leader) : null;
        if (wait !== null && grid.times.length > 0) {
          const onBeat = nearestBeatMs(grid, follower.position * 1000) / 1000;
          if (Math.abs(onBeat - follower.position) > 0.001) playback.seek(onBeat);
          playback.playAfter(wait * 1000);
          return;
        }
        const nudge = beatNudgeFor(leader, follower);
        if (Math.abs(nudge) > 0.001) playback.seek(follower.position + nudge);
      }
    }
    playback.toggle();
  }, [playback, synced, quantize, peerSync, grid]);

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
        // Left/Right always beat-jump Player 1, wherever the focus is; the
        // other decks leave the arrows to the browser.
        if (deck !== "a") return;
        event.preventDefault();
        jump(action === "jumpForward" ? 1 : -1);
        return;
      }
      if (action === "loadPlayer1") {
        // Enter loads the highlighted track onto Player 1. A deck already
        // playing carries the sound into the new track; a stopped one cues it.
        if (deck !== "a" || !onLoadSelected || selectedTrackId === null) return;
        event.preventDefault();
        if (playback.playing) playback.playWhenLoaded(selectedTrackId);
        onLoadSelected();
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
          togglePlay();
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
        default: {
          // `1`-`3` are the first three pads and `command + 1`-`3` their
          // clears; a repeat on a held key is one press, as with M and X.
          const pad = hotCuePad(action);
          if (!pad || event.repeat) break;
          // Command with a digit is a tab switch in a browser; not here.
          event.preventDefault();
          if (pad.clear) hot.clear(pad.letter);
          else hot.press(pad.letter);
          break;
        }
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
  }, [deck, jump, platform, playback, onLoadSelected, selectedTrackId, holdCue, dropCue, memory, hot, togglePlay]);

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
  const grab = useRef<{ x: number; y: number; at: number } | null>(null);

  const startDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    grab.current = { x: event.clientX, y: event.clientY, at: playback.positionRef.current };
    playback.scrubBegin();
  };

  /**
   * A press let go where it landed: View › Display Type › Click on the
   * waveform for PLAY and CUE. The head goes to the music under the pointer;
   * a stopped deck takes that as its cue point too and plays, the way a
   * CUE press followed by PLAY would. Off, a click does nothing.
   */
  const clickDetail = (event: React.PointerEvent<HTMLDivElement>, held: { x: number; at: number }) => {
    if (!viewPrefs.waveformClick || playback.idle || total <= 0) return;
    const box = event.currentTarget.getBoundingClientRect();
    if (box.width <= 0) return;
    const target = clickSeconds(event.clientX - box.left, box.width, held.at, span, total);
    playback.seek(target);
    if (!playback.playing) {
      setCuePoint(target);
      playback.toggle();
    }
  };

  const dragDetail = (event: React.PointerEvent<HTMLDivElement>) => {
    const held = grab.current;
    if (!held || !event.currentTarget.hasPointerCapture(event.pointerId)) return;
    const box = event.currentTarget.getBoundingClientRect();
    playback.scrubTo(held.at + dragSeconds(event.clientX - held.x, box.width, span, total));
  };

  const endDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    const held = grab.current;
    grab.current = null;
    playback.scrubEnd();
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    if (held && event.type === "pointerup" && isClick(event.clientX - held.x, event.clientY - held.y)) {
      clickDetail(event, held);
    }
  };

  // A beat's length, for phrases whose time the grid did not resolve.
  const beatMs = (track?.bpmX100 ?? 0) > 0 ? 6_000_000 / (track?.bpmX100 ?? 1) : 0;
  const remaining = splitTime(Math.max(total - playback.position, 0));
  const elapsed = splitTime(playback.position);

  useEffect(() => {
    publishZoom?.(zoom);
  }, [publishZoom, zoom]);

  // What this deck is playing at, for a synced deck to follow: the file's
  // tempo times the deck's. Reported when either changes and nothing else.
  const playingBpmX100 = track && track.bpmX100 > 0 ? Math.round(track.bpmX100 * playback.tempo) : null;
  useEffect(() => {
    onPlayingBpm?.(playingBpmX100);
  }, [onPlayingBpm, playingBpmX100]);

  /** Match this deck to the other one: its tempo, then its bar. */
  const matchLeader = useCallback(() => {
    const leader = peerSync?.();
    const follower = syncState.current();
    if (!leader || !follower) return;
    // BEAT/BPM SYNC in Preferences: whether the bar is matched as well as
    // the tempo, and whether a double or half BPM counts as the same tempo.
    const { tempo, nudge } = syncTo(leader, follower, {
      type: advancedPrefs.syncType,
      doubleHalf: advancedPrefs.syncDoubleHalf,
    });
    playback.setTempo(tempo);
    // The nudge second: it is measured against where the follower is now, and
    // the tempo does not move the playhead.
    if (Math.abs(nudge) > 0.001) playback.seek(follower.position + nudge);
  }, [peerSync, playback, advancedPrefs.syncType, advancedPrefs.syncDoubleHalf]);

  /**
   * BEAT SYNC: lights and matches, or goes out. Lit, the deck keeps the
   * master's tempo — the effect below re-matches on every change to it — and
   * the bar is matched once, now, as a CDJ does on the press.
   */
  const beatSync = useCallback(() => {
    if (!onSyncToggle) {
      matchLeader();
      return;
    }
    if (!synced) matchLeader();
    onSyncToggle();
  }, [onSyncToggle, synced, matchLeader]);

  // Following: the tempo alone, with the bar left where the press put it.
  // The master's BPM arrives from the shell rather than being read through
  // the getter, so this runs exactly when that BPM changes and never on a
  // frame.
  const fileBpmX100 = track?.bpmX100 ?? 0;
  useEffect(() => {
    if (!synced || leaderBpmX100 === null || leaderBpmX100 <= 0 || fileBpmX100 <= 0) return;
    const tempo = tempoFor(
      { bpmX100: leaderBpmX100, position: 0, grid: NO_BEATS },
      { bpmX100: fileBpmX100, position: 0, grid: NO_BEATS },
      { doubleHalf: advancedPrefs.syncDoubleHalf },
    );
    if (Math.abs(tempo - playback.tempo) > 1e-4) playback.setTempo(tempo);
  }, [synced, leaderBpmX100, fileBpmX100, advancedPrefs.syncDoubleHalf, playback]);

  /** RST: the file's own speed, and no longer following anything. */
  const resetTempo = useCallback(() => {
    if (synced) onSyncToggle?.();
    playback.setTempo(1);
  }, [synced, onSyncToggle, playback]);

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
        onToggle={togglePlay}
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
          title={tip("Select the beat/bar length jumping from the current position.")}
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
          onClick={togglePlay}
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

  // The sleeve: the eject button loaded and the load button empty, and the
  // record where there is no artwork. Both bodies draw it, at their own size.
  const sleeve = (
          <button
            type="button"
            className={styles.artwork}
            // Loaded, the sleeve ejects — as it does on a CDJ's screen. Empty,
            // it takes whatever the browser has selected, which is the third
            // way a track reaches a deck alongside the drop and the menu.
            aria-label={track ? "Eject" : "Load the selected track"}
            title={tip(track ? "Eject" : "Load the selected track")}
            onClick={track ? onEject : onLoadSelected}
            disabled={track ? !onEject : !onLoadSelected}
          >
            {/* The record underneath, the way the track list draws a row
                without artwork: the same asset, so one record looks the same
                everywhere. */}
            <RecordIcon className={styles.disc} aria-hidden />
            {track?.hasArtwork ? (
              <Artwork trackId={track.id} className={styles.sleeve} />
            ) : null}
            {/* Shown on hover, over a scrim: what the sleeve does when clicked
                is not otherwise guessable from a sleeve. */}
            {track ? <EjectIcon className={styles.eject} /> : null}
          </button>
  );

  const overviewStack = (
          <div className={styles.overviewStack}>
            {/* Where the vocals are, from the analysis; Vocal (Full
                Waveform) in Preferences turns the strip off. */}
            {viewPrefs.vocalFull ? (
              <VocalStrip trackId={track && track.analysed ? track.id : null} />
            ) : null}
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
                  // Full/Preview Waveform in Preferences: single-sided from
                  // the baseline, or mirrored about the middle.
                  half={viewPrefs.overviewWaveform === "half"}
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
      // The two-deck body, whose rows are DualDeck's.
      data-dual={dual || undefined}
      data-droppable={takesDrop || undefined}
      onDragOver={dragOver}
      onDrop={drop}
    >
      {transportSlot ? createPortal(transport, transportSlot) : transport}

      <div className={styles.main}>
        {dual ? (
          <DualHead
            track={track}
            remaining={remaining}
            elapsed={elapsed}
            sleeve={sleeve}
            onBeatSync={beatSync}
            synced={synced}
            isMaster={isMaster}
            onMaster={onMaster}
          />
        ) : (
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
              <span className={styles.readout}>{formatKey(track.key, viewPrefs.keyDisplay)}</span>
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
                aria-pressed={synced}
                data-on={synced ? "" : undefined}
                disabled={!track || isMaster}
                title={tip(
                  isMaster
                    ? "This deck is the master; sync the other one to it."
                    : synced
                      ? "Following the master's tempo; press to stop."
                      : "Match this deck to the master's tempo and bar, and keep its tempo.",
                )}
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
        )}

        {/* The one-deck overview sits beside the sleeve; the two-deck one
            runs the deck's width, its sleeve up in the title row. */}
        <div className={styles.overviewRow}>
          {dual ? null : sleeve}
          {overviewStack}
        </div>

        {viewPrefs.phraseFull ? (
          <PhraseBar
            phrases={phrases}
            totalMs={total * 1000}
            beatMs={beatMs}
            labels={viewPrefs.phraseLabels}
          />
        ) : null}

        {/* The control row comes before the detail in the two-deck body, as
            the capture has it: the detail is the last row, and takes what is
            left. */}
        {dual ? (
          <DualControls
            idle={playback.idle}
            readOnly={readOnly}
            memory={memory}
            trackBpmX100={track?.bpmX100 ?? 0}
            tempo={playback.tempo}
            onTempo={playback.setTempo}
            keyShift={playback.keyShift}
            shiftsKey={playback.shiftsKey}
            onKeyShift={playback.setKeyShift}
            onNudgeTempo={playback.nudgeTempo}
            synced={synced}
            masterTempo={playback.masterTempo}
            onMasterTempo={playback.setMasterTempo}
            atUnity={playback.tempo === 1 && !synced}
            onResetTempo={resetTempo}
            quantize={quantize}
            onQuantize={() => setQuantize((on) => !on)}
          />
        ) : null}

        <div className={styles.detailRow}>
          {/* The two-deck layout's zoom is the shell's, one for the pair. */}
          {dual ? null : (
          <div className={styles.zoom}>
            <button type="button" aria-label="Zoom in" onClick={() => zoom(-1)}>+</button>
            <span className={styles.rst} aria-hidden>RST</span>
            <button type="button" aria-label="Zoom out" onClick={() => zoom(1)}>−</button>
          </div>
          )}
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
                  // In the 2 PLAYER layout the two details are halves that
                  // meet at the line between the decks: deck A's rises from
                  // it and deck B's, whose canvas is flipped, hangs from it
                  // [OBS]. On its own the deck draws the centred waveform.
                  half={dual ? "overlaid" : false}
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

        {dual ? null : (
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
                          // The metronome is live: it plays the grid rather
                          // than changing it. Saving the rest needs the PQT2
                          // tag, whose payload is not understood; an editor
                          // that cannot save is a trap.
                          disabled={edit.id !== "metronome" || playback.idle}
                          aria-pressed={edit.id === "metronome" ? metronome : undefined}
                          onClick={edit.id === "metronome" ? toggleMetronome : undefined}
                          title={tip(
                            edit.id === "metronome"
                              ? "Metronome: a click on every beat while the deck plays"
                              : "Grid editing needs the PQT2 tag, which is not yet understood",
                          )}
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
            {/* A set pad calls its cue; an empty one sets `Hot Cue <letter>`
                at the playhead — `Set Hot Cue A` in german.lang, on `1`-`3`
                for the first three pads. An empty pad that cannot be set is
                disabled with the reason the MEMORY cluster gives. */}
            <div className={styles.hotCues} aria-label="Hot cues">
              {PADS.map((letter) => {
                const cue = hot.at(letter);
                const key = HOT_CUE_KEYS[letter];
                return (
                  <button
                    key={letter}
                    type="button"
                    className={styles.pad}
                    data-set={cue ? "" : undefined}
                    style={cueStyle(undefined, cue?.colour, viewPrefs.hotCueColor)}
                    aria-label={`Hot cue ${letter}`}
                    aria-pressed={cue !== null}
                    title={cue
                      ? undefined
                      : tip(readOnly ? READ_ONLY_REASON : `Set Hot Cue ${letter}${key ? ` (${key})` : ""}`)}
                    disabled={!cue && !hot.canEdit}
                    onClick={() => hot.press(letter)}
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
                title={tip(readOnly ? READ_ONLY_REASON : "Set Memory Cue (M)")}
                disabled={!memory.canEdit}
                onClick={memory.store}
              >
                MEMORY
              </button>
              <button
                type="button"
                className={styles.step}
                aria-label="Previous memory cue"
                title={tip("Call Previous Memory Cue (B)")}
                disabled={playback.idle}
                onClick={memory.callPrevious}
              >
                ◀
              </button>
              <button
                type="button"
                className={styles.step}
                aria-label="Next memory cue"
                title={tip("Call Next Memory Cue (N)")}
                disabled={playback.idle}
                onClick={memory.callNext}
              >
                ▶
              </button>
              <button
                type="button"
                className={styles.step}
                aria-label="Delete memory cue"
                title={tip(readOnly ? READ_ONLY_REASON : "Delete Memory Cue (X)")}
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

            {/* Not a pad page. The capture's `‹ 2 ›` sits beside AU/MA,
                which german.lang describes as "Change Auto Beat Loop/Manual
                Loop display", and the arrows as "Switch the page of beat
                length": it is the auto beat loop's length, in beats. The
                pads are A to H whatever it reads. [ASSUME] Loops are not
                built, so it is drawn and inert. */}
            <div className={styles.page} aria-label="Beat loop length">
              <button type="button" className={styles.step} aria-label="Shorter loop" disabled>‹</button>
              <span className={styles.pageNumber}>2</span>
              <button type="button" className={styles.step} aria-label="Longer loop" disabled>›</button>
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
              disabled={playback.idle || synced}
              title={tip(synced ? "The tempo is the master's while BEAT SYNC is on." : undefined)}
              onClick={() => playback.nudgeTempo(-1)}
            >
              −
            </button>
            <TempoField
              trackBpmX100={track?.bpmX100 ?? 0}
              tempo={playback.tempo}
              onTempo={playback.setTempo}
              keyShift={playback.keyShift}
              shiftsKey={playback.shiftsKey}
              onKeyShift={playback.setKeyShift}
              disabled={playback.idle || synced}
              disabledBecause={synced ? "The tempo is the master's while BEAT SYNC is on." : undefined}
              fieldClassName={styles.bpmField}
            />
            <button
              type="button"
              className={styles.step}
              aria-label="Faster"
              disabled={playback.idle || synced}
              title={tip(synced ? "The tempo is the master's while BEAT SYNC is on." : undefined)}
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
              disabled={playback.idle || (playback.tempo === 1 && !synced)}
              onClick={resetTempo}
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
          <button
            type="button"
            className={styles.padMenu}
            aria-label="Player menu"
            aria-haspopup="menu"
            aria-expanded={deckMenuAt !== null}
            onClick={(event) => {
              // Opened from the button's corner, as rekordbox's is.
              const box = event.currentTarget.getBoundingClientRect();
              setDeckMenuAt({ x: box.left, y: box.bottom });
            }}
          >
            ≡
          </button>
        </div>
        )}
        {deckMenuAt ? (
          <ContextMenu
            x={deckMenuAt.x}
            y={deckMenuAt.y}
            rows={deckMenu({
              waveformColor: viewPrefs.waveformColor,
              beatCount: viewPrefs.beatCount,
              waveformClick: viewPrefs.waveformClick,
            })}
            label="Player menu"
            context={{ inPlaylist: false, hasFile: true, readOnly: false }}
            onChoose={chooseFromDeckMenu}
            onClose={() => setDeckMenuAt(null)}
          />
        ) : null}

      </div>

      <aside className={styles.side} aria-label="Cue list">
        {panel === "info" ? (
          <DeckInfo track={track} details={details} />
        ) : panel === "hotCue" ? (
          /* Eight slots, always: an empty one is a slot you can fill, and
             hiding it makes the list read as a shorter track. A row rather
             than a button, as the memory list's are, because the ✕ inside a
             set row is one: the row calls the cue, the ✕ clears it. */
          <div className={styles.cueList}>
            {PADS.map((letter) => {
              const cue = hot.at(letter);
              return (
                <div
                  key={letter}
                  role="button"
                  tabIndex={cue ? 0 : -1}
                  className={styles.cueRow}
                  aria-label={`Hot cue ${letter}`}
                  aria-disabled={cue ? undefined : true}
                  data-empty={cue ? undefined : ""}
                  onClick={() => hot.press(letter)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      hot.press(letter);
                    }
                  }}
                >
                  <span
                    className={styles.cueChip}
                    data-set={cue ? "" : undefined}
                    style={cueStyle(undefined, cue?.colour, viewPrefs.hotCueColor)}
                  >
                    {letter}
                  </span>
                  {cue ? (
                    <>
                      <span className={styles.cueTime}>{splitTime(cue.positionMs / 1000).main}</span>
                      <span className={styles.cueName}>CUE(Auto)</span>
                      <button
                        type="button"
                        className={styles.cueDelete}
                        aria-label={`Clear hot cue ${letter}`}
                        title={readOnly ? READ_ONLY_REASON : `Clear Hot Cue ${letter}`}
                        disabled={!hot.canEdit || cue.id === ""}
                        onClick={(event) => {
                          // The row underneath calls the cue; a clear is not
                          // also a jump.
                          event.stopPropagation();
                          hot.clear(letter);
                        }}
                      >
                        ✕
                      </button>
                    </>
                  ) : null}
                </div>
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
                  title={tip(readOnly ? READ_ONLY_REASON : "Delete Memory Cue")}
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
            {/* The boxes below the cues, so the panel is the same grid whether
                the track has ten memory cues or none. Nothing to press: the
                ✕ is drawn dimmed, as the capture draws it, and is not a
                control. */}
            {Array.from({ length: Math.max(0, MEMORY_ROWS - cuesFor(cues, panel).length) }, (_, i) => (
              <div key={`empty-${i}`} className={styles.cueRow} data-blank="" aria-hidden>
                <span className={styles.cueDelete}>✕</span>
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
