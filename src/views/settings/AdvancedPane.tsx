/**
 * Advanced: Database, Browse and Others. Captures docs/screenshots 9.49.32
 * to 9.49.51 PM.
 *
 * Database holds the library's own facts and the missing-file manager,
 * with rekordbox's Auto Relocate Search Folders feeding it. Browse holds
 * Library Protection and Edit Library. Others holds BEAT/BPM SYNC and the
 * quantize beat value.
 *
 * Not here: iTunes and rekordbox xml (neither is read), Auto Export and
 * Database management (neither is built), My Tag, colour names, display
 * speed, the long-press menu and the Tag List (none exist here), the
 * export name (link export is not built), hot cue GATE, loop export,
 * Recordings, and every streaming service.
 */
import { useState } from "react";

import { getBackend } from "@/ipc/client";
import type { LibrarySummary, MissingTracks, RelocateReport } from "@/ipc/types";
import { QUANTIZE_BEATS } from "@/lib/preferences";
import { usePreferencesContext } from "@/store/usePreferences";
import styles from "./Preferences.module.css";
import { Button, Note, Radios, Section, Select, Sub, Toggle } from "./controls";

export type AdvancedTab = "database" | "browse" | "others";

export const ADVANCED_TABS: readonly { id: AdvancedTab; label: string }[] = [
  { id: "database", label: "Database" },
  { id: "browse", label: "Browse" },
  { id: "others", label: "Others" },
];

/** How many missing tracks to list. The count above it is exact. */
const MISSING_SHOWN = 20;

export function AdvancedPane({ tab, summary }: { tab: AdvancedTab; summary: LibrarySummary | null }) {
  const { preferences, update } = usePreferencesContext();
  const advanced = preferences.advanced;
  const set = (patch: Partial<typeof advanced>) => update("advanced", patch);

  if (tab === "browse") {
    return (
      <>
        <Section title="Library Protection">
          <Toggle
            label="Protect library edit."
            checked={advanced.protectLibrary}
            onChange={(protectLibrary) => set({ protectLibrary })}
          />
          <Note>
            Every edit is refused while this is on — ratings, comments, cues,
            playlists — as they already are while rekordbox is running.
          </Note>
        </Section>
        <Section title="Edit Library">
          <Toggle
            label="Double-click to edit"
            checked={advanced.doubleClickToEdit}
            onChange={(doubleClickToEdit) => set({ doubleClickToEdit })}
          />
          <Note>
            {advanced.doubleClickToEdit
              ? "A comment is edited by double-clicking it; a double-click on the rest of the row loads the track."
              : "A comment is edited by clicking it on a row that is already selected."}
          </Note>
        </Section>
      </>
    );
  }

  if (tab === "others") {
    return (
      <>
        <Section title="QUANTIZE BEAT VALUE">
          <Select
            label="Quantize beat value"
            value={advanced.quantizeBeat}
            choices={QUANTIZE_BEATS.map((value) => ({ value, label: value }))}
            onChange={(quantizeBeat) => set({ quantizeBeat })}
          />
        </Section>
        <Section title="BEAT/BPM SYNC">
          <Sub>Sync Type</Sub>
          <Radios
            label="Sync Type"
            nested
            value={advanced.syncType}
            choices={[
              { value: "beat", label: "BEAT SYNC" },
              { value: "bpm", label: "BPM SYNC" },
            ]}
            onChange={(syncType) => set({ syncType })}
          />
          <Toggle
            label="Allow BEAT/BPM SYNC with double/half BPM."
            nested
            checked={advanced.syncDoubleHalf}
            onChange={(syncDoubleHalf) => set({ syncDoubleHalf })}
          />
        </Section>
      </>
    );
  }

  return (
    <>
      <Section title="Library">
        <dl className={styles.facts}>
          <dt>Tracks</dt>
          <dd>{summary ? summary.trackCount.toLocaleString() : "—"}</dd>
          <dt>Playlists</dt>
          <dd>{summary ? summary.playlistCount.toLocaleString() : "—"}</dd>
          <dt>Database version</dt>
          <dd>{summary?.dbVersion ?? "—"}</dd>
          <dt>Editing</dt>
          <dd>
            {summary?.readOnly
              ? "Read-only — rekordbox is running"
              : advanced.protectLibrary
                ? "Protected — see Browse"
                : "Available"}
          </dd>
        </dl>
        <Note>
          Changes are refused while rekordbox is running, because it holds the
          database open. Quit it to edit.
        </Note>
      </Section>
      <RelocateSection
        folders={advanced.relocateFolders}
        onFolders={(relocateFolders) => set({ relocateFolders })}
        readOnly={(summary?.readOnly ?? false) || advanced.protectLibrary}
      />
    </>
  );
}

function RelocateSection({ folders, onFolders, readOnly }: {
  folders: readonly string[];
  onFolders: (folders: string[]) => void;
  readOnly: boolean;
}) {
  const [missing, setMissing] = useState<MissingTracks | null>(null);
  const [scanning, setScanning] = useState(false);
  const [picked, setPicked] = useState<string>(folders[0] ?? "");
  const [report, setReport] = useState<RelocateReport | null>(null);
  const current = folders.includes(picked) ? picked : (folders[0] ?? "");

  const rescan = async () => {
    const backend = await getBackend();
    setMissing(await backend.missingTracks(MISSING_SHOWN));
  };

  return (
    <>
      <Section title="Auto Relocate Search Folders">
        <Sub>Specified user folders</Sub>
        <div className={styles.actions}>
          <select
            className={styles.select}
            data-plain
            aria-label="Search folders"
            value={current}
            onChange={(e) => setPicked(e.target.value)}
          >
            {folders.length === 0 ? <option value="">No folders</option> : null}
            {folders.map((folder) => (
              <option key={folder} value={folder}>{folder}</option>
            ))}
          </select>
          <Button
            onClick={() => {
              void (async () => {
                const backend = await getBackend();
                const folder = await backend.pickFolder("Choose a folder to search for moved files");
                if (folder === null || folders.includes(folder)) return;
                onFolders([...folders, folder]);
                setPicked(folder);
              })();
            }}
          >
            Add
          </Button>
          <Button disabled={current === ""} onClick={() => onFolders(folders.filter((f) => f !== current))}>
            Del
          </Button>
        </div>
        <Note>
          A missing track is looked for in these folders by its file name,
          and pointed at the first file found.
        </Note>
      </Section>

      <Section title="Missing files">
        {missing === null ? (
          <>
            <div className={styles.actions}>
              <Button
                disabled={scanning}
                onClick={() => {
                  setScanning(true);
                  void rescan().finally(() => setScanning(false));
                }}
              >
                {scanning ? "Checking…" : "Check for missing files"}
              </Button>
            </div>
            <Note>
              Checks every track&rsquo;s file, which takes a moment on a large
              library. Nothing is changed by looking.
            </Note>
          </>
        ) : missing.total === 0 ? (
          <Note>Every track&rsquo;s file is where the library expects it.</Note>
        ) : (
          <>
            <Note>
              {missing.total.toLocaleString()} track{missing.total === 1 ? "" : "s"} cannot be found.
            </Note>
            <div className={styles.actions}>
              <Button
                disabled={readOnly || folders.length === 0 || scanning}
                onClick={() => {
                  setScanning(true);
                  void (async () => {
                    try {
                      const backend = await getBackend();
                      setReport(await backend.autoRelocate([...folders]));
                      await rescan();
                    } finally {
                      setScanning(false);
                    }
                  })();
                }}
              >
                {scanning ? "Searching…" : "Auto Relocate"}
              </Button>
            </div>
            {report ? (
              <Note>
                {report.relocated} relocated
                {report.unresolved > 0 ? `, ${report.unresolved} not found in the search folders.` : "."}
              </Note>
            ) : folders.length === 0 ? (
              <Note>Add a search folder above to relocate them automatically.</Note>
            ) : null}
            <ul className={styles.list}>
              {missing.tracks.map((track) => (
                <li key={track.id}>
                  <span className={styles.listTitle}>
                    {track.title}
                    {track.artist ? ` — ${track.artist}` : ""}
                  </span>
                  <span className={styles.listPath}>{track.path}</span>
                  <button
                    type="button"
                    className={styles.listAction}
                    disabled={readOnly}
                    onClick={() => {
                      void (async () => {
                        const backend = await getBackend();
                        const chosen = await backend.relocateTrack(track.id);
                        // Cancelling leaves the list alone; a successful
                        // relocate means the track is no longer missing.
                        if (chosen === null) return;
                        await rescan();
                      })();
                    }}
                  >
                    Locate&hellip;
                  </button>
                </li>
              ))}
            </ul>
            {missing.total > missing.tracks.length ? (
              <Note>Showing the first {missing.tracks.length}.</Note>
            ) : null}
            <Note>
              Locating a track points it at a new file. Its analysis, cues and
              playlists all key off the track rather than the path, so they
              stay as they are.
            </Note>
          </>
        )}
      </Section>
    </>
  );
}
