import { useEffect, useLayoutEffect, useRef } from "react";
import { createPortal } from "react-dom";

import styles from "./CueColorMenu.module.css";

const MEMORY = [
  ["Pink", "#E778F1"], ["Red", "#E33122"], ["Orange", "#EBA44A"], ["Yellow", "#F4E458"],
  ["Green", "#66DD42"], ["Aqua", "#56BDF3"], ["Blue", "#204FEF"], ["Purple", "#8B1EEF"],
] as const;

// Rekordbox's compact 4 × 4 hot-cue picker, in its displayed order. Values
// are ColorTableIndex entries from the desktop palette.
const HOT = [49, 56, 60, 62, 1, 3, 9, 15, 18, 22, 26, 30, 32, 38, 41, 46] as const;
const HOT_CSS = [
  "#DE44CF", "#B432FF", "#AA72FF", "#6473FF", "#305AFF", "#508CFF", "#00E0FF", "#19A08C",
  "#10B176", "#28E214", "#A5E116", "#B4BE04", "#C3AF04", "#E0641B", "#E02823", "#F51E8C",
] as const;

export function CueColorMenu({ x, y, memory, onChoose, onClose }: {
  x: number; y: number; memory: boolean;
  onChoose: (colour: number | null) => void; onClose: () => void;
}) {
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const outside = (event: MouseEvent) => { if (!menu.current?.contains(event.target as Node)) onClose(); };
    const key = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    window.addEventListener("mousedown", outside, true);
    window.addEventListener("keydown", key);
    return () => { window.removeEventListener("mousedown", outside, true); window.removeEventListener("keydown", key); };
  }, [onClose]);
  useLayoutEffect(() => {
    const element = menu.current;
    if (!element) return;
    const box = element.getBoundingClientRect();
    element.style.left = `${Math.max(8, Math.min(x, window.innerWidth - box.width - 8))}px`;
    element.style.top = `${Math.max(8, Math.min(y, window.innerHeight - box.height - 8))}px`;
  }, [x, y, memory]);
  const choose = (value: number | null) => { onChoose(value); onClose(); };
  return createPortal(
    <div ref={menu} className={`${styles.menu} ${memory ? "" : styles.hotMenu}`} role="menu" aria-label={`${memory ? "Memory" : "Hot"} cue color`} style={{left: x, top: y}}>
      {memory ? MEMORY.map(([name, color], index) => (
        <button key={name} role="menuitem" className={styles.memory} onClick={() => choose(index)}>
          <span className={styles.dot} style={{background: color}} />{name}
        </button>
      )) : (
        <>
          <button role="menuitem" className={styles.comments} disabled>Add comments</button>
          <div className={styles.grid} role="group" aria-label="Hot cue colors">
            {HOT.map((value, index) => <button key={value} aria-label={`Color ${index + 1}`} style={{background: HOT_CSS[index]}} onClick={() => choose(value)} />)}
          </div>
        </>
      )}
      <button role="menuitem" className={styles.reset} onClick={() => choose(null)}>{memory ? "No Color" : "Reset"}</button>
    </div>, document.body,
  );
}
