import { describe, expect, it } from "vitest";

import { formatBytes, parseChangelog, spans } from "./changelog";

const SECTION = `## [0.6.0] — 2026-09-12

### Added
- The app checks for a newer version when it starts, downloads it with a
  progress bar, and shows what changed.
- A \`limiter\` on the mix bus.

### Fixed
- A dragged track carries a faded row.

Some closing words.

[0.6.0]: https://example.com/compare/v0.5.0...v0.6.0
`;

describe("parseChangelog", () => {
  it("reads the release heading, sub-headings, bullets and paragraphs in order", () => {
    expect(parseChangelog(SECTION)).toEqual([
      { kind: "release", version: "0.6.0", date: "2026-09-12" },
      { kind: "heading", text: "Added" },
      {
        kind: "list",
        items: [
          "The app checks for a newer version when it starts, downloads it with a progress bar, and shows what changed.",
          "A `limiter` on the mix bus.",
        ],
      },
      { kind: "heading", text: "Fixed" },
      { kind: "list", items: ["A dragged track carries a faded row."] },
      { kind: "paragraph", text: "Some closing words." },
    ]);
  });

  it("keeps a remark after the date out of the date", () => {
    expect(parseChangelog("## [0.3.0] — 2026-09-10 — not published\n\nSee 0.4.0.")).toEqual([
      { kind: "release", version: "0.3.0", date: "2026-09-10" },
      { kind: "paragraph", text: "See 0.4.0." },
    ]);
  });

  it("gives nothing for nothing", () => {
    expect(parseChangelog("")).toEqual([]);
    expect(parseChangelog("\n\n")).toEqual([]);
  });
});

describe("spans", () => {
  it("splits code spans from prose", () => {
    expect(spans("Settings › `Audio` pane")).toEqual([
      { code: false, text: "Settings › " },
      { code: true, text: "Audio" },
      { code: false, text: " pane" },
    ]);
  });

  it("leaves an unpaired backtick as text", () => {
    expect(spans("a `b")).toEqual([
      { code: false, text: "a " },
      { code: false, text: "b" },
    ]);
  });
});

describe("formatBytes", () => {
  it("reads as the window shows it", () => {
    expect(formatBytes(512)).toBe("1 KB");
    expect(formatBytes(300 * 1024)).toBe("300 KB");
    expect(formatBytes(16_342_693)).toBe("15.6 MB");
    expect(formatBytes(Number.NaN)).toBe("0 KB");
  });
});
