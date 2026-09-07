/**
 * Track-table selection model.
 *
 * Selection is by row id, not row index, so it survives a re-sort. A range
 * selection can span rows the frontend has not fetched, so the caller resolves
 * ranges through the backend rather than from cached pages.
 */
export interface SelectionState {
  ids: ReadonlySet<string>;
  /** Row index the next shift-click extends from. */
  anchorIndex: number | null;
}

export const emptySelection: SelectionState = { ids: new Set(), anchorIndex: null };

export type ClickModifier = "none" | "toggle" | "range";

export function modifierFor(e: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean }): ClickModifier {
  if (e.shiftKey) return "range";
  return e.metaKey || e.ctrlKey ? "toggle" : "none";
}

/**
 * Applies a click. `rangeIds` must be supplied by the caller for "range"
 * clicks — it is the ids between the anchor and the clicked index inclusive.
 */
export function applyClick(
  state: SelectionState,
  clicked: { id: string; index: number },
  modifier: ClickModifier,
  rangeIds?: readonly string[],
): SelectionState {
  switch (modifier) {
    case "none":
      return { ids: new Set([clicked.id]), anchorIndex: clicked.index };
    case "toggle": {
      const next = new Set(state.ids);
      if (next.has(clicked.id)) next.delete(clicked.id);
      else next.add(clicked.id);
      return { ids: next, anchorIndex: clicked.index };
    }
    case "range": {
      if (state.anchorIndex === null || !rangeIds) {
        return { ids: new Set([clicked.id]), anchorIndex: clicked.index };
      }
      // Anchor stays put so successive shift-clicks grow from the same origin.
      return { ids: new Set(rangeIds), anchorIndex: state.anchorIndex };
    }
  }
}

export function isSelected(state: SelectionState, id: string): boolean {
  return state.ids.has(id);
}
