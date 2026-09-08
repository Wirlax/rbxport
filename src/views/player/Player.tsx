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
import { memo } from "react";

import type { RowDto } from "@/ipc/types";
import { artworkUrl } from "@/ipc/artwork";
import { formatBpm, formatDuration } from "@/lib/format";
import { usePlayback } from "@/store/usePlayback";
import { WaveformPreview } from "@/views/browser/WaveformPreview";
import styles from "./Player.module.css";

export interface PlayerProps {
  /** The row the browser has selected, or `null` when nothing is. */
  track: RowDto | null;
}

/** Transport buttons, drawn in the order rekordbox has them. */
const TRANSPORT = [
  { id: "previous", label: "Previous track", glyph: "◀❘" },
  { id: "next", label: "Next track", glyph: "❘▶" },
] as const;

export const Player = memo(function Player({ track }: PlayerProps) {
  const playback = usePlayback(track?.id ?? null);
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
          <span className={styles.playhead} style={{ left: `${progress * 100}%` }} aria-hidden />
        </div>
        {playback.error ? (
          <p className={styles.error} role="alert">{playback.error}</p>
        ) : null}
      </div>
    </section>
  );
});
