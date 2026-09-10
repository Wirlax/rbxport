/**
 * View: Display Type and Layout. Captures docs/screenshots 9.46.53 to
 * 9.47.14 PM.
 *
 * The Color tab is not here: it holds the light theme, the BLUE / RGB
 * waveform palettes and the hot cue palette, none of which this build draws
 * — the deck has one waveform renderer, the three-band one rekordbox 7
 * defaults to — and a tab of three dead choices is not a tab.
 */
import { BROWSE_SCALE_STEPS } from "@/lib/preferences";
import { usePreferencesContext } from "@/store/usePreferences";
import styles from "./Preferences.module.css";
import {
  Button, Checkbox, Note, Radios, Section, Select, Separator, Slider, Sub, Toggle,
} from "./controls";

export type ViewTab = "display" | "layout";

export const VIEW_TABS: readonly { id: ViewTab; label: string }[] = [
  { id: "display", label: "Display Type" },
  { id: "layout", label: "Layout" },
];

export interface ViewPaneProps {
  tab: ViewTab;
  onResetColumns: () => void;
  onResetLayout: () => void;
}

export function ViewPane({ tab, onResetColumns, onResetLayout }: ViewPaneProps) {
  const { preferences, update } = usePreferencesContext();
  const view = preferences.view;
  const set = (patch: Partial<typeof view>) => update("view", patch);

  if (tab === "layout") {
    return (
      <>
        <Section title="Layout">
          <Sub>Media Browser</Sub>
          {/* Of rekordbox's twelve sources only the Explorer exists here; the
              others are streaming services, iTunes and its own formats. */}
          <Checkbox
            label="Explorer"
            nested
            checked={view.explorer}
            onChange={(explorer) => set({ explorer })}
          />
          <Separator />
          <Sub>Browser panel</Sub>
          <Checkbox
            label="Display Cue Markers on Preview"
            nested
            checked={view.previewCueMarkers}
            onChange={(previewCueMarkers) => set({ previewCueMarkers })}
          />
          <Checkbox
            label="Display All Tracks in the Playlists"
            nested
            checked={view.allTracks}
            onChange={(allTracks) => set({ allTracks })}
          />
          <Checkbox
            label="Display the number of tracks in a playlist on the Tree View"
            nested
            checked={view.playlistCounts}
            onChange={(playlistCounts) => set({ playlistCounts })}
          />
          <Separator />
          <Sub>Phrases</Sub>
          <Checkbox
            label="Phrase (Full Waveform)"
            nested
            checked={view.phraseFull}
            onChange={(phraseFull) => set({ phraseFull })}
          />
          <Toggle
            label="Always show types of phrases"
            nested
            checked={view.phraseLabels}
            disabled={!view.phraseFull}
            onChange={(phraseLabels) => set({ phraseLabels })}
          />
          <Separator />
          <Sub>Vocal</Sub>
          <Checkbox
            label="Vocal (Full Waveform)"
            nested
            checked={view.vocalFull}
            onChange={(vocalFull) => set({ vocalFull })}
          />
        </Section>
        {/* Ours: the columns and pane widths the browser remembers. */}
        <Section title="Browser">
          <div className={styles.actions}>
            <Button onClick={onResetColumns}>Reset columns</Button>
            <Button onClick={onResetLayout}>Reset panel sizes</Button>
          </div>
          <Note>
            Columns, their order and widths, and the tree&rsquo;s width are
            remembered between sessions.
          </Note>
        </Section>
      </>
    );
  }

  return (
    <>
      <Section title="Language">
        {/* The strings are rekordbox's own English ones; no other language
            has been transcribed, so there is nothing else to pick. */}
        <Select label="Language" value="en" choices={[{ value: "en", label: "English" }]} onChange={() => {}} />
      </Section>
      <Section title="Tooltips">
        <Toggle label="Show Tooltips" checked={view.tooltips} onChange={(tooltips) => set({ tooltips })} />
      </Section>
      <Section title="Browse">
        <Sub>FontSize</Sub>
        <Slider
          label="FontSize"
          value={view.browseFontSize}
          steps={BROWSE_SCALE_STEPS}
          onChange={(browseFontSize) => set({ browseFontSize })}
        />
        <Toggle label="Bold" nested checked={view.browseBold} onChange={(browseBold) => set({ browseBold })} />
        <Separator />
        <Sub>Line Space</Sub>
        <Slider
          label="Line Space"
          value={view.browseLineSpace}
          steps={BROWSE_SCALE_STEPS}
          onChange={(browseLineSpace) => set({ browseLineSpace })}
        />
      </Section>
      <Section title="Key display format">
        <Radios
          label="Key display format"
          value={view.keyDisplay}
          choices={[
            { value: "classic", label: "Classic" },
            { value: "alphanumeric", label: "Alphanumeric" },
          ]}
          onChange={(keyDisplay) => set({ keyDisplay })}
        />
      </Section>
      <Section title="Waveform">
        <Sub>Waveform Drawing Rate</Sub>
        <Radios
          label="Waveform Drawing Rate"
          nested
          value={view.waveformRate}
          choices={[
            { value: "high", label: "High Speed" },
            { value: "medium", label: "Medium Speed" },
            { value: "low", label: "Low Speed" },
          ]}
          onChange={(waveformRate) => set({ waveformRate })}
        />
        <Separator />
        <Sub>Full/Preview Waveform</Sub>
        <Radios
          label="Full/Preview Waveform"
          nested
          value={view.overviewWaveform}
          choices={[
            { value: "half", label: "Half Waveform" },
            { value: "full", label: "Full Waveform" },
          ]}
          onChange={(overviewWaveform) => set({ overviewWaveform })}
        />
      </Section>
      <Section title="Traffic Light">
        {/* rekordbox's own reaches, from its tooltip: for a track in 2A,
            Same Key lights 2A; Related Key 1 adds 2B; 2 adds 1A and 3A; 3
            adds 1B and 3B. */}
        <Select
          label="Traffic Light"
          value={view.trafficLight}
          choices={[
            { value: "same", label: "Same Key" },
            { value: "related1", label: "Related Key 1" },
            { value: "related2", label: "Related Key 2" },
            { value: "related3", label: "Related Key 3" },
          ]}
          onChange={(trafficLight) => set({ trafficLight })}
        />
        <Note>
          Rows whose key goes with the loaded track&rsquo;s are lit green in
          the browser; the MASTER menu above the track list picks which deck.
        </Note>
      </Section>
    </>
  );
}
