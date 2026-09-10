/**
 * Audio › Configuration. Capture docs/screenshots 9.47.23 PM.
 *
 * The output device, and the master limiter. Sample rate and buffer size are
 * the device's own defaults — the engine opens whatever the device offers
 * rather than asking for a rate — and the metronome is not built, so neither
 * is drawn. Input/Output, the channel routing tab, is not here for the same
 * reason.
 *
 * The limiter is ours, not a control rekordbox's pane has: two decks at full
 * level sum past what the output can carry, and this is where the switch and
 * its two numbers live.
 */
import { useEffect, useState } from "react";

import { getBackend } from "@/ipc/client";
import type { AudioDevices, Limiter } from "@/ipc/types";
import { CEILING_DB, RELEASE_MS } from "@/store/useLimiter";
import styles from "./Preferences.module.css";
import { Note, Section, Slider, Sub, Toggle } from "./controls";

export type AudioTab = "configuration";

export const AUDIO_TABS: readonly { id: AudioTab; label: string }[] = [
  { id: "configuration", label: "Configuration" },
];

export interface AudioPaneProps {
  tab: AudioTab;
  limiter: Limiter;
  onLimiterChange: (change: Partial<Limiter>) => void;
  /** How far the limiter is turning the sum down right now, in dB. */
  reduction: number;
}

export function AudioPane({ limiter, onLimiterChange, reduction }: AudioPaneProps) {
  const [audio, setAudio] = useState<AudioDevices | null>(null);

  // Read when the pane opens rather than held: an interface is plugged in
  // while the app is running more often than not, and a list from launch
  // would be missing whatever somebody just connected.
  useEffect(() => {
    void (async () => {
      try {
        const backend = await getBackend();
        setAudio(await backend.audioDevices());
      } catch {
        // A build with no engine behind it has no devices to offer, and the
        // pane says so rather than showing an error for something nobody
        // asked about.
      }
    })();
  }, []);

  return (
    <Section title="Audio" label="Audio output">
      {audio && audio.devices.length > 0 ? (
        <>
          <div className={styles.actions}>
            <select
              className={styles.select}
              aria-label="Audio output device"
              value={audio.chosen ?? ""}
              onChange={(event) => {
                const chosen = event.target.value === "" ? null : event.target.value;
                setAudio({ ...audio, chosen });
                void (async () => {
                  const backend = await getBackend();
                  await backend.setAudioDevice(chosen);
                })();
              }}
            >
              <option value="">
                System default
                {audio.devices.find((d) => d.id === audio.default)?.name
                  ? ` — ${audio.devices.find((d) => d.id === audio.default)?.name}`
                  : ""}
              </option>
              {audio.devices.map((device) => (
                <option key={device.id} value={device.id}>
                  {device.name}
                </option>
              ))}
            </select>
          </div>
          <Note>
            A change takes effect the next time a deck plays: a running stream
            belongs to the device it was opened on.
          </Note>
        </>
      ) : (
        <Note>
          No output devices to choose from — this build has no audio engine
          behind it.
        </Note>
      )}

      <Sub>Master limiter</Sub>
      <Toggle
        label={
          limiter.enabled && reduction > 0.05
            ? `Limiter — turning down ${reduction.toFixed(1)} dB`
            : "Limiter"
        }
        checked={limiter.enabled}
        onChange={(enabled) => onLimiterChange({ enabled })}
      />
      <div className={styles.row} data-nested="">
        <span>Ceiling {limiter.ceilingDb.toFixed(1)} dB</span>
      </div>
      <Slider
        label="Limiter ceiling"
        value={limiter.ceilingDb}
        range={CEILING_DB}
        ends={[`${CEILING_DB.min} dB`, `${CEILING_DB.max} dB`]}
        disabled={!limiter.enabled}
        onChange={(ceilingDb) => onLimiterChange({ ceilingDb })}
      />
      <div className={styles.row} data-nested="">
        <span>Release {Math.round(limiter.releaseMs)} ms</span>
      </div>
      <Slider
        label="Limiter release"
        value={limiter.releaseMs}
        range={RELEASE_MS}
        ends={[`${RELEASE_MS.min} ms`, `${RELEASE_MS.max} ms`]}
        disabled={!limiter.enabled}
        onChange={(releaseMs) => onLimiterChange({ releaseMs })}
      />
      <Note>
        Two decks at full level add up to more than the output can carry. The
        limiter turns the sum down for the moment a peak lasts rather than
        letting it clip; off, anything over full scale is flat-topped.
      </Note>
    </Section>
  );
}
