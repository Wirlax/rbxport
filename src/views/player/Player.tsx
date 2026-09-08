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

import type { Cue, RowDto } from "@/ipc/types";
import { getBackend } from "@/ipc/client";
import { artworkUrl } from "@/ipc/artwork";
import { formatBpm, formatDuration } from "@/lib/format";
import { usePlayback } from "@/store/usePlayback";
import { WaveformPreview } from "@/views/browser/WaveformPreview";
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
  cues, totalMs, labelled,
}: {
  cues: readonly Cue[];
  totalMs: number;
  labelled?: boolean;
}) {
  if (totalMs <= 0) return null;
  return (
    <>
      {cues.map((cue) => (
        <span
          key={`${cue.memory ? "m" : cue.letter}-${cue.positionMs}`}
          className={cue.memory ? styles.memoryCue : styles.hotCue}
          style={{ left: `${Math.min((cue.positionMs / totalMs) * 100, 100)}%` }}
          title={cue.memory ? "Memory cue" : `Hot cue ${cue.letter}`}
          aria-hidden
        >
          {labelled && !cue.memory ? cue.letter : null}
        </span>
      ))}
    </>
  );
});

/** Transport buttons, drawn in the order rekordbox has them. */
const TRANSPORT = [
  { id: "previous", label: "Previous track", glyph: "◀❘" },
  { id: "next", label: "Next track", glyph: "❘▶" },
] as const;

export const Player = memo(function Player({ track }: PlayerProps) {
  const playback = usePlayback(track?.id ?? null);
  const [cues, setCues] = useState<Cue[]>([]);

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
            <WaveformPreview trackId={track.id} width={1200} height={14} />
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

        <div className={styles.detail} data-testid="player-detail" onMouseDown={scrub}>
          {track && track.analysed ? (
            <WaveformPreview trackId={track.id} width={1200} height={68} />
          ) : null}
          <CueMarkers cues={cues} totalMs={total * 1000} labelled />
          <span className={styles.playhead} style={{ left: `${progress * 100}%` }} aria-hidden />
        </div>
        {playback.error ? (
          <p className={styles.error} role="alert">{playback.error}</p>
        ) : null}
      </div>
    </section>
  );
});
