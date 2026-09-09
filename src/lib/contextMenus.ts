/**
 * The two context menus, transcribed from rekordbox rather than designed.
 *
 * Every label is a left-hand key of `german.lang`, and the order, the
 * separators and which entries carry a submarker are the capture's. Items the
 * app cannot do yet are still drawn, greyed, exactly as rekordbox greys the
 * ones a given selection cannot do — a menu that is half the length of the
 * real one is a menu people have to relearn later.
 */

/** What an entry does, or `null` for one that is only drawn. */
export type TrackAction =
  | "analyse"
  | "removeFromPlaylist"
  | "showInformation"
  | "showInFinder";

export type TreeAction =
  | "export"
  | "createPlaylist"
  | "createFolder"
  | "delete";

export interface MenuEntry<A> {
  /** The label, as `german.lang` spells it. */
  label: string;
  /** What it runs. `null` is drawn and greyed: rekordbox has it, we do not. */
  action: A | null;
  /** Opens a submenu, drawn with rekordbox's arrow. */
  submenu?: boolean;
  /** A rule beyond "we have it": no playlist to remove from, and so on. */
  needs?: "playlist" | "file";
}

/** A separator between groups. */
export const SEPARATOR = "-" as const;

export type MenuRow<A> = MenuEntry<A> | typeof SEPARATOR;

/**
 * Right-clicking a track, top to bottom as the capture has it.
 *
 * The greyed entries are not oversights. `Load` needs a deck to load *into*,
 * which is the 2-player view; the tag list, iTunes, the cloud and the play
 * count are features this does not have; `Remove from Collection` is a write
 * to the shared library that no recording has pinned down yet.
 */
export const TRACK_MENU: readonly MenuRow<TrackAction>[] = [
  { label: "Load", action: null, submenu: true },
  SEPARATOR,
  { label: "Import To Collection", action: null },
  { label: "Analyze Track", action: "analyse" },
  { label: "Analysis Lock", action: null, submenu: true },
  SEPARATOR,
  // Its submenu is the playlist tree, and there is no submenu here yet, so it
  // is drawn with rekordbox's arrow and greyed like the others.
  { label: "Add To Playlist", action: null, submenu: true },
  { label: "Add To Tag List", action: null },
  { label: "Reload Tag", action: null },
  { label: "Get Info from iTunes", action: null },
  { label: "Track Type", action: null, submenu: true },
  SEPARATOR,
  { label: "Cloud Library Sync", action: null, submenu: true },
  { label: "Export Track", action: null, submenu: true },
  SEPARATOR,
  { label: "Auto Load Hot Cue", action: null, submenu: true },
  { label: "Reset DJ Play Count", action: null },
  { label: "Add New Analysis Data", action: null },
  SEPARATOR,
  { label: "Remove from Playlist", action: "removeFromPlaylist", needs: "playlist" },
  { label: "Remove from Collection", action: null },
  { label: "Remove from History", action: null },
  SEPARATOR,
  { label: "Show information", action: "showInformation" },
  { label: "Show in Finder", action: "showInFinder", needs: "file" },
  SEPARATOR,
  { label: "Track information", action: null, submenu: true },
];

/**
 * Right-clicking the tree.
 *
 * `Delete` takes the node's own word — rekordbox writes "Delete Folder" over a
 * folder and "Delete Playlist" over a playlist, so this does too.
 */
export function treeMenu(kind: "playlist" | "folder"): readonly MenuRow<TreeAction>[] {
  const folder = kind === "folder";
  return [
    { label: "Batch Auto Upload setting", action: null },
    { label: folder ? "Export Folder" : "Export Playlist", action: "export", submenu: true },
    SEPARATOR,
    { label: "Create New Playlist", action: "createPlaylist" },
    { label: "Create New Intelligent Playlist", action: null },
    { label: "Create New Folder", action: "createFolder" },
    SEPARATOR,
    { label: "Playlist display setting", action: null },
    SEPARATOR,
    { label: folder ? "Delete Folder" : "Delete Playlist", action: "delete" },
    SEPARATOR,
    { label: "Sort Items", action: null },
    SEPARATOR,
    { label: "Add To Shortcut", action: null },
    SEPARATOR,
    { label: "Cancel sharing of all collaborative playlists", action: null },
  ];
}

/** What is in a menu, for the caller to decide what an entry can do. */
export interface MenuContext {
  /** The view is a playlist, so a track can be taken out of it. */
  inPlaylist: boolean;
  /** The track's file is known, so the OS can be asked to show it. */
  hasFile: boolean;
  /** The library is open read-only, because rekordbox is running. */
  readOnly: boolean;
}

/** Writes to the shared library, so rekordbox running is a refusal. */
const WRITES: ReadonlySet<string> = new Set([
  "removeFromPlaylist",
  "createPlaylist",
  "createFolder",
  "delete",
]);

/** Whether an entry can be clicked. Everything else is drawn and greyed. */
export function enabled<A extends string>(
  entry: MenuEntry<A>,
  context: MenuContext,
): boolean {
  if (entry.action === null) return false;
  if (context.readOnly && WRITES.has(entry.action)) return false;
  if (entry.needs === "playlist") return context.inPlaylist;
  if (entry.needs === "file") return context.hasFile;
  return true;
}

/** The entries of a menu, without its separators. */
export function entriesOf<A>(rows: readonly MenuRow<A>[]): MenuEntry<A>[] {
  return rows.filter((row): row is MenuEntry<A> => row !== SEPARATOR);
}
