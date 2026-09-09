/**
 * The rendered app against the measurements taken from real rekordbox.
 *
 * Not a pixel diff. The mock's data is not the reference library's, and the
 * top bar deliberately diverges (no EXPORT dropdown, a settings gear instead),
 * so comparing images would fail for reasons that are not regressions.
 *
 * What *is* worth gating is that the app draws at the geometry the captures
 * were measured to. Every number below is a token with a recorded source, so
 * a failure here means either the layout drifted or a token changed without
 * the interface following it.
 */
import { expect, test, type Page } from "@playwright/test";

/** Reads a token from the running page, so the test and the app agree. */
async function token(page: Page, name: string): Promise<number> {
  const value = await page.evaluate(
    (n) => getComputedStyle(document.documentElement).getPropertyValue(n),
    name,
  );
  return Number.parseFloat(value);
}

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
});

test("the tree is the measured width", async ({ page }) => {
  // browseSetting.xml TreeView 319 including the rail.
  const expected = await token(page, "--s-tree-w");
  const box = await page.getByRole("navigation", { name: "Library" }).boundingBox();
  expect(box?.width).toBeCloseTo(expected, 0);
});

test("the source rail is the measured width", async ({ page }) => {
  // browseSetting.xml TreeShortcut w=57.
  const expected = await token(page, "--s-tree-rail-w");
  const box = await page.getByRole("tablist", { name: "Library sources" }).boundingBox();
  expect(box?.width).toBeCloseTo(expected, 0);
});

test("the player is the measured height", async ({ page }) => {
  // Measured at 231px by saturation banding, against the 275 an unsourced
  // token used to claim.
  const expected = await token(page, "--s-player-h");
  const box = await page.getByRole("region", { name: "Preview player" }).boundingBox();
  expect(box?.height).toBeCloseTo(expected, 0);
});

test("track rows sit on the measured pitch", async ({ page }) => {
  const expected = await token(page, "--s-row-height");
  const tops = await page
    .locator('[role="row"][aria-selected]')
    .evaluateAll((rows) => rows.slice(0, 6).map((r) => r.getBoundingClientRect().top));
  expect(tops.length).toBeGreaterThan(2);
  for (let i = 1; i < tops.length; i++) {
    expect(Math.round((tops[i] ?? 0) - (tops[i - 1] ?? 0))).toBe(Math.round(expected));
  }
});

test("the column header is the measured height", async ({ page }) => {
  const expected = await token(page, "--s-col-header-h");
  const box = await page.getByRole("columnheader").first().boundingBox();
  expect(box?.height).toBeCloseTo(expected, 0);
});

test("the status bar and top bar are the measured heights", async ({ page }) => {
  for (const [name, selector] of [
    ["--s-status-bar-h", page.getByRole("contentinfo")],
    ["--s-top-bar-h", page.getByRole("banner")],
  ] as const) {
    const expected = await token(page, name);
    const box = await selector.boundingBox();
    expect(box?.height, name).toBeCloseTo(expected, 0);
  }
});

test("default columns are the twelve the captured header menu ticks, behind the row number", async ({ page }) => {
  const headings = await page
    .getByRole("columnheader")
    .evaluateAll((h) => h.map((c) => c.textContent?.replace(/[↑↓]/g, "").trim() ?? ""));
  expect(headings).toEqual([
    // `#` leads and is not one of the twelve: it is a fixed column, absent
    // from the captured menu and from german.lang's column names alike.
    "#",
    "Preview", "Artwork", "Track Title", "Key", "BPM", "Time",
    "Rating", "Artist", "Comments", "Label", "Date Added", "Release Date",
  ]);
});

test("each default column is its measured width", async ({ page }) => {
  // Widths are rekordbox's own, from TableHeader-PlaylistTracks.
  const expected: Record<string, number> = {
    "Track Title": 387, Key: 73, BPM: 80, Time: 80, Rating: 101, Artist: 301,
    Comments: 210, Label: 128, Artwork: 80, Preview: 128,
  };
  for (const [label, width] of Object.entries(expected)) {
    const box = await page.getByRole("columnheader", { name: new RegExp(`^${label}`) }).boundingBox();
    expect(Math.round(box?.width ?? 0), label).toBe(width);
  }
});

test("the player is laid out the way the capture measures it", async ({ page }) => {
  // Every number here is measured off a 2x capture of rekordbox 7.2.11
  // running — see the `source` on each token in design/tokens/tokens.json.
  const player = await page.getByRole("region", { name: "Preview player" }).boundingBox();
  expect(player?.height).toBe(279);

  const transport = await page.locator("section[aria-label='Preview player'] > div").first().boundingBox();
  expect(transport?.width).toBe(80);

  const side = await page.getByRole("complementary", { name: "Cue list" }).boundingBox();
  expect(side?.width).toBe(209);

  // The bands stack in rekordbox's order, top to bottom.
  const tops = await Promise.all(
    ["player-vocal", "player-overview", "player-phrase", "player-detail"].map(async (id) =>
      (await page.getByTestId(id).boundingBox())?.y ?? 0,
    ),
  );
  expect(tops).toEqual([...tops].sort((a, b) => a - b));
});

test("the overview waveform is the measured height, not the whole band", async ({ page }) => {
  // The capture has a 49pt band holding a 5pt vocal strip, a 30pt waveform and
  // a 3pt position bar. Stretching the waveform over the whole band drew it
  // half again as tall as rekordbox does.
  const wave = await page.getByTestId("player-overview").boundingBox();
  expect(wave?.height).toBeCloseTo(await token(page, "--s-player-overview-wave-h"), 1);

  const vocal = await page.getByTestId("player-vocal").boundingBox();
  const phrase = await page.getByTestId("player-phrase").boundingBox();
  const band = (phrase?.y ?? 0) - (vocal?.y ?? 0);
  expect(band).toBeCloseTo(await token(page, "--s-player-overview-h"), 0);
});

test("hot cues are badges on the overview and red triangles at the top of the grid", async ({ page }) => {
  // Measured off docs/screenshots: four hot cues draw four 11pt badges along
  // the top of the overview, letter inside, and no line through the waveform.
  await page.locator('[role="gridcell"][data-col="title"]').nth(3).dblclick();
  const overview = page.getByTestId("player-overview");
  // The marker itself is zero-width — the position is its left edge — so the
  // badge inside it is what there is to look at.
  const badge = overview.locator('[title="Hot cue A"] b');
  await expect(badge).toBeVisible();

  const size = await token(page, "--s-cue-badge");
  const box = await badge.boundingBox();
  expect(box?.width).toBeCloseTo(size, 1);
  expect(box?.height).toBeCloseTo(size, 1);

  // Hung from the top of the strip, not centred in it.
  const strip = await overview.boundingBox();
  expect((box?.y ?? 0) - (strip?.y ?? 0)).toBeCloseTo(0, 1);

  // Widening the detail window until the cue falls inside it draws it there
  // too, as a triangle at the very top of the band, against the phrase blocks.
  const out = page.getByRole("button", { name: "Zoom out", exact: true });
  for (let i = 0; i < 3; i++) await out.click();
  const inDetail = page.getByTestId("player-detail").locator('[title="Hot cue A"] i');
  await expect(inDetail).toBeVisible();
  const detail = await page.getByTestId("player-detail").boundingBox();
  const triangle = await inDetail.boundingBox();
  expect(triangle?.y).toBeCloseTo(detail?.y ?? 0, 1);
  expect(triangle?.height).toBeCloseTo(await token(page, "--s-beat-head-h"), 1);
  // And over the grid, not behind a downbeat line.
  const layer = await inDetail.evaluate((e) =>
    getComputedStyle(e.parentElement as HTMLElement).zIndex,
  );
  expect(Number(layer)).toBeGreaterThan(0);
});

test("every strip of the deck shares one inset", async ({ page }) => {
  // They disagreed: 16 on the left and 12 on the right for the title row and
  // the overview, 71 and 12 for the phrase, and nothing at all for the detail,
  // which therefore ran wider than everything above it.
  const edges = await Promise.all(
    ["player-overview", "player-phrase", "player-detail"].map(async (id) => {
      const box = await page.getByTestId(id).boundingBox();
      return { left: Math.round(box?.x ?? 0), right: Math.round((box?.x ?? 0) + (box?.width ?? 0)) };
    }),
  );
  for (const edge of edges) {
    expect(edge.left).toBe(edges[0]?.left);
    expect(edge.right).toBe(edges[0]?.right);
  }

  // And that inset is the token's, measured from the deck's own right edge.
  const player = await page.getByRole("region", { name: "Preview player" }).boundingBox();
  const inset = await token(page, "--s-player-inset");
  const panel = await token(page, "--s-player-right-w");
  const right = (player?.x ?? 0) + (player?.width ?? 0) - panel;
  expect(right - (edges[0]?.right ?? 0)).toBeCloseTo(inset, 0);
});

test("a measured gap separates the player from the browser", async ({ page }) => {
  // The capture has the pad bar ending at 335pt and the track list starting at
  // 338pt, so three points of black sit between them. With the player and the
  // browser butted together the deck reads as part of the list.
  const player = await page.getByRole("region", { name: "Preview player" }).boundingBox();
  const body = await page.getByTestId("body").boundingBox();
  const gap = (body?.y ?? 0) - ((player?.y ?? 0) + (player?.height ?? 0));
  expect(gap).toBeCloseTo(await token(page, "--s-player-gutter-h"), 1);
});

test("each transport control sits where the capture measures it", async ({ page }) => {
  // Distances from the top of the player, in points, off the 2x capture. They
  // were wrong once because a flex `gap` and a per-item margin were both
  // applying, which put PLAY 34pt below CUE instead of the measured 10.5.
  const player = page.getByRole("region", { name: "Preview player" });
  const top = (await player.boundingBox())?.y ?? 0;
  for (const [name, want] of [
    ["Previous track", 36],
    ["Beat jump back", 82],
    ["Beat jump size", 110],
    ["Cue", 156],
    ["Play", 206.5],
  ] as const) {
    const box = await player.getByRole("button", { name, exact: true }).boundingBox();
    expect((box?.y ?? 0) - top, name).toBeCloseTo(want, 1);
  }
});

test("the pad row is built to the sizes the capture measures", async ({ page }) => {
  const player = page.getByRole("region", { name: "Preview player" });
  // Two stacked tabs, 80x24pt, filling the 48pt row between them.
  for (const name of ["CUE/LOOP", "GRID"]) {
    const box = await player.getByRole("tab", { name }).boundingBox();
    expect(box?.width, name).toBe(await token(page, "--s-pad-tab-w"));
    expect(box?.height, name).toBe(await token(page, "--s-pad-tab-h"));
  }

  // A pad is a dark well with a small bright square inside it when a cue is
  // set — not a filled button, which is what it was.
  const pad = player.getByRole("button", { name: "Hot cue A", exact: true });
  const padBox = await pad.boundingBox();
  expect(padBox?.width).toBe(await token(page, "--s-pad-cue-w"));
  expect(padBox?.height).toBe(await token(page, "--s-pad-cue-h"));
  const inner = await pad.locator("span").boundingBox();
  expect(inner?.height).toBe(await token(page, "--s-pad-cue-inner"));
  expect(inner?.height).toBeLessThan(padBox?.height ?? 0);
});

test("the player carries the controls a deck has", async ({ page }) => {
  const player = page.getByRole("region", { name: "Preview player" });
  for (const name of [
    "Previous track", "Next track",
    "Beat jump back", "Beat jump forward",
    "Zoom in", "Zoom out",
    "Cue", "Play",
    "Hot cue A", "Hot cue H",
  ]) {
    await expect(player.getByRole("button", { name, exact: true })).toBeVisible();
  }
  // The pad modes are stacked tabs, and the cue-list has its own three.
  await expect(player.getByRole("tab", { name: "CUE/LOOP" })).toBeVisible();
  await expect(player.getByRole("tab", { name: "GRID" })).toBeVisible();
  await expect(player.getByRole("group", { name: "Cue mode" })).toBeVisible();
  await expect(player.getByRole("tab", { name: "MEMORY" })).toBeVisible();
  await expect(player.getByRole("tab", { name: "HOT CUE" })).toBeVisible();
  await expect(player.getByRole("tab", { name: "INFO" })).toBeVisible();
});
