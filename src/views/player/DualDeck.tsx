/**
 * What a deck draws in the two-deck layouts and nowhere else.
 *
 * rekordbox's 2 PLAYER deck is not the 1 PLAYER deck at half height. Its title
 * row carries the sleeve, the sync buttons and the readouts together; its
 * overview runs the deck's full width; and it has no pad row at all — a single
 * control row of grid, MEMORY, loop and tempo buttons sits between the phrase
 * bar and a detail waveform that takes everything left. Every size here is a
 * `playerDual*` token scanned off the 2 PLAYER capture with both decks loaded
 * (`docs/screenshots`, Screenshot 2026-09-08 at 3.00.12 PM).
 *
 * `Player` still owns the deck: the engine, the cues, the frame loop and the
 * keys. These are the rows it draws when it is told `dual`, given the values
 * it already holds.
 */
import { memo, type ReactNode } from "react";

import type { RowDto } from "@/ipc/types";
import { formatBpm } from "@/lib/format";
import { LoopInIcon, LoopOutIcon, MagnifierMinusIcon, MagnifierPlusIcon } from "@/components/icons";
import { READ_ONLY_REASON } from "./useMemoryCues";
import styles from "./DualDeck.module.css";
import { useTooltip } from "@/store/usePreferences";
import { TempoField } from "./TempoField";

/** A time split the way `splitTime` returns it. */
interface Split {
  main: string;
  tenths: string;
}

export interface DualHeadProps {
  track: RowDto | null;
  remaining: Split;
  elapsed: Split;
  /** The sleeve button, which `Player` builds because it owns load and eject. */
  sleeve: ReactNode;
  keyControl: ReactNode;
  bpmX100: number;
  /** BEAT SYNC: pull this deck to the master. Disabled while this deck is it. */
  onBeatSync: () => void;
  /** BEAT SYNC is lit: the deck is following the master's tempo. */
  synced: boolean;
  isMaster: boolean;
  onMaster: (() => void) | undefined;
}

/**
 * The title row: sleeve, title over artist, and at the right two rows of
 * controls — KEY SYNC and BEAT SYNC over the readouts, the key shift and
 * MASTER.
 *
 * Deck B draws the same row at the bottom of its panel; the stylesheet moves
 * it there, and the row itself is the same either way up.
 */
export const DualHead = memo(function DualHead({
  track, remaining, elapsed, sleeve, keyControl, bpmX100, onBeatSync, synced, isMaster, onMaster,
}: DualHeadProps) {
  const tip = useTooltip();
  return (
    <div className={styles.head} data-testid="player-head-row">
      {sleeve}
      <div className={styles.text}>
        <span className={styles.title} data-testid="player-title">
          {track ? track.title : ""}
        </span>
        <span className={styles.artist} data-testid="player-artist">
          {track ? track.artist : ""}
        </span>
      </div>
      <div className={styles.right}>
        {/* Rekordbox's readouts, hairline-separated: the remaining and
            elapsed times share a cell, then the key, then the BPM. Drawn
            only with a track, as the one-deck title row does. */}
        <div className={styles.readouts}>
          {track ? (
            <>
              <span className={styles.cell}>
                <span className={styles.remaining} data-testid="player-time">
                  -{remaining.main}
                  <i className={styles.tenths}>.{remaining.tenths}</i>
                </span>
                <span className={styles.elapsed}>
                  {elapsed.main}
                  <i className={styles.tenths}>.{elapsed.tenths}</i>
                </span>
              </span>
              <span className={styles.cell}>{formatBpm(bpmX100)}</span>
            </>
          ) : null}
        </div>
        {/* KEY SYNC shifts the key to the master's — "Enable/Disable Key
            Sync." in german.lang. There is no key shifting in the engine, so
            it is drawn and inert, with the reason. The capture lights deck A's
            BEAT SYNC blue: in rekordbox it is a toggle that keeps the deck
            following the master. Here it is a press that matches the deck
            once, so it is never lit — a lit toggle that is not one would lie. */}
        <button
          type="button"
          className={styles.syncButton}
          aria-label="Key sync"
          disabled
          title={tip("Key sync needs a key shifter, which the engine does not have yet.")}
        >
          KEY SYNC
        </button>
        <button
          type="button"
          className={styles.syncButton}
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
          onClick={onBeatSync}
        >
          BEAT SYNC
        </button>
        {keyControl}
        <button
          type="button"
          className={styles.masterButton}
          aria-label="Sync master"
          aria-pressed={isMaster}
          data-on={isMaster ? "" : undefined}
          onClick={onMaster}
        >
          MASTER
        </button>
      </div>
    </div>
  );
});

export interface DualControlsProps {
  showTempoButtons: boolean;
  /** Nothing loaded: the transport-shaped buttons are inert. */
  idle: boolean;
  readOnly: boolean;
  /** MEMORY, the one memory-cue control the row draws; the rest stay on keys. */
  memory: {
    canEdit: boolean;
    store: () => void;
  };
  /** The BPM the deck is playing at, already formatted. */
  /** The BPM field: the track's own BPM, the tempo, and the key shift. */
  trackBpmX100: number;
  tempo: number;
  onTempo: (tempo: number) => void;
  onNudgeTempo: (direction: number) => void;
  /** Following the master: the tempo is not this deck's to step. */
  synced: boolean;
  masterTempo: boolean;
  onMasterTempo: (on: boolean) => void;
  /** RST is inert once the tempo is back at 1. */
  atUnity: boolean;
  onResetTempo: () => void;
  quantize: boolean;
  onQuantize: () => void;
}

/**
 * The control row, which stands in for the pad row of the one-deck layout.
 *
 * Left to right, as the capture has it: the three grid-shift buttons, MEMORY,
 * AU | MA, the loop length with a step either side, the tempo with its own,
 * the two loop buttons; then at the right, MT, RST and Q. The grid buttons
 * and the loops are drawn and inert — grid editing needs the PQT2 tag and
 * loops are not built — with the reason on each.
 */
export const DualControls = memo(function DualControls({
  showTempoButtons, idle, readOnly, memory, trackBpmX100, tempo, onTempo,
  onNudgeTempo, synced, masterTempo, onMasterTempo,
  atUnity, onResetTempo, quantize, onQuantize,
}: DualControlsProps) {
  const gridReason = "Grid editing needs the PQT2 tag, which is not yet understood";
  const tip = useTooltip();
  return (
    <div className={styles.controls} role="group" aria-label="Deck controls" data-testid="player-controls">
      <div className={styles.group}>
        <button type="button" className={styles.icon} aria-label="Shift the grid earlier" disabled title={tip(gridReason)}>
          <span className={styles.gridGlyph} data-dir="back" aria-hidden />
        </button>
        <button type="button" className={styles.mark} aria-label="Mark the downbeat here" disabled title={tip(gridReason)}>
          <span className={styles.markGlyph} aria-hidden />
        </button>
        <button type="button" className={styles.icon} aria-label="Shift the grid later" disabled title={tip(gridReason)}>
          <span className={styles.gridGlyph} data-dir="forward" aria-hidden />
        </button>
      </div>

      {/* MEMORY stores the cue point: `Set Memory Cue` in german.lang, on M.
          The capture draws MEMORY alone — no ◀ ▶ ✕ beside it — so calling and
          deleting stay on their keys, B, N and X. */}
      <button
        type="button"
        className={styles.memory}
        aria-label="Set memory cue"
        title={tip(readOnly ? READ_ONLY_REASON : "Set Memory Cue (M)")}
        disabled={!memory.canEdit}
        onClick={memory.store}
      >
        MEMORY
      </button>

      <div className={styles.group} role="group" aria-label="Cue mode">
        <button type="button" className={styles.chip} data-on aria-pressed>AU</button>
        <button type="button" className={styles.chip} aria-pressed={false}>MA</button>
      </div>

      {/* The auto beat loop's length in beats — "Switch the page of beat
          length" in german.lang. Loops are not built, so it is inert. */}
      <div className={styles.loopLength} aria-label="Beat loop length">
        <button type="button" className={styles.step} aria-label="Shorter loop" disabled>‹</button>
        <span className={styles.loopField}>2</span>
        <button type="button" className={styles.step} aria-label="Longer loop" disabled>›</button>
      </div>

      <div className={styles.group} role="group" aria-label="Tempo">
        <button
          type="button"
          className={styles.tempoStep}
          aria-label="Slower"
          disabled={idle || synced}
          title={tip(synced ? "The tempo is the master's while BEAT SYNC is on." : undefined)}
          onClick={() => onNudgeTempo(-1)}
        >
          −
        </button>
        <TempoField
          trackBpmX100={trackBpmX100}
          tempo={tempo}
          onTempo={onTempo}
          disabled={idle || synced}
          disabledBecause={synced ? "The tempo is the master's while BEAT SYNC is on." : undefined}
          fieldClassName={styles.bpmField}
        />
        <button
          type="button"
          className={styles.tempoStep}
          aria-label="Faster"
          disabled={idle || synced}
          title={tip(synced ? "The tempo is the master's while BEAT SYNC is on." : undefined)}
          onClick={() => onNudgeTempo(1)}
        >
          +
        </button>
      </div>

      {/* Loop In and Loop Out — german.lang's names. Not built. */}
      <div className={styles.loops} role="group" aria-label="Loop">
        <button type="button" className={styles.icon} aria-label="Loop in" disabled title={tip("Loops are not built yet.")}>
          <LoopInIcon className={styles.loopGlyph} />
        </button>
        <button type="button" className={styles.icon} aria-label="Loop out" disabled title={tip("Loops are not built yet.")}>
          <LoopOutIcon className={styles.loopGlyph} />
        </button>
      </div>

      <span className={styles.spacer} />

      {showTempoButtons ? <div className={styles.group}>
        <button
          type="button"
          className={styles.tempoButton}
          aria-label="Master tempo"
          aria-pressed={masterTempo}
          data-on={masterTempo ? "" : undefined}
          disabled={idle}
          onClick={() => onMasterTempo(!masterTempo)}
        >
          MT
        </button>
        <button
          type="button"
          className={styles.tempoButton}
          aria-label="Reset tempo"
          disabled={idle || atUnity}
          onClick={onResetTempo}
        >
          RST
        </button>
      </div> : null}

      <button
        type="button"
        className={styles.chip}
        aria-label="Quantize"
        aria-pressed={quantize}
        data-on={quantize ? "" : undefined}
        onClick={onQuantize}
      >
        Q
      </button>
    </div>
  );
});

/**
 * The zoom cluster the pair shares: + over the centre line, − under it, RST
 * between. The capture draws one for both decks, floating over the two
 * detail waveforms where they meet, so it belongs to the shell rather than to
 * either deck; pressing it zooms both.
 */
export function DualZoom({ onZoom }: { onZoom: (by: number) => void }) {
  return (
    <div className={styles.zoom} role="group" aria-label="Waveform zoom">
      <button type="button" className={styles.zoomButton} aria-label="Zoom in" onClick={() => onZoom(-1)}>
        <MagnifierPlusIcon className={styles.zoomGlyph} />
      </button>
      <span className={styles.zoomReset} aria-hidden>RST</span>
      <button type="button" className={styles.zoomButton} aria-label="Zoom out" onClick={() => onZoom(1)}>
        <MagnifierMinusIcon className={styles.zoomGlyph} />
      </button>
    </div>
  );
}
