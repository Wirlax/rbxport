/**
 * How much of the window the deck takes.
 *
 * rekordbox puts this in the top left corner, and it is a real layout switch
 * rather than a zoom: the browser is the same browser in every one of them,
 * and what changes is how many decks sit above it and how much of a deck each
 * of those shows.
 */
export type PlayerLayout = "one" | "two" | "dual" | "simple" | "browser";

export const LAYOUTS: readonly { id: PlayerLayout; label: string }[] = [
  { id: "one", label: "1 PLAYER" },
  { id: "two", label: "2 PLAYER" },
  { id: "dual", label: "DUAL PLAYER" },
  { id: "simple", label: "SIMPLE PLAYER" },
  { id: "browser", label: "FULL BROWSER" },
];

/** How many decks a layout draws. */
export function deckCount(layout: PlayerLayout): number {
  if (layout === "browser") return 0;
  if (layout === "two" || layout === "dual") return 2;
  return 1;
}

/**
 * Whether the two decks sit beside each other rather than one above the other.
 *
 * That is the difference the menu's own icons draw: 2 PLAYER is two stacked
 * bars, DUAL PLAYER two overlapping panels.
 */
export function isSideBySide(layout: PlayerLayout): boolean {
  return layout === "dual";
}

/**
 * Whether a layout draws the whole deck.
 *
 * The simple player keeps the transport and the waveforms and drops the pad
 * row and the cue list beside them — the parts you set a track up with, rather
 * than the parts you read it with.
 */
export function isFullDeck(layout: PlayerLayout): boolean {
  return layout !== "simple";
}

/** The label the dropdown shows, falling back rather than going blank. */
export function layoutLabel(layout: PlayerLayout): string {
  return LAYOUTS.find((entry) => entry.id === layout)?.label ?? "1 PLAYER";
}

/** Reads a stored or wire value back, so an unknown one cannot wedge the app. */
export function asLayout(value: unknown): PlayerLayout {
  return LAYOUTS.some((entry) => entry.id === value) ? (value as PlayerLayout) : "one";
}
