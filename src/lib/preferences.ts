/**
 * The Preferences window's choices, and how they are kept.
 *
 * Every field here drives something the application actually does; a choice
 * with nothing behind it is not stored, because a switch that changes nothing
 * is worse than no switch. The layout of rekordbox's window — which pane and
 * tab a choice sits under — is the window's business, not this module's.
 *
 * Stored in localStorage beside the session, and checked on the way back in
 * the same way: a hand-edited value, a value from a build that spelt a choice
 * differently, or nothing at all must each come back as a working set.
 */
import { toCamelot, type TrafficLightReach } from "./camelot";
import type {
  KeyDisplay, MenuSlot, OverviewWaveform, WaveformColor, WaveformPosition,
} from "@/ipc/types";

/** How fast the deck redraws while it plays. */
export type WaveformRate = "high" | "medium" | "low";

/** BEAT SYNC matches the tempo and the bar; BPM SYNC the tempo alone. */
export type SyncType = "beat" | "bpm";

/** The fraction of a beat the quantized cue snaps to. */
export type QuantizeBeat = "1/1" | "1/2" | "1/4" | "1/8";

export const QUANTIZE_BEATS: readonly QuantizeBeat[] = ["1/1", "1/2", "1/4", "1/8"];

/** How many beats one quantize step is: `1/4` is a quarter of a beat. */
export function quantizeFraction(value: QuantizeBeat): number {
  switch (value) {
    case "1/2": return 0.5;
    case "1/4": return 0.25;
    case "1/8": return 0.125;
    default: return 1;
  }
}

/**
 * The five stops of the FontSize and Line Space sliders, as multiples of the
 * measured token. The middle stop is the measured size; the others are
 * [ASSUME] — rekordbox's own steps have not been measured, only that the
 * slider has a small end and a large one.
 */
export const BROWSE_SCALE_STEPS = 5;
export const BROWSE_SCALES: readonly number[] = [0.8, 0.9, 1, 1.15, 1.3];
export const BROWSE_SCALE_DEFAULT = 2;

export function browseScale(step: number): number {
  return BROWSE_SCALES[step] ?? 1;
}

export interface ViewPreferences {
  /** Show Tooltips. */
  tooltips: boolean;
  /** Browse › FontSize, as a slider stop 0 to 4. */
  browseFontSize: number;
  /** Browse › Bold. */
  browseBold: boolean;
  /** Browse › Line Space, as a slider stop 0 to 4. */
  browseLineSpace: number;
  /** Key display format: `Ebm` or `2A`. */
  keyDisplay: KeyDisplay;
  /** Waveform Drawing Rate. */
  waveformRate: WaveformRate;
  /** Full/Preview Waveform: the deck's overview, single-sided or mirrored. */
  overviewWaveform: OverviewWaveform;
  /** Media Browser › Explorer: the folders on disk in the tree. */
  explorer: boolean;
  /** Browser panel › Display Cue Markers on Preview. */
  previewCueMarkers: boolean;
  /** Browser panel › Display All Tracks in the Playlists. */
  allTracks: boolean;
  /** Browser panel › Display the number of tracks in a playlist on the Tree View. */
  playlistCounts: boolean;
  /** Phrases › Phrase (Full Waveform): the phrase strip over the overview. */
  phraseFull: boolean;
  /** Phrases › Always show types of phrases: the labels in that strip. */
  phraseLabels: boolean;
  /** Vocal › Vocal (Full Waveform): the vocal strip under the overview. */
  vocalFull: boolean;
  /** Traffic Light: how far around the loaded track's key the browser lights. */
  trafficLight: TrafficLightReach;
}

export interface AnalysisPreferences {
  /** Auto Analysis: analyse a track when it is added to the library. */
  auto: boolean;
}

/**
 * DJ System: what a stick gets on its first export. A stick that already
 * carries settings keeps its own, which its device panel edits.
 */
export interface DjSystemPreferences {
  waveformColor: WaveformColor;
  waveformPosition: WaveformPosition;
  overviewWaveform: OverviewWaveform;
  keyDisplay: KeyDisplay;
  /** The browse categories, or null for rekordbox's reference rows. */
  categories: MenuSlot[] | null;
  /** The sort options, or null for the reference rows. */
  sorts: MenuSlot[] | null;
  /** `menuItem` of the sort option shown beside the track name; null for none. */
  subColumn: number | null;
}

export interface AdvancedPreferences {
  /** Auto Relocate Search Folders › Specified user folders. */
  relocateFolders: string[];
  /** Library Protection: refuse every edit, whatever rekordbox is doing. */
  protectLibrary: boolean;
  /** Edit Library › Double-click to edit; off is a click on a selected row. */
  doubleClickToEdit: boolean;
  syncType: SyncType;
  /** Allow BEAT/BPM SYNC with double/half BPM. */
  syncDoubleHalf: boolean;
  quantizeBeat: QuantizeBeat;
}

export interface Preferences {
  view: ViewPreferences;
  analysis: AnalysisPreferences;
  djSystem: DjSystemPreferences;
  advanced: AdvancedPreferences;
}

export type PreferencePane = keyof Preferences;

/**
 * rekordbox's own defaults, as the captures of a fresh window show them
 * [OBS], except where the application differs on purpose: the browse scale
 * is the measured size, and a stick's rows default to the reference rows.
 */
export const DEFAULT_PREFERENCES: Preferences = {
  view: {
    tooltips: true,
    browseFontSize: BROWSE_SCALE_DEFAULT,
    browseBold: false,
    browseLineSpace: BROWSE_SCALE_DEFAULT,
    keyDisplay: "classic",
    waveformRate: "high",
    overviewWaveform: "half",
    explorer: true,
    previewCueMarkers: true,
    allTracks: true,
    playlistCounts: false,
    phraseFull: true,
    phraseLabels: true,
    vocalFull: true,
    trafficLight: "related3",
  },
  analysis: {
    auto: true,
  },
  djSystem: {
    waveformColor: "3band",
    waveformPosition: "center",
    overviewWaveform: "half",
    keyDisplay: "classic",
    categories: null,
    sorts: null,
    subColumn: null,
  },
  advanced: {
    relocateFolders: [],
    protectLibrary: false,
    doubleClickToEdit: false,
    syncType: "beat",
    syncDoubleHalf: true,
    quantizeBeat: "1/1",
  },
};

/** `Ebm` as the library stores it, or `2A` when the window says Alphanumeric. */
export function formatKey(key: string, display: KeyDisplay): string {
  if (display !== "alphanumeric" || key === "") return key;
  // A key the wheel does not know — a blank, "Unknown", a typo — is shown as
  // it is rather than hidden.
  return toCamelot(key) || key;
}

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function oneOf<T extends string>(value: unknown, choices: readonly T[], fallback: T): T {
  return choices.includes(value as T) ? (value as T) : fallback;
}

function step(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 && value < BROWSE_SCALE_STEPS
    ? value
    : fallback;
}

function strings(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string" && item !== "");
}

/** The stored rows, only if every one is a row; anything else is the reference. */
function slots(value: unknown): MenuSlot[] | null {
  if (!Array.isArray(value) || value.length === 0) return null;
  const ok = value.every(
    (s: unknown) =>
      typeof s === "object" && s !== null &&
      typeof (s as MenuSlot).id === "number" && typeof (s as MenuSlot).menuItem === "number" &&
      typeof (s as MenuSlot).name === "string" && typeof (s as MenuSlot).seq === "number" &&
      typeof (s as MenuSlot).visible === "boolean",
  );
  return ok ? (value as MenuSlot[]) : null;
}

const KEY_DISPLAYS: readonly KeyDisplay[] = ["classic", "alphanumeric"];
const OVERVIEWS: readonly OverviewWaveform[] = ["half", "full"];
const RATES: readonly WaveformRate[] = ["high", "medium", "low"];
const WAVEFORM_COLORS: readonly WaveformColor[] = ["blue", "rgb", "3band"];
const POSITIONS: readonly WaveformPosition[] = ["center", "left"];
const SYNC_TYPES: readonly SyncType[] = ["beat", "bpm"];
const REACHES: readonly TrafficLightReach[] = ["same", "related1", "related2", "related3"];

type Raw<T> = Partial<Record<keyof T, unknown>>;

function part<T>(value: unknown): Raw<T> {
  return typeof value === "object" && value !== null ? value : {};
}

/** Turns whatever was stored into a set the application can run on. */
export function sanitisePreferences(value: unknown): Preferences {
  const raw = part<Preferences>(value);
  const view = part<ViewPreferences>(raw.view);
  const analysis = part<AnalysisPreferences>(raw.analysis);
  const dj = part<DjSystemPreferences>(raw.djSystem);
  const advanced = part<AdvancedPreferences>(raw.advanced);
  const d = DEFAULT_PREFERENCES;
  return {
    view: {
      tooltips: bool(view.tooltips, d.view.tooltips),
      browseFontSize: step(view.browseFontSize, d.view.browseFontSize),
      browseBold: bool(view.browseBold, d.view.browseBold),
      browseLineSpace: step(view.browseLineSpace, d.view.browseLineSpace),
      keyDisplay: oneOf(view.keyDisplay, KEY_DISPLAYS, d.view.keyDisplay),
      waveformRate: oneOf(view.waveformRate, RATES, d.view.waveformRate),
      overviewWaveform: oneOf(view.overviewWaveform, OVERVIEWS, d.view.overviewWaveform),
      explorer: bool(view.explorer, d.view.explorer),
      previewCueMarkers: bool(view.previewCueMarkers, d.view.previewCueMarkers),
      allTracks: bool(view.allTracks, d.view.allTracks),
      playlistCounts: bool(view.playlistCounts, d.view.playlistCounts),
      phraseFull: bool(view.phraseFull, d.view.phraseFull),
      phraseLabels: bool(view.phraseLabels, d.view.phraseLabels),
      vocalFull: bool(view.vocalFull, d.view.vocalFull),
      trafficLight: oneOf(view.trafficLight, REACHES, d.view.trafficLight),
    },
    analysis: {
      auto: bool(analysis.auto, d.analysis.auto),
    },
    djSystem: {
      waveformColor: oneOf(dj.waveformColor, WAVEFORM_COLORS, d.djSystem.waveformColor),
      waveformPosition: oneOf(dj.waveformPosition, POSITIONS, d.djSystem.waveformPosition),
      overviewWaveform: oneOf(dj.overviewWaveform, OVERVIEWS, d.djSystem.overviewWaveform),
      keyDisplay: oneOf(dj.keyDisplay, KEY_DISPLAYS, d.djSystem.keyDisplay),
      categories: slots(dj.categories),
      sorts: slots(dj.sorts),
      subColumn: typeof dj.subColumn === "number" && Number.isInteger(dj.subColumn)
        ? dj.subColumn
        : null,
    },
    advanced: {
      relocateFolders: strings(advanced.relocateFolders),
      protectLibrary: bool(advanced.protectLibrary, d.advanced.protectLibrary),
      doubleClickToEdit: bool(advanced.doubleClickToEdit, d.advanced.doubleClickToEdit),
      syncType: oneOf(advanced.syncType, SYNC_TYPES, d.advanced.syncType),
      syncDoubleHalf: bool(advanced.syncDoubleHalf, d.advanced.syncDoubleHalf),
      quantizeBeat: oneOf(advanced.quantizeBeat, QUANTIZE_BEATS, d.advanced.quantizeBeat),
    },
  };
}

const KEY = "rbl.preferences";

/** Reads the stored preferences, falling back to the defaults on anything odd. */
export function loadPreferences(): Preferences {
  try {
    const raw = localStorage.getItem(KEY);
    return raw === null ? DEFAULT_PREFERENCES : sanitisePreferences(JSON.parse(raw));
  } catch {
    // A private window, cleared storage, or a browser that refuses it: the
    // defaults are a working window, so there is nothing to report.
    return DEFAULT_PREFERENCES;
  }
}

export function savePreferences(preferences: Preferences): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(preferences));
  } catch {
    // Not being able to remember a preference is not worth interrupting anyone.
  }
}
