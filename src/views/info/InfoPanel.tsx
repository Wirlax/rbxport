/**
 * The track information panel.
 *
 * rekordbox calls it the Information Window and keeps it on the right of the
 * browser. Its width here is the one in the user's own
 * `browseSetting.xml` — `ListInfo w=557` — and it starts closed, which is
 * what that file records too, so the default view is unchanged.
 *
 * **The arrangement inside is ours, not measured.** No capture of rekordbox's
 * own Information Window was ever taken, so the fields and their order come
 * from the column catalogue transcribed from its header menu rather than from
 * a screenshot. Recorded in TODO.md; a capture would change it.
 */
import { Artwork } from "@/components/Artwork";
import type { RowDto } from "@/ipc/types";
import { formatBpm, formatDuration, formatShortDate } from "@/lib/format";
import { toCamelot } from "@/lib/camelot";
import styles from "./InfoPanel.module.css";

export interface InfoPanelProps {
  track: RowDto | null;
  onClose: () => void;
}

export interface Field {
  label: string;
  value: string;
}

/** The fields, in the order the column catalogue lists them. */
export function fieldsOf(track: RowDto): Field[] {
  const code = track.key ? toCamelot(track.key) : "";
  const key = track.key ? (code ? `${track.key} / ${code}` : track.key) : "";
  return [
    { label: "Artist", value: track.artist },
    { label: "Album", value: track.album },
    { label: "Genre", value: track.genre },
    { label: "Label", value: track.label },
    { label: "BPM", value: track.bpmX100 > 0 ? formatBpm(track.bpmX100) : "" },
    { label: "Key", value: key },
    { label: "Time", value: formatDuration(track.durationSec) },
    { label: "Rating", value: track.rating > 0 ? "★".repeat(track.rating) : "" },
    { label: "Date Added", value: formatShortDate(track.dateAdded) },
    { label: "Release Date", value: formatShortDate(track.releaseDate) },
    { label: "Hot Cue", value: track.cues },
    { label: "Comments", value: track.comment },
  ];
}

export function InfoPanel({ track, onClose }: InfoPanelProps) {
  return (
    <aside className={styles.panel} aria-label="Information">
      <header className={styles.head}>
        <span className={styles.title}>Information</span>
        <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
          ×
        </button>
      </header>

      {track === null ? (
        <p className={styles.empty}>Select a track.</p>
      ) : (
        <div className={styles.body}>
          <div className={styles.artwork} style={{ ["--hue" as string]: `${track.artworkHue}deg` }}>
            {track.hasArtwork ? (
              <Artwork trackId={track.id} className={styles.image} />
            ) : null}
          </div>
          <h3 className={styles.track}>{track.title}</h3>
          <dl className={styles.fields}>
            {fieldsOf(track).map((field) => (
              <div key={field.label} className={styles.field}>
                <dt className={styles.label}>{field.label}</dt>
                {/* An empty value keeps its row: a field that vanishes when
                    blank makes the panel jump as the selection moves. */}
                <dd className={styles.value}>{field.value || "—"}</dd>
              </div>
            ))}
          </dl>
        </div>
      )}
    </aside>
  );
}
