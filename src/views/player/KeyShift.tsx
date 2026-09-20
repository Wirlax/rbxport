import styles from "./TempoField.module.css";

export function KeyShift({ musicalKey, shift, disabled, onChange }: {
  musicalKey: string;
  shift: number;
  disabled: boolean;
  onChange: (shift: number) => void;
}) {
  return (
    <div className={styles.keys} role="group" aria-label="Key shift">
      <button type="button" className={styles.key} aria-label="Key down a semitone"
        disabled={disabled || shift <= -12} onClick={() => onChange(shift - 1)}>−</button>
      <span>{musicalKey}</span>
      <span className={styles.keyShift} data-testid="key-shift">{shift === 0 ? "+/-0" : `${shift > 0 ? "+" : ""}${shift}`}</span>
      <button type="button" className={styles.key} aria-label="Key up a semitone"
        disabled={disabled || shift >= 12} onClick={() => onChange(shift + 1)}>+</button>
    </div>
  );
}
