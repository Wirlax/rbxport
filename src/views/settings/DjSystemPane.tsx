/**
 * DJ System: what a stick gets when it is first exported to. Captures
 * docs/screenshots 9.47.58 to 9.48.44 PM.
 *
 * A stick that already carries settings keeps its own — its device panel
 * edits those — so these are the defaults, applied once, to a stick that
 * has none. The General tab is `DEVSETTING.DAT`; Category, Sort and Column
 * are `exportLibrary.db`'s rows.
 *
 * Not here: the account nickname (no account), the background colour (its
 * choices have not been seen), the jog image (not written), My Settings
 * (`MYSETTING.DAT` is not written), and the Device tab (history import is
 * not built). PRO DJ LINK, in place of rekordbox's Others tab, holds the
 * LINK switch and the network interface it runs on.
 */
import { useEffect, useMemo, useState } from "react";

import { getBackend } from "@/ipc/client";
import type { LinkStatus, MenuSlot } from "@/ipc/types";
import { displayName, isFixed } from "@/lib/deviceSettings";
import { usePreferencesContext } from "@/store/usePreferences";
import { ListPairTab } from "@/views/devices/ListPairTab";
import styles from "./Preferences.module.css";
import { Button, Note, Radios, Section, Select, Sub } from "./controls";

export type DjSystemTab = "general" | "category" | "sort" | "column" | "link";

export const DJ_SYSTEM_TABS: readonly { id: DjSystemTab; label: string }[] = [
  { id: "general", label: "General" },
  { id: "category", label: "Category" },
  { id: "sort", label: "Sort" },
  { id: "column", label: "Column" },
  { id: "link", label: "PRO DJ LINK" },
];

/** The reference rows, read once: they are what a stored null stands for. */
function useReferenceRows(): { categories: MenuSlot[]; sorts: MenuSlot[] } | null {
  const [rows, setRows] = useState<{ categories: MenuSlot[]; sorts: MenuSlot[] } | null>(null);
  useEffect(() => {
    let live = true;
    void getBackend()
      .then((backend) => backend.referenceStickSettings())
      .then((read) => {
        if (live) setRows(read);
      })
      .catch(() => {
        // Nothing to edit against; the tabs stay empty rather than inventing rows.
      });
    return () => {
      live = false;
    };
  }, []);
  return rows;
}

export function DjSystemPane({ tab }: { tab: DjSystemTab }) {
  const { preferences, update } = usePreferencesContext();
  const dj = preferences.djSystem;
  const set = (patch: Partial<typeof dj>) => update("djSystem", patch);
  const reference = useReferenceRows();
  const categories = dj.categories ?? reference?.categories ?? [];
  const sorts = dj.sorts ?? reference?.sorts ?? [];

  if (tab === "category" || tab === "sort") {
    return (
      <Section title={tab === "category" ? "Category" : "Sort"}>
        <div className={styles.pairHost}>
          <ListPairTab
            kind={tab}
            slots={tab === "category" ? categories : sorts}
            disabled={reference === null && (tab === "category" ? dj.categories : dj.sorts) === null}
            onChange={(slots) => set(tab === "category" ? { categories: slots } : { sorts: slots })}
          />
        </div>
        <Note>
          What a CDJ lists when it browses a stick this application has
          exported to for the first time. A stick that already holds a
          library keeps its own, which its device panel edits.
        </Note>
      </Section>
    );
  }

  if (tab === "column") {
    return <ColumnSection sorts={sorts} subColumn={dj.subColumn} onChange={(subColumn) => set({ subColumn })} />;
  }

  if (tab === "link") {
    return <LinkSection linkInterface={dj.linkInterface} onChoose={(linkInterface) => set({ linkInterface })} />;
  }

  return (
    <>
      <Section title="Waveform color">
        <Sub>Waveform color displayed on CDJ/XDJ</Sub>
        <Radios
          label="Waveform color displayed on CDJ/XDJ"
          nested
          value={dj.waveformColor}
          choices={[
            { value: "blue", label: "BLUE" },
            { value: "rgb", label: "RGB" },
            { value: "3band", label: "3Band" },
          ]}
          onChange={(waveformColor) => set({ waveformColor })}
        />
      </Section>
      <Section title="Waveform Current Position">
        <Sub>Waveform Current Position displayed on CDJ/XDJ</Sub>
        <Radios
          label="Waveform Current Position displayed on CDJ/XDJ"
          nested
          value={dj.waveformPosition}
          choices={[
            { value: "left", label: "LEFT" },
            { value: "center", label: "CENTER" },
          ]}
          onChange={(waveformPosition) => set({ waveformPosition })}
        />
      </Section>
      <Section title="Type of the Overview Waveform">
        <Sub>Type of the Overview Waveform displayed on CDJ/XDJ</Sub>
        <Radios
          label="Type of the Overview Waveform displayed on CDJ/XDJ"
          nested
          value={dj.overviewWaveform}
          choices={[
            { value: "half", label: "Half Waveform" },
            { value: "full", label: "Full Waveform" },
          ]}
          onChange={(overviewWaveform) => set({ overviewWaveform })}
        />
      </Section>
      <Section title="Key display format">
        <Sub>The key display format displayed on CDJ/XDJ.</Sub>
        <Radios
          label="The key display format displayed on CDJ/XDJ"
          nested
          value={dj.keyDisplay}
          choices={[
            { value: "classic", label: "Classic" },
            { value: "alphanumeric", label: "Alphanumeric" },
          ]}
          onChange={(keyDisplay) => set({ keyDisplay })}
        />
        <Note>
          Written to a stick&rsquo;s DEVSETTING.DAT the first time it is
          exported to, with the three choices above.
        </Note>
      </Section>
    </>
  );
}

function ColumnSection({ sorts, subColumn, onChange }: {
  sorts: readonly MenuSlot[];
  subColumn: number | null;
  onChange: (subColumn: number | null) => void;
}) {
  // DEFAULT and ALPHABET are how a list is ordered, not something to show
  // beside a title, so they are left out.
  const choices = useMemo(
    () => [
      { value: "", label: "Not Specified" },
      ...sorts.filter((s) => !isFixed("sort", s.menuItem)).map((s) => ({
        value: String(s.menuItem),
        label: displayName(s),
      })),
    ],
    [sorts],
  );
  return (
    <Section title="Column">
      <Sub>Select item which is shown next to track name on CDJ/XDJ</Sub>
      <Select
        label="Default right column"
        nested
        value={subColumn === null ? "" : String(subColumn)}
        choices={choices}
        onChange={(value) => onChange(value === "" ? null : Number(value))}
      />
    </Section>
  );
}

/** The dropdown's value for "no interface chosen". */
const AUTOMATIC = "";

/**
 * PRO DJ LINK: the LINK switch, the interface it runs on, and the players
 * on the network with what each has loaded from us.
 *
 * The interface is a preference, so the strip's LINK button honours it too;
 * "Automatic" leaves the choice to the app, which takes the interface the
 * players are reached through. It is changed with LINK off: a session is
 * bound to its interface for as long as it runs.
 *
 * Status arrives by event as it changes and is read once on open; the
 * session itself outlives the pane, as LINK does — a source that vanished
 * when Preferences closed would be no source at all.
 */
function LinkSection({ linkInterface, onChoose }: {
  linkInterface: string | null;
  onChoose: (name: string | null) => void;
}) {
  const [link, setLink] = useState<LinkStatus | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    let stop: (() => void) | undefined;
    void (async () => {
      const backend = await getBackend();
      if (!live) return;
      stop = backend.onLinkStatus((status) => {
        if (live) setLink(status);
      });
      const status = await backend.linkStatus();
      if (live) setLink(status);
    })();
    return () => {
      live = false;
      stop?.();
    };
  }, []);

  const interfaces = link?.interfaces ?? [];
  // A chosen interface that is not there right now (unplugged, renamed) is
  // kept in the store and shown as such, not silently swapped for another.
  const choices = [
    { value: AUTOMATIC, label: "Automatic" },
    ...interfaces.map((i) => ({ value: i.name, label: `${i.name} — ${i.address}` })),
  ];
  if (linkInterface !== null && !interfaces.some((i) => i.name === linkInterface)) {
    choices.push({ value: linkInterface, label: `${linkInterface} — not present` });
  }

  const toggle = () => {
    setBusy(true);
    void (async () => {
      const backend = await getBackend();
      try {
        setLink(link?.on ? await backend.stopLinkExport() : await backend.startLinkExport(linkInterface ?? undefined));
      } finally {
        setBusy(false);
      }
    })();
  };

  return (
    <Section title="PRO DJ LINK" label="Link">
      <div className={styles.actions}>
        <Button onClick={toggle} disabled={busy || link === null}>
          {link?.on ? "Disconnect" : "Connect to PRO DJ LINK"}
        </Button>
      </div>
      <Select
        label="Network interface"
        caption="Network interface"
        plain
        value={linkInterface ?? AUTOMATIC}
        disabled={link === null || link.on}
        choices={choices}
        onChange={(value) => onChoose(value === AUTOMATIC ? null : value)}
      />
      {link === null ? null : link.on ? (
        <>
          <Note>
            On as <b>rekordbox</b>
            {link.interface ? ` on ${link.interface.name} (${link.interface.address})` : ""}.
            Players list the library under LINK.
          </Note>
          {link.players.length === 0 ? (
            <Note>No players have announced themselves yet.</Note>
          ) : (
            <ul className={styles.list} aria-label="Players on the link">
              {link.players.map((player) => (
                <li key={player.number}>
                  <span className={styles.listTitle}>
                    {player.name} — {player.kind} {player.number}
                    {player.master ? " · MASTER" : ""}
                  </span>
                  <span className={styles.listPath}>
                    {player.loaded
                      ? `${player.playing ? "Playing" : "Loaded"}: ${player.loaded.title}` +
                        (player.loaded.artist ? ` — ${player.loaded.artist}` : "")
                      : `Nothing of ours loaded · ${player.address}`}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </>
      ) : link.problem ? (
        <Note failed>{link.problem}</Note>
      ) : null}
    </Section>
  );
}
