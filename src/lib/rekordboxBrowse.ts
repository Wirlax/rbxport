/** Import the parts of rekordbox's browseSetting.xml that our rows support. */
import { AVAILABLE_COLUMNS, sanitise, type ColumnKey, type Layout } from "./columns";
import { loadPreferences } from "./preferences";
import { getBackend } from "@/ipc/client";
import { loadSession, saveSession } from "./session";

export type BrowseContext = "collection" | "playlist" | "history" | "subBrowser" | "folder";

// Verified numeric IDs from the measured rekordbox headers. Unknown IDs must
// stay unknown: the XML has numeric IDs only, so guessing would map a visible
// rekordbox field onto the wrong RBXport column.
const IDS: Readonly<Record<string, ColumnKey>> = {
  "20": "trackNo", "21": "title", "22": "artist", "24": "genre", "25": "comment",
  "27": "rating", "29": "bpm", "34": "key", "44": "duration", "53": "releaseDate",
  "60": "artwork", "68": "preview",
};

const SOURCES: Readonly<Record<BrowseContext, readonly string[]>> = {
  collection: ["TableHeader-CollectionTracks"],
  playlist: ["TableHeader-PlaylistTracks"],
  history: ["TableHeader-HistoryTracks"],
  subBrowser: ["TableHeader-PlaylistTracks-sub", "TableHeader-CollectionTracks-sub"],
  folder: ["TableHeader-FolderTracks"],
};

export function parseRekordboxBrowse(xml: string): Partial<Record<BrowseContext, Layout>> {
  const doc = new DOMParser().parseFromString(xml, "application/xml");
  if (doc.querySelector("parsererror")) return {};
  const values = Array.from(doc.getElementsByTagName("VALUE"));
  const layouts: Partial<Record<BrowseContext, Layout>> = {};
  for (const context of Object.keys(SOURCES) as BrowseContext[]) {
    const value = SOURCES[context]
      .map((name) => values.find((entry) => entry.getAttribute("name") === name))
      .find((entry) => entry?.getElementsByTagName("COLUMN").length);
    if (!value) continue;
    const order: ColumnKey[] = [];
    const widths: Layout["widths"] = {};
    for (const column of Array.from(value.getElementsByTagName("COLUMN"))) {
      const key = IDS[column.getAttribute("id") ?? ""];
      if (!key || !AVAILABLE_COLUMNS.includes(key)) continue;
      const width = Number(column.getAttribute("width"));
      if (Number.isFinite(width) && width > 0) widths[key] = width;
      if (key !== "trackNo" && column.getAttribute("visible") === "1" && !order.includes(key)) order.push(key);
    }
    if (order.includes("title")) layouts[context] = sanitise({ order, widths });
  }
  return layouts;
}

/** Main tree and second browser widths from BROWSELAYOUT, when usable. */
export function parseRekordboxBrowseWidths(xml: string): { treeWidth?: number; subWidth?: number } {
  const doc = new DOMParser().parseFromString(xml, "application/xml");
  if (doc.querySelector("parsererror")) return {};
  const browse = Array.from(doc.getElementsByTagName("BROWSE"));
  const width = (component: string): number | undefined => {
    const raw = browse.find((item) => item.getAttribute("comp") === component)?.getAttribute("w");
    const value = Number(raw);
    return raw && Number.isFinite(value) && value >= 100 && value <= 4000 ? Math.round(value) : undefined;
  };
  const treeWidth = width("TreeView");
  const subWidth = width("SubBrowse");
  return {
    ...(treeWidth === undefined ? {} : { treeWidth }),
    ...(subWidth === undefined ? {} : { subWidth }),
  };
}

/** Run before the main window mounts, so useColumns reads the imported layout. */
export async function syncRekordboxBrowseAtStartup(): Promise<void> {
  if (!loadPreferences().rekordbox.syncBrowseSettings) return;
  try {
    const xml = await (await getBackend()).rekordboxBrowseSettings();
    if (!xml) return;
    for (const [context, layout] of Object.entries(parseRekordboxBrowse(xml))) {
      localStorage.setItem(`rbl.columns.v2.${context}`, JSON.stringify(layout));
    }
    const widths = parseRekordboxBrowseWidths(xml);
    if (widths.treeWidth !== undefined || widths.subWidth !== undefined) {
      saveSession({ ...loadSession(), ...widths });
    }
  } catch {
    // A missing or unreadable rekordbox install leaves local layouts intact.
  }
}
