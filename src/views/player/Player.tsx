/**
 * The preview player.
 *
 * Geometry is measured from `design/reference/macos/playlist-player@2x.png`:
 * the overview waveform, the phrase bar and the detail waveform were found by
 * saturation banding — they are the only saturated colour in the window — and
 * their heights are tokens with that recorded as their source.
 *
 * # What this does not do yet
 *
 * There is no audio. Nothing here plays, seeks or moves a playhead, because
 * decoding and output are not built (`rbl-audio` covers analysis only). The
 * transport is drawn and disabled rather than omitted, so the region reads as
 * a player waiting for playback rather than as an unfinished panel.
 *
 * What it *does* do is show the selected track: its title, artist, key, BPM,
 * artwork and waveform all come from the row the browser has selected.
 */
import { memo } from "react";

import type { RowDto } from "@/ipc/types";
import { artworkUrl } from "@/ipc/artwork";
import { formatBpm, formatDuration } from "@/lib/format";
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
        <button type="button" className={styles.cue} aria-label="Cue" disabled>
          CUE
        </button>
        <button type="button" className={styles.play} aria-label="Play" disabled>
          <span className={styles.playGlyph} aria-hidden />
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
              <span className={styles.readout}>{formatDuration(track.durationSec)}</span>
            </>
          ) : null}
        </div>

        <div className={styles.overview} data-testid="player-overview">
          {track && track.analysed ? (
            <WaveformPreview trackId={track.id} width={1200} height={14} />
          ) : null}
        </div>

        {/*
          The phrase bar. rekordbox fills this from the PSSI tag, which our
          analysis does not produce — the phrase detector is a stub that
          returns nothing rather than inventing structure — so it is drawn
          empty until that lands.
        */}
        <div className={styles.phrase} aria-label="Phrase" data-testid="player-phrase" />

        <div className={styles.detail} data-testid="player-detail">
          {track && track.analysed ? (
            <WaveformPreview trackId={track.id} width={1200} height={68} />
          ) : null}
        </div>
      </div>
    </section>
  );
});
