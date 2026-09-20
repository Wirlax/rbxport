/** Analysis presets retain high-precision beat placement and key detection. */
import { usePreferencesContext } from "@/store/usePreferences";
import { Section, Select, Sub, Toggle } from "./controls";

export type AnalysisTab = "track";

export const ANALYSIS_TABS: readonly { id: AnalysisTab; label: string }[] = [
  { id: "track", label: "Track Analysis" },
];

export function AnalysisPane(_: { tab: AnalysisTab }) {
  const { preferences, update } = usePreferencesContext();
  const auto = preferences.analysis.auto;
  return (
    <Section title="Track Analysis">
      <Select label="Analysis mode" value={preferences.analysis.mode}
        choices={[{ value: "rekordbox", label: "Rekordbox — Normal, 70–180 BPM, high precision" },
          { value: "rbxport", label: "RBXport — Electronic, 70–180 BPM, high precision" }]}
        onChange={mode => update("analysis", { mode })} />
      <Select label="Tracks analysed at once" value={String(preferences.analysis.concurrentTracks)}
        choices={[1, 2, 3, 4].map(n => ({ value: String(n), label: String(n) }))}
        onChange={value => update("analysis", { concurrentTracks: Number(value) })} />
      <p>Three tracks at once is the default. Lower this if analysis competes with playback.</p>
      <Sub>Auto Analysis</Sub>
      {/* rekordbox's switch is labelled Disable and is off by default, so on
          means no auto analysis. Kept that way: the wording is the capture's. */}
      <Toggle label="Disable" nested checked={!auto} onChange={(off) => update("analysis", { auto: !off })} />
    </Section>
  );
}
