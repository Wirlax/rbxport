/** @vitest-environment jsdom */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { UpdateReadyNotice } from "./UpdateReadyNotice";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("UpdateReadyNotice", () => {
  it("announces the downloaded version and offers both next steps", () => {
    const onWhatsNew = vi.fn();
    const onRestart = vi.fn();
    act(() => root.render(
      <UpdateReadyNotice version="1.2.3" onWhatsNew={onWhatsNew} onRestart={onRestart} />,
    ));

    expect(host.textContent).toContain("rbxport v1.2.3 is ready. Restart to finish updating.");
    const buttons = Array.from(host.querySelectorAll("button"));
    expect(buttons.map((button) => button.textContent)).toEqual(["What’s new?", "Restart now"]);
    act(() => buttons[0]?.click());
    act(() => buttons[1]?.click());
    expect(onWhatsNew).toHaveBeenCalledOnce();
    expect(onRestart).toHaveBeenCalledOnce();
  });
});
