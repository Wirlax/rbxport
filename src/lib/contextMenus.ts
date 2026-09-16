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
  | "showInFinder"
  | "loadPlayer1"
  | "loadPlayer2";

export type TreeAction =
  | "export"
  | "createPlaylist"
  | "createFolder"
  | "rename"
  | "delete";

/** The deck's ≡ menu: the choices it changes, and the one thing it does. */
export type DeckAction =
  | "waveformBlue"
  | "waveformRgb"
  | "waveform3band"
  | "analyse"
  | "beatPosition"
  | "beatToMemoryBars"
  | "beatToMemoryBeats"
  | "waveformClickOn"
  | "waveformClickOff";

export interface MenuEntry<A> {
  /** The label, as `german.lang` spells it. */
  label: string;
  /** What it runs. `null` is drawn and greyed: rekordbox has it, we do not. */
  action: A | null;
  /** Opens a submenu, drawn with rekordbox's arrow. */
  submenu?: boolean;
  /**
   * The submenu's own rows, when there is one behind the arrow.
   *
   * Absent with `submenu` set is rekordbox's arrow over nothing: the entry is
   * drawn and greyed, because a menu missing half its rows is a menu people
   * have to relearn later.
   */
  items?: readonly MenuRow<A>[];
  /** A rule beyond "we have it": no playlist to remove from, and so on. */
  needs?: "playlist" | "file";
  /** In a submenu of choices, the one in force: drawn with a tick. */
  checked?: boolean;
}

/** A separator between groups. */
export const SEPARATOR = "-" as const;

export type MenuRow<A> = MenuEntry<A> | typeof SEPARATOR;

/**
 * Right-clicking a track, top to bottom as the capture has it.
 *
 * The greyed entries are not oversights: the tag list, iTunes and the play
 * count are features this does not have, and `Remove from Collection` is a
 * write to the shared library that no recording has pinned down yet. `Load`
 * is greyed here and filled in by `trackMenu` below, which knows how many
 * players the layout is drawing.
 *
 * The cloud entries are gone rather than greyed: there is no cloud library
 * behind this app to sync with, so drawing the row promises a feature that is
 * not coming. `Analyze Track` is greyed deliberately — the engine is still
 * there, but analysis is not trustworthy enough to offer yet.
 */
export const TRACK_MENU: readonly MenuRow<TrackAction>[] = [
  { label: "Load", action: null, submenu: true },
  SEPARATOR,
  { label: "Import To Collection", action: null },
  { label: "Analyze Track", action: null },
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
  { label: "Export Track", action: null, submenu: true },
  SEPARATOR,
  { label: "Auto Load Hot Cue", action: null, submenu: true },
  { label: "Reset DJ Play Count", action: null },
  { label: "Add New Analysis Data", action: null },
  { label: "Convert Memory Cues to Hot Cues", action: null },
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
 * Right-clicking the tree, top to bottom as the capture has it
 * (docs/screenshots context-menu-tree@2x, over a playlist).
 *
 * `Delete` and `Export` take the node's own word — rekordbox writes "Delete
 * Folder" over a folder and "Delete Playlist" over a playlist, so this does
 * too. Only the playlist menu is captured; the folder's is assumed to be the
 * same list with those two words changed [ASSUME].
 *
 * The greyed entries are artwork, the intelligent playlist, the file export,
 * sharing and the shortcut list: rekordbox has them, this does not.
 *
 * The cloud rows rekordbox draws first — "Cloud Library Sync", "Auto Upload"
 * and "Batch Auto Upload setting" — are left out rather than greyed. There is
 * no cloud library behind this app, so the rows would promise a feature that
 * is not coming, which is a different thing from one not built yet.
 */
export function treeMenu(kind: "playlist" | "folder"): readonly MenuRow<TreeAction>[] {
  const folder = kind === "folder";
  return [
    { label: folder ? "Export Folder" : "Export Playlist", action: "export", submenu: true },
    SEPARATOR,
    { label: "Create New Playlist", action: "createPlaylist" },
    { label: "Create New Intelligent Playlist", action: null },
    { label: "Create New Folder", action: "createFolder" },
    SEPARATOR,
    { label: "Playlist display setting", action: null },
    SEPARATOR,
    { label: "Add Artwork", action: null },
    SEPARATOR,
    // Rename is not in the capture [ASSUME]: rekordbox renames from a double
    // click on the row. It sits with Delete because the two are the same kind
    // of thing — the node itself rather than what is in it.
    { label: folder ? "Rename Folder" : "Rename Playlist", action: "rename" },
    { label: folder ? "Delete Folder" : "Delete Playlist", action: "delete" },
    SEPARATOR,
    { label: "Export a playlist to a file", action: null, submenu: true },
    SEPARATOR,
    { label: "Collaborative playlist", action: null, submenu: true },
    SEPARATOR,
    { label: "Add To Shortcut", action: null },
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

/** What the deck's menu shows, so it can tick what is in force. */
export interface DeckMenuState {
  waveformColor: "blue" | "rgb" | "3band";
  beatCount: "position" | "toMemoryBars" | "toMemoryBeats";
  waveformClick: boolean;
}

/**
 * The ≡ at the foot of the deck, top to bottom as rekordbox draws it
 * (capture of 7.2.11's player menu). The waveform colour, the beat count and
 * the waveform click are Preferences › View's own choices, reachable from
 * here as well; each submenu ticks what is in force. Export Track wants a
 * single track added to a stick, which the export cannot yet do without
 * rewriting the stick; Export Loop As WAV and Active Loop Playback need
 * loops, which are not built. All three are drawn greyed, as rekordbox
 * greys Export Loop As WAV with nothing to export. Analyze Track is greyed
 * for the reason `TRACK_MENU` gives: the engine is there, the result is not
 * trustworthy enough to offer.
 */
export function deckMenu(state: DeckMenuState): readonly MenuRow<DeckAction>[] {
  const tick = (on: boolean) => ({ checked: on });
  return [
    {
      label: "Change waveform color",
      action: null,
      items: [
        { label: "BLUE", action: "waveformBlue", ...tick(state.waveformColor === "blue") },
        { label: "RGB", action: "waveformRgb", ...tick(state.waveformColor === "rgb") },
        { label: "3Band", action: "waveform3band", ...tick(state.waveformColor === "3band") },
      ],
    },
    { label: "Analyze Track", action: null },
    SEPARATOR,
    {
      label: "Beat Count Display",
      action: null,
      items: [
        { label: "Current Position (Bars)", action: "beatPosition", ...tick(state.beatCount === "position") },
        {
          label: "Count to the next MEMORY CUE (Bars)",
          action: "beatToMemoryBars",
          ...tick(state.beatCount === "toMemoryBars"),
        },
        {
          label: "Count to the next MEMORY CUE (Beats)",
          action: "beatToMemoryBeats",
          ...tick(state.beatCount === "toMemoryBeats"),
        },
      ],
    },
    SEPARATOR,
    { label: "Export Track", action: null, submenu: true },
    SEPARATOR,
    { label: "Export Loop As WAV", action: null },
    SEPARATOR,
    { label: "Active Loop Playback", action: null, submenu: true },
    SEPARATOR,
    {
      label: "Click on the waveform for PLAY and CUE",
      action: null,
      // The submenu's wording is [ASSUME]: the capture shows the arrow and
      // not what is behind it. Preferences calls the switch "Disable".
      items: [
        { label: "Enable", action: "waveformClickOn", ...tick(state.waveformClick) },
        { label: "Disable", action: "waveformClickOff", ...tick(!state.waveformClick) },
      ],
    },
  ];
}

/** Writes to the shared library, so rekordbox running is a refusal. */
const WRITES: ReadonlySet<string> = new Set([
  "removeFromPlaylist",
  "createPlaylist",
  "createFolder",
  "rename",
  "delete",
]);

/** Whether an entry can be clicked. Everything else is drawn and greyed. */
export function enabled<A extends string>(
  entry: MenuEntry<A>,
  context: MenuContext,
): boolean {
  // An entry that opens a submenu does nothing itself; what makes it live is
  // having something under it that is.
  if (entry.items) return entriesOf(entry.items).some((row) => enabled(row, context));
  if (entry.action === null) return false;
  if (context.readOnly && WRITES.has(entry.action)) return false;
  if (entry.needs === "playlist") return context.inPlaylist;
  if (entry.needs === "file") return context.hasFile;
  return true;
}

/**
 * The track menu for a layout drawing `players` decks.
 *
 * rekordbox lists players 1 to 4 under `Load` whether or not they are on
 * screen; this lists the ones there are, because a player that is not drawn
 * has nowhere to put a track. With none — the Full Browser layout — `Load` is
 * the greyed arrow it is in `TRACK_MENU`.
 *
 * The labels are `german.lang`'s own keys: "Load track to player 1".
 */
export function trackMenuFor(players: number): readonly MenuRow<TrackAction>[] {
  const every: MenuRow<TrackAction>[] = [
    { label: "Load track to player 1", action: "loadPlayer1" },
    { label: "Load track to player 2", action: "loadPlayer2" },
  ];
  const decks = every.slice(0, Math.max(0, players));
  if (decks.length === 0) return TRACK_MENU;
  return TRACK_MENU.map((row) =>
    row !== SEPARATOR && row.label === "Load" ? { ...row, items: decks } : row,
  );
}

/** The entries of a menu, without its separators. */
export function entriesOf<A>(rows: readonly MenuRow<A>[]): MenuEntry<A>[] {
  return rows.filter((row): row is MenuEntry<A> => row !== SEPARATOR);
}
