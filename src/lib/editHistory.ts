import { isTyping } from "./shortcuts";
import { setHistoryMenu } from "@/ipc/client";

export type HistoryAction = "undo" | "redo";
const EVENT = "deck-edit-history";

/** Native Edit menu commands follow focus, just like typing shortcuts. */
export function runEditHistory(action: HistoryAction): void {
  if (isTyping(document.activeElement)) {
    document.execCommand(action);
    return;
  }
  window.dispatchEvent(new CustomEvent<HistoryAction>(EVENT, { detail: action }));
}

/** Only the deck with the keyboard subscribes. */
export function listenEditHistory(
  handle: (action: HistoryAction) => void,
  undo: string | null = null,
  redo: string | null = null,
): () => void {
  const listener = (event: Event) => handle((event as CustomEvent<HistoryAction>).detail);
  const updateLabels = () => {
    const typing = isTyping(document.activeElement);
    void setHistoryMenu(typing ? null : undo, typing ? null : redo).catch(console.error);
  };
  window.addEventListener(EVENT, listener);
  document.addEventListener("focusin", updateLabels);
  document.addEventListener("focusout", updateLabels);
  updateLabels();
  return () => {
    window.removeEventListener(EVENT, listener);
    document.removeEventListener("focusin", updateLabels);
    document.removeEventListener("focusout", updateLabels);
    void setHistoryMenu(null, null).catch(console.error);
  };
}
