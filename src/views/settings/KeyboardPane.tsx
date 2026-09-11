/**
 * Keyboard: rekordbox's Export key map, grouped as its pane groups it.
 * Capture docs/screenshots 9.48.54 PM.
 *
 * Every row is rekordbox's — the ten groups and the commands in each, with
 * the key the Export preset binds (`keymap.ts`). A row this app answers is
 * drawn live; one it does not is greyed, the way rekordbox greys what a
 * selection cannot do, so the map reads the same here as there and nothing
 * has to be relearnt later. This app's own few shortcuts are filed with
 * them. Read-only: the capture's "+" buttons and its reset are for
 * rebinding, which is not built, so neither is drawn.
 */
import { useMemo, useState } from "react";

import { TwistyIcon } from "@/components/icons";
import { KEYMAP, KEYMAP_GROUPS, keyForPlatform, type KeymapGroup } from "@/lib/keymap";
import { BINDINGS, describeChord, detectPlatform } from "@/lib/shortcuts";
import styles from "./Preferences.module.css";
import { Note } from "./controls";

interface Row {
  id: string;
  label: string;
  /** The key as printed, or null for a row rekordbox lists unbound. */
  key: string | null;
  /** Whether this app answers it. */
  built: boolean;
}

/** The pane's rows for a group: rekordbox's, then this app's own. */
export function rowsFor(group: KeymapGroup, mac: boolean): Row[] {
  const built = new Set(BINDINGS.map((b) => b.command).filter((c): c is string => c !== undefined));
  const rows: Row[] = KEYMAP[group].map((row) => ({
    id: row.id,
    label: row.label,
    key: row.key === null ? null : keyForPlatform(row.key, mac),
    built: built.has(row.id),
  }));
  for (const binding of BINDINGS) {
    if (binding.command !== undefined || binding.pane !== group) continue;
    rows.push({
      id: `ours:${binding.label}`,
      label: binding.label,
      key: describeChord(binding.chord, { mac }),
      built: true,
    });
  }
  return rows;
}

export function KeyboardPane() {
  const platform = useMemo(detectPlatform, []);
  // The capture opens with Browse closed and Player A open.
  const [open, setOpen] = useState<Set<KeymapGroup>>(() => new Set(["Player A"]));

  return (
    <section className={styles.section} aria-label="Keyboard">
      <div className={styles.keys}>
        {KEYMAP_GROUPS.map((group) => {
          const shown = open.has(group);
          const rows = shown ? rowsFor(group, platform.mac) : [];
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
              {shown && rows.length === 0 ? (
                <div className={styles.keyRow} data-dim="">
                  <span className={styles.keyName}>Nothing is bound here in the Export preset.</span>
                </div>
              ) : null}
              {rows.map((row) => (
                <div
                  key={row.id}
                  className={styles.keyRow}
                  data-dim={row.built ? undefined : ""}
                  title={row.built ? undefined : "rekordbox has this; it is not built here"}
                >
                  <span className={styles.keyName}>{row.label}</span>
                  {row.key !== null ? <span className={styles.keyBadge}>{row.key}</span> : null}
                </div>
              ))}
            </div>
          );
        })}
      </div>
      <Note>
        rekordbox&rsquo;s Export preset. The keys in white work here; the
        greyed ones are rekordbox&rsquo;s and not built. The keys cannot be
        changed.
      </Note>
    </section>
  );
}
