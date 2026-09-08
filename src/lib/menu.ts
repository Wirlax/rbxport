/**
 * What a native menu item does.
 *
 * The shell sends one event carrying the item's id and nothing else, so the
 * decision — which action, and whether it is allowed right now — is made here
 * rather than in a listener. Import writes to the shared library, so it is
 * refused while rekordbox holds it, exactly as the other write paths are.
 */

export type MenuAction = "settings" | "import" | "missing";

export interface MenuCommand {
  action: MenuAction;
  /** Writes to the library, so rekordbox running is a refusal, not a race. */
  writes: boolean;
}

const COMMANDS: Record<string, MenuCommand> = {
  settings: { action: "settings", writes: false },
  import: { action: "import", writes: true },
  missing: { action: "missing", writes: true },
};

/** The command an item id names, or null when it is not one of ours. */
export function menuCommand(id: string): MenuCommand | null {
  return COMMANDS[id] ?? null;
}

/**
 * What to do about a menu click: run the action, or say why not.
 *
 * Returning the refusal rather than silently ignoring the click matters —
 * a menu item that does nothing reads as a broken app.
 */
export function resolveMenu(
  id: string,
  readOnly: boolean,
): { action: MenuAction } | { refused: string } | null {
  const command = menuCommand(id);
  if (!command) return null;
  if (command.writes && readOnly) {
    return { refused: "rekordbox is running, so the library is open read-only." };
  }
  return { action: command.action };
}
