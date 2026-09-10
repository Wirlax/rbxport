/**
 * Keyboard: every key that is bound, grouped as rekordbox groups them.
 * Capture docs/screenshots 9.48.54 PM.
 *
 * Read-only. The bindings are rekordbox's Export preset transcribed, and
 * the keys are what the map in `shortcuts.ts` answers to; the capture's
 * "+" buttons and its reset are for rebinding, which is not built, so
 * neither is drawn.
 */
import { useMemo, useState } from "react";

import { TwistyIcon } from "@/components/icons";
import { BINDINGS, describeChord, detectPlatform, type Binding } from "@/lib/shortcuts";
import styles from "./Preferences.module.css";
import { Note } from "./controls";

const GROUPS: readonly Binding["group"][] = ["Browse", "Player A", "Menu"];

export function KeyboardPane() {
  const platform = useMemo(detectPlatform, []);
  // The capture opens with Browse closed and Player A open.
  const [open, setOpen] = useState<Set<Binding["group"]>>(() => new Set(["Player A"]));

  return (
    <section className={styles.section} aria-label="Keyboard">
      <div className={styles.keys}>
        {GROUPS.map((group) => {
          const shown = open.has(group);
          return (
            <div key={group}>
              <button
                type="button"
                className={styles.keyGroup}
                aria-expanded={shown}
                onClick={() =>
                  setOpen((current) => {
                    const next = new Set(current);
                    if (next.has(group)) next.delete(group);
                    else next.add(group);
                    return next;
                  })
                }
              >
                <TwistyIcon />
                {group}
              </button>
              {shown
                ? BINDINGS.filter((b) => b.group === group).map((b) => (
                    <div key={b.label} className={styles.keyRow}>
                      <span className={styles.keyName}>{b.label}</span>
                      <span className={styles.keyBadge}>{describeChord(b.chord, platform)}</span>
                    </div>
                  ))
                : null}
            </div>
          );
        })}
      </div>
      <Note>
        rekordbox&rsquo;s Export preset, as it is bound here. The keys cannot
        be changed.
      </Note>
    </section>
  );
}
