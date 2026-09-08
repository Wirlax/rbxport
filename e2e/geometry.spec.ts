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
  // The pad modes, the memory transport and the cue-list tabs.
  await expect(player.getByRole("button", { name: "CUE/LOOP" })).toBeVisible();
  await expect(player.getByRole("button", { name: "GRID" })).toBeVisible();
  await expect(player.getByRole("group", { name: "Cue mode" })).toBeVisible();
  await expect(player.getByRole("tab", { name: "MEMORY" })).toBeVisible();
  await expect(player.getByRole("tab", { name: "HOT CUE" })).toBeVisible();
  await expect(player.getByRole("tab", { name: "INFO" })).toBeVisible();
});
