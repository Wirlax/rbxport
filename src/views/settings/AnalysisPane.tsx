/**
 * Analysis › Track Analysis. Captures docs/screenshots 9.47.29 and 9.47.32 PM.
 *
 * What the analyser computes — tempo, the beat grid, the key — it computes
 * every time, so the per-kind checkboxes, the mode and the BPM range the
 * capture shows are not here: a box that cannot be cleared is not a choice.
 * CUE Analysis is not here either; analysis sets no cues.
 */
import { usePreferencesContext } from "@/store/usePreferences";
import { Section, Sub, Toggle } from "./controls";

export type AnalysisTab = "track";

export const ANALYSIS_TABS: readonly { id: AnalysisTab; label: string }[] = [
  { id: "track", label: "Track Analysis" },
];

export function AnalysisPane(_: { tab: AnalysisTab }) {
  const { preferences, update } = usePreferencesContext();
  const auto = preferences.analysis.auto;
  return (
    <Section title="Track Analysis">
      <Sub>Auto Analysis</Sub>
      {/* rekordbox's switch is labelled Disable and is off by default, so on
          means no auto analysis. Kept that way: the wording is the capture's. */}
      <Toggle label="Disable" nested checked={!auto} onChange={(off) => update("analysis", { auto: !off })} />
    </Section>
  );
}
