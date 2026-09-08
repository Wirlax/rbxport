import { describe, expect, it } from "vitest";

import { menuCommand, resolveMenu } from "./menu";

describe("menu", () => {
  it("ignores an id that is not ours", () => {
    expect(menuCommand("quit")).toBeNull();
    expect(resolveMenu("quit", false)).toBeNull();
  });

  it("runs an action that only reads", () => {
    expect(resolveMenu("settings", true)).toEqual({ action: "settings" });
  });

  it("refuses a write while rekordbox holds the library", () => {
    expect(resolveMenu("import", true)).toEqual({
      refused: "rekordbox is running, so the library is open read-only.",
    });
    expect(resolveMenu("import", false)).toEqual({ action: "import" });
  });

  it("says why rather than doing nothing", () => {
    const outcome = resolveMenu("missing", true);
    expect(outcome).not.toBeNull();
    expect(outcome).toHaveProperty("refused");
  });
});
