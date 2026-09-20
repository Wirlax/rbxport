import { formatBpm } from "@/lib/format";
import { usePreferencesContext } from "@/store/usePreferences";
import styles from "./TempoField.module.css";

export function TempoToggle({ bpmX100, className }: { bpmX100: number; className?: string | undefined }) {
  const { preferences, update } = usePreferencesContext();
  return (
    <button type="button" className={`${styles.tempoToggle} ${className ?? ""}`}
      aria-label="Toggle tempo slider" aria-expanded={preferences.view.tempoSlider}
      data-testid="player-bpm"
      onClick={() => update("view", { tempoSlider: !preferences.view.tempoSlider })}>
      {formatBpm(bpmX100)}
    </button>
  );
}
