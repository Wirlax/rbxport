/**
 * Audio › Configuration. Capture docs/screenshots 9.47.23 PM: the output
 * device, Sample Rate, Buffer size and the Metronome, and then a section of
 * ours — the master limiter, which rekordbox's pane does not have: two
 * decks at full level sum past what the output can carry, and this is where
 * the switch and its two numbers live. Input/Output, the channel routing
 * tab, is not here: there is one stereo output and nothing to route.
 */
import { useEffect, useState } from "react";

import { getBackend } from "@/ipc/client";
import type { AudioDevices, Limiter } from "@/ipc/types";
import { BUFFER_SIZES, SAMPLE_RATES, type SampleRate } from "@/lib/preferences";
import { CEILING_DB, RELEASE_MS } from "@/store/useLimiter";
import { usePreferencesContext } from "@/store/usePreferences";
import styles from "./Preferences.module.css";
import { Note, Radios, Section, Select, Separator, Slider, Sub, Toggle } from "./controls";

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

/** `512 samples (10.7 ms)`, as the capture prints the buffer size. */
export function bufferCaption(frames: number, sampleRate: number): string {
  const ms = sampleRate > 0 ? (frames / sampleRate) * 1000 : 0;
  return `${frames} samples (${ms.toFixed(1)} ms)`;
}

export function AudioPane({ limiter, onLimiterChange, reduction }: AudioPaneProps) {
  const [audio, setAudio] = useState<AudioDevices | null>(null);
  const { preferences, update } = usePreferencesContext();
  const prefs = preferences.audio;
  const set = (patch: Partial<typeof prefs>) => update("audio", patch);
  // The slider moves over the stops; the stop is what is stored.
  const bufferStop = Math.max(0, BUFFER_SIZES.indexOf(prefs.bufferSize));

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
    <>
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
    </Section>

    <Section title="Sample Rate">
      <Select
        label="Sample Rate"
        value={String(prefs.sampleRate)}
        choices={SAMPLE_RATES.map((rate) => ({ value: String(rate), label: `${rate} Hz` }))}
        onChange={(value) => set({ sampleRate: Number(value) as SampleRate })}
      />
      <Note>
        Asked of the device the next time a deck plays. One that does not
        offer the rate is opened at its own, and the log says so.
      </Note>
    </Section>

    <Section title="Buffer size">
      <div className={styles.row} data-nested="">
        <span data-testid="buffer-size">{bufferCaption(prefs.bufferSize, prefs.sampleRate)}</span>
      </div>
      <Slider
        label="Buffer size"
        value={bufferStop}
        steps={BUFFER_SIZES.length}
        ends={[`${BUFFER_SIZES[0]}`, `${BUFFER_SIZES[BUFFER_SIZES.length - 1]}`]}
        onChange={(stop) => set({ bufferSize: BUFFER_SIZES[stop] ?? prefs.bufferSize })}
      />
      <Note>
        Smaller is quicker to answer a press and easier to underrun; larger
        is the other way round. Takes effect with the sample rate.
      </Note>
    </Section>

    <Section title="Metronome">
      <Radios
        label="Metronome"
        value={String(prefs.metronomeSound)}
        choices={[
          { value: "1", label: "Click Sound 01" },
          { value: "2", label: "Click Sound 02" },
          { value: "3", label: "Click Sound 03" },
        ]}
        onChange={(value) => set({ metronomeSound: Number(value) as 1 | 2 | 3 })}
      />
      <Separator />
      <Sub>Volume</Sub>
      <Radios
        label="Metronome volume"
        nested
        value={prefs.metronomeVolume}
        choices={[
          { value: "small", label: "Small" },
          { value: "middle", label: "Middle" },
          { value: "large", label: "Large" },
        ]}
        onChange={(metronomeVolume) => set({ metronomeVolume })}
      />
      <Note>
        The metronome button in the deck&rsquo;s GRID EDIT row clicks on every
        beat of the grid while the deck plays.
      </Note>
    </Section>

    <Section title="Master limiter">
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
    </>
  );
}
