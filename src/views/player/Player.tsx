/**
 * The preview player.
 *
 * Every size here is a token measured off a 2x capture of rekordbox 7.2.11
 * running: the player spans y 56..335pt, its transport column is 80pt wide and
 * its memory panel 209pt, and the five bands inside it (title, overview,
 * phrase, detail, pads) have their measured heights recorded as token sources.
 *
 * Playback is an `<audio>` element over the `rbl://` scheme rather than an
 * audio stack in Rust — see `usePlayback`. Outside Tauri there is no such
 * scheme, so the transport is drawn and disabled: a player waiting for a
 * backend, rather than an unfinished panel. Controls with nothing behind them
 * yet are drawn the same way, for the same reason.
 */
import { memo, useCallback, useEffect, useState } from "react";

import type { Beat, Cue, Phrase, RowDto } from "@/ipc/types";
import { getBackend } from "@/ipc/client";
import { useElementSize } from "@/store/useElementSize";
import { artworkUrl } from "@/ipc/artwork";
import { formatBpm } from "@/lib/format";
import {
  DETAIL_BARS,
  ZOOM_STEPS,
  cuesFor,
  detailSpan,
  headPercent,
  phraseSpans,
  splitTime,
  windowAround,
  type CuePanel,
  type PadMode,
} from "@/lib/player";
import { usePlayback } from "@/store/usePlayback";
import { WaveformDetail } from "./WaveformDetail";
import { VocalStrip } from "./VocalStrip";
import styles from "./Player.module.css";

export interface PlayerProps {
  /** The row the browser has selected, or `null` when nothing is. */
  track: RowDto | null;
}

/**
 * Cue points on a waveform.
 *
 * No colour: `djmdCue.ColorTableIndex` decides what rekordbox draws and is not
 * understood, so hot cues take the accent and memory cues the dim text colour
 * rather than a guessed palette. Hot cues carry their letter where there is
 * room for it.
 */
const CueMarkers = memo(function CueMarkers({
  cues, totalMs, labelled, window,
}: {
  cues: readonly Cue[];
  totalMs: number;
  labelled?: boolean;
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
        return (
          <span
            key={`${cue.memory ? "m" : cue.letter}-${cue.positionMs}`}
            className={cue.memory ? styles.memoryCue : styles.hotCue}
            style={{ left: `${((at - from) / span) * 100}%` }}
            title={cue.memory ? "Memory cue" : `Hot cue ${cue.letter}`}
            aria-hidden
          >
            {labelled && !cue.memory ? cue.letter : null}
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
  beats, totalMs, window,
}: {
  beats: readonly Beat[];
  totalMs: number;
  window: { from: number; to: number };
}) {
  if (totalMs <= 0 || beats.length === 0) return null;
  const span = Math.max(window.to - window.from, 1e-6);
  return (
    <>
      {beats.map((beat) => {
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

/** Hot cue slots, as the pad row lays them out. */
const PADS = ["A", "B", "C", "D", "E", "F", "G", "H"] as const;

/**
 * The grid-editing controls, which is what GRID puts in the pad row.
 *
 * These are the four edits `rbl-anlz::grid` implements, plus a reset. The set
 * is right; **the arrangement is ours**, because no capture of rekordbox's
 * GRID row has been taken — recorded in TODO.md.
 *
 * They are drawn and inert. Saving a grid edit needs the `PQT2` tag, whose
 * per-beat payload is not understood, and a grid editor that cannot save is a
 * trap rather than a feature.
 */
const GRID_EDITS = [
  { id: "nudge-back", label: "Nudge grid earlier", text: "◀ NUDGE" },
  { id: "nudge-forward", label: "Nudge grid later", text: "NUDGE ▶" },
  { id: "halve", label: "Halve the tempo", text: "÷2" },
  { id: "double", label: "Double the tempo", text: "×2" },
  { id: "downbeat", label: "Set the downbeat here", text: "DOWNBEAT" },
  { id: "reset", label: "Reset the grid", text: "RESET" },
] as const;

/** The panel tabs beside the deck. */
const PANELS = [
  { id: "memory", label: "MEMORY" },
  { id: "hotCue", label: "HOT CUE" },
  { id: "info", label: "INFO" },
] as const;

export const Player = memo(function Player({ track }: PlayerProps) {
  const playback = usePlayback(track?.id ?? null);
  // The waveforms follow their containers, which change with the window and
  // with the tree splitter — a fixed-width canvas stretched by CSS is blurry
  // on a wide window and wasted resolution on a narrow one.
  const [overviewRef, overview] = useElementSize<HTMLDivElement>();
  const [detailRef, detail] = useElementSize<HTMLDivElement>();
  const [cues, setCues] = useState<Cue[]>([]);
  const [beats, setBeats] = useState<Beat[]>([]);
  const [phrases, setPhrases] = useState<Phrase[]>([]);
  const [bars, setBars] = useState<number>(DETAIL_BARS);
  const [padMode, setPadMode] = useState<PadMode>("cue");
  const [panel, setPanel] = useState<CuePanel>("memory");

  useEffect(() => {
    if (!track) {
      setCues([]);
      setPhrases([]);
      return;
    }
    let live = true;
    void (async () => {
      const backend = await getBackend();
      const [foundCues, foundPhrases] = await Promise.all([
        backend.trackCues(track.id),
        backend.trackPhrases(track.id),
      ]);
      // The track may have changed while these were in flight.
      if (!live) return;
      setCues(foundCues);
      setPhrases(foundPhrases);
    })();
    return () => {
      live = false;
    };
  }, [track]);

  // The fraction played, for the playhead. Falls back to the track's own
  // length before the file's metadata has loaded, so the head does not jump.
  const total = playback.duration || track?.durationSec || 0;
  const progress = total > 0 ? Math.min(playback.position / total, 1) : 0;

  // Bars rather than a fraction: twelve bars is twelve bars whether the track
  // is three minutes or ninety.
  const span = detailSpan(bars, track?.bpmX100 ?? 0, total);
  const window = windowAround(progress, span);

  // The grid for the detail window only, refetched as the window moves. A
  // whole track's beats would blow the IPC cap on a long mix. Quantised, so
  // the refetch happens as the window moves on rather than on every
  // `timeupdate` — of which a four-minute track fires several hundred.
  const fromStep = Math.round(window.from * 200);
  const toStep = Math.round(window.to * 200);

  useEffect(() => {
    if (!track || !track.analysed || total <= 0) {
      setBeats([]);
      return;
    }
    let live = true;
    void (async () => {
      const backend = await getBackend();
      const found = await backend.trackBeats(
        track.id,
        Math.floor((fromStep / 200) * total * 1000),
        Math.ceil((toStep / 200) * total * 1000),
      );
      if (live) setBeats(found);
    })();
    return () => {
      live = false;
    };
  }, [track, total, fromStep, toStep]);

  const zoom = useCallback((by: number) => {
    setBars((current) => {
      const at = ZOOM_STEPS.indexOf(current as (typeof ZOOM_STEPS)[number]);
      // An unrecognised value snaps back to the default rather than sticking.
      const from = at === -1 ? ZOOM_STEPS.indexOf(DETAIL_BARS) : at;
      const to = Math.min(Math.max(from + by, 0), ZOOM_STEPS.length - 1);
      return ZOOM_STEPS[to] ?? DETAIL_BARS;
    });
  }, []);

  const scrub = (event: React.MouseEvent<HTMLDivElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    if (box.width <= 0) return;
    playback.seekFraction((event.clientX - box.left) / box.width);
  };

  // A beat's length, for phrases whose time the grid did not resolve.
  const beatMs = (track?.bpmX100 ?? 0) > 0 ? 6_000_000 / (track?.bpmX100 ?? 1) : 0;
  const remaining = splitTime(Math.max(total - playback.position, 0));
  const elapsed = splitTime(playback.position);

  return (
    <section className={styles.player} aria-label="Preview player">
      <div className={styles.transport}>
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
        <div className={styles.pair}>
          {JUMPS.map((button) => (
            <button
              key={button.id}
              type="button"
              className={styles.square}
              aria-label={button.label}
              disabled={playback.idle}
              onClick={() => {
                // A beat jump is the beat length times the chosen count.
                const bpm = (track?.bpmX100 ?? 0) / 100;
                if (bpm <= 0) return;
                const step = (4 * 60) / bpm;
                playback.seek(playback.position + (button.id === "jump-back" ? -step : step));
              }}
            >
              {button.glyph}
            </button>
          ))}
        </div>
        <button type="button" className={styles.beats} aria-label="Beat jump size" disabled>
          4Beats
          <span className={styles.chevron} aria-hidden />
        </button>
        <button
          type="button"
          className={styles.cue}
          aria-label="Cue"
          // Back to the start, which is what CUE does with no cue point set.
          onClick={() => playback.seek(0)}
          disabled={playback.idle}
        >
          CUE
        </button>
        <button
          type="button"
          className={styles.play}
          aria-label={playback.playing ? "Pause" : "Play"}
          onClick={playback.toggle}
          disabled={playback.idle}
        >
          <span
            className={playback.playing ? styles.pauseGlyph : styles.playGlyph}
            aria-hidden
          />
        </button>
      </div>

      <div className={styles.main}>
        <div className={styles.head}>
          <span className={styles.title} data-testid="player-title">
            {track ? track.title : "No track loaded"}
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
        </div>

        <div className={styles.overviewRow}>
          <div className={styles.artwork}>
            {track?.hasArtwork ? (
              <img src={artworkUrl(track.id)} alt="" draggable={false} />
            ) : null}
          </div>
          <div className={styles.overviewStack}>
            {/* Where the vocals are, from the analysis. */}
            <VocalStrip trackId={track && track.analysed ? track.id : null} />
            {/* Clicking either waveform seeks, which is what they are for. */}
            <div
              ref={overviewRef}
              className={styles.overview}
              data-testid="player-overview"
              onMouseDown={scrub}
              role="slider"
              aria-label="Position"
              aria-valuemin={0}
              aria-valuemax={Math.round(total)}
              aria-valuenow={Math.round(playback.position)}
              tabIndex={0}
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
              <span className={styles.playhead} style={{ left: `${progress * 100}%` }} aria-hidden />
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
          <div ref={detailRef} className={styles.detail} data-testid="player-detail" onMouseDown={scrub}>
            {track && track.analysed ? (
              <WaveformDetail
                trackId={track.id}
                progress={progress}
                span={span}
                width={detail.width}
                height={detail.height}
                detail
              />
            ) : null}
            <BeatGrid beats={beats} totalMs={total * 1000} window={window} />
            <CueMarkers cues={cues} totalMs={total * 1000} labelled window={window} />
            {/*
              The detail window is centred on the playhead, so the head is
              drawn at the centre rather than at the progress fraction — except
              near the ends, where the window is pinned and the head moves.
            */}
            <span
              className={styles.playhead}
              style={{ left: `${headPercent(progress, span)}%` }}
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
            <div className={styles.padCluster}>
              <div className={styles.gridEdits} aria-label="Beat grid">
                {GRID_EDITS.map((edit) => (
                  <button
                    key={edit.id}
                    type="button"
                    className={styles.gridEdit}
                    aria-label={edit.label}
                    // Saving needs the PQT2 tag, whose payload is not
                    // understood; an editor that cannot save is a trap.
                    disabled
                    title="Grid editing needs the PQT2 tag, which is not yet understood"
                  >
                    {edit.text}
                  </button>
                ))}
              </div>
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

            <div className={styles.memory} aria-label="Memory cues">
              <span className={styles.memoryLabel}>MEMORY</span>
              <button type="button" className={styles.step} aria-label="Previous memory cue" disabled>◀</button>
              <button type="button" className={styles.step} aria-label="Next memory cue" disabled>▶</button>
              <button type="button" className={styles.step} aria-label="Delete memory cue" disabled>✕</button>
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

          <button type="button" className={styles.chip} aria-label="Quantize" data-on>Q</button>
          <button type="button" className={styles.padMenu} aria-label="Pad settings">≡</button>
        </div>

        {playback.error ? (
          <p className={styles.error} role="alert">{playback.error}</p>
        ) : null}
      </div>

      <aside className={styles.side} aria-label="Cue list">
        {panel === "info" ? (
          <dl className={styles.info}>
            {(track
              ? [
                  ["Artist", track.artist],
                  ["Album", track.album],
                  ["Genre", track.genre],
                  ["Label", track.label],
                  ["Key", track.key],
                  ["BPM", formatBpm(track.bpmX100)],
                  ["Time", splitTime(track.durationSec).main],
                  ["Comments", track.comment],
                ]
              : []
            ).map(([name, value]) => (
              <div key={name} className={styles.infoRow}>
                <dt className={styles.infoLabel}>{name}</dt>
                {/* An empty field keeps its row: one that vanishes makes the
                    panel jump as the selection moves. */}
                <dd className={styles.infoValue}>{value || "—"}</dd>
              </div>
            ))}
          </dl>
        ) : (
          <div className={styles.cueList}>
            {cuesFor(cues, panel).map((cue) => (
              <button
                key={`${cue.memory ? "m" : cue.letter}-${cue.positionMs}`}
                type="button"
                className={styles.cueRow}
                onClick={() => playback.seek(cue.positionMs / 1000)}
              >
                <span className={styles.cueTime}>{splitTime(cue.positionMs / 1000).main}</span>
                <span className={styles.cueName}>
                  {cue.memory ? "CUE(Auto)" : `HOT CUE ${cue.letter}`}
                </span>
              </button>
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
    </section>
  );
});
