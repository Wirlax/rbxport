/**
 * The preview player.
 *
 * Geometry is measured from `design/reference/macos/playlist-player@2x.png`:
 * the overview waveform, the phrase bar and the detail waveform were found by
 * saturation banding — they are the only saturated colour in the window — and
 * their heights are tokens with that recorded as their source.
 *
 * Playback is an `<audio>` element over the `rbl://` scheme rather than an
 * audio stack in Rust — see `usePlayback`. Outside Tauri there is no such
 * scheme, so the transport is drawn and disabled: a player waiting for a
 * backend, rather than an unfinished panel.
 */
import { memo, useEffect, useState } from "react";

import type { Beat, Cue, RowDto } from "@/ipc/types";
import { getBackend } from "@/ipc/client";
import { useElementSize } from "@/store/useElementSize";
import { artworkUrl } from "@/ipc/artwork";
import { formatBpm, formatDuration } from "@/lib/format";
import { usePlayback } from "@/store/usePlayback";
import { WaveformDetail } from "./WaveformDetail";
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

/** How much of a track the detail waveform shows at once. */
const DETAIL_SPAN = 0.08;

/** Transport buttons, drawn in the order rekordbox has them. */
const TRANSPORT = [
  { id: "previous", label: "Previous track", glyph: "◀❘" },
  { id: "next", label: "Next track", glyph: "❘▶" },
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

  useEffect(() => {
    if (!track) {
      setCues([]);
      return;
    }
    let live = true;
    void (async () => {
      const backend = await getBackend();
      const found = await backend.trackCues(track.id);
      // The track may have changed while this was in flight.
      if (live) setCues(found);
    })();
    return () => {
      live = false;
    };
  }, [track]);


  // The fraction played, for the playhead. Falls back to the track's own
  // length before the file's metadata has loaded, so the head does not jump.
  const total = playback.duration || track?.durationSec || 0;
  const progress = total > 0 ? Math.min(playback.position / total, 1) : 0;

  /** The slice of the track the detail waveform is showing. */
  const detailWindow = (fraction: number) => {
    const half = DETAIL_SPAN / 2;
    const centre = Math.min(Math.max(fraction, half), 1 - half);
    return { from: centre - half, to: centre + half };
  };

  // The grid for the detail window only, refetched as the window moves. A
  // whole track's beats would blow the IPC cap on a long mix.
  const windowFrom = detailWindow(progress).from;
  const windowTo = detailWindow(progress).to;
  // The window quantised to hundredths, so the grid is refetched as the window
  // moves on rather than on every `timeupdate` — of which a four-minute track
  // fires several hundred.
  const fromStep = Math.round(windowFrom * 100);
  const toStep = Math.round(windowTo * 100);

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
        Math.floor(windowFrom * total * 1000),
        Math.ceil(windowTo * total * 1000),
      );
      if (live) setBeats(found);
    })();
    return () => {
      live = false;
    };
    // The unquantised bounds are read inside but are deliberately not
    // dependencies; the steps above are what decides when to refetch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [track, total, fromStep, toStep]);

  // Where the playhead sits within the detail window. Centred, except at the
  // ends where the window stops moving and the head crosses it instead.
  const detailHeadPercent = (fraction: number) => {
    const half = DETAIL_SPAN / 2;
    if (fraction <= half) return (fraction / DETAIL_SPAN) * 100;
    if (fraction >= 1 - half) return ((fraction - (1 - DETAIL_SPAN)) / DETAIL_SPAN) * 100;
    return 50;
  };

  const scrub = (event: React.MouseEvent<HTMLDivElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    if (box.width <= 0) return;
    playback.seekFraction((event.clientX - box.left) / box.width);
  };

  return (
    <section className={styles.player} aria-label="Preview player">
      <div className={styles.transport}>
        <div className={styles.skips}>
          {TRANSPORT.map((button) => (
            <button
              key={button.id}
              type="button"
              className={styles.skip}
              aria-label={button.label}
              // No audio yet, so these are drawn and inert rather than absent.
              disabled
            >
              {button.glyph}
            </button>
          ))}
        </div>
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
          <div className={styles.artwork}>
            {track?.hasArtwork ? (
              <img src={artworkUrl(track.id)} alt="" draggable={false} />
            ) : null}
          </div>
          <span className={styles.title} data-testid="player-title">
            {track ? track.title : "No track loaded"}
          </span>
          {track ? (
            <>
              <span className={styles.artist}>{track.artist}</span>
              <span className={styles.readout}>{track.key}</span>
              <span className={styles.readout}>{formatBpm(track.bpmX100)}</span>
              <span className={styles.readout} data-testid="player-time">
                {formatDuration(Math.round(playback.position))} /{" "}
                {formatDuration(Math.round(total))}
              </span>
            </>
          ) : null}
        </div>

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
            />
          ) : null}
          <CueMarkers cues={cues} totalMs={total * 1000} />
          <span className={styles.playhead} style={{ left: `${progress * 100}%` }} aria-hidden />
        </div>

        {/*
          The phrase bar. rekordbox fills this from the PSSI tag, which our
          analysis does not produce — the phrase detector is a stub that
          returns nothing rather than inventing structure — so it is drawn
          empty until that lands.
        */}
        <div className={styles.phrase} aria-label="Phrase" data-testid="player-phrase" />

        <div ref={detailRef} className={styles.detail} data-testid="player-detail" onMouseDown={scrub}>
          {track && track.analysed ? (
            <WaveformDetail
              trackId={track.id}
              progress={progress}
              span={DETAIL_SPAN}
              width={detail.width}
              height={detail.height}
            />
          ) : null}
          <BeatGrid beats={beats} totalMs={total * 1000} window={detailWindow(progress)} />
          <CueMarkers cues={cues} totalMs={total * 1000} labelled window={detailWindow(progress)} />
          {/*
            The detail window is centred on the playhead, so the head is drawn
            at the centre rather than at the progress fraction — except near
            the ends, where the window is pinned and the head moves instead.
          */}
          <span
            className={styles.playhead}
            style={{ left: `${detailHeadPercent(progress)}%` }}
            aria-hidden
          />
        </div>
        {playback.error ? (
          <p className={styles.error} role="alert">{playback.error}</p>
        ) : null}
      </div>
    </section>
  );
});
