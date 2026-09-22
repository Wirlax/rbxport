/**
 * What the decks cost per frame, against the same window with none drawn.
 *
 * Relative, not absolute. A headless browser's `requestAnimationFrame` is
 * jittery enough that nearly half the frames of an empty window come back over
 * 16.7 ms — an absolute budget here would fail for reasons that have nothing
 * to do with the app. What is worth gating is the *difference* the decks make,
 * which is stable, and which is the number the waveform work has to hold.
 */
import { expect, test, type Page } from "@playwright/test";

/**
 * Not on a GitHub-hosted runner.
 *
 * The gate is a *difference* of medians precisely so a headless browser's own
 * jitter cancels out, and that holds on a machine this process has to itself.
 * It does not hold on a shared runner: the two halves are measured minutes
 * apart against whatever else that host is doing, and the difference picks up
 * the contention rather than the decks. Observed failing there on 213 while
 * passing three times out of three locally, with no frontend change between
 * them. Same reasoning as the ten-minute xrun test in the TODO — a timing gate
 * wants a machine that is not shared. `pnpm e2e` still runs it everywhere else.
 */
// TODO: run this timing gate on a dedicated runner without competing workloads.
test.skip(
  process.env.GITHUB_ACTIONS === "true",
  "a frame-timing gate cannot be measured on a shared runner",
);

/** Frames to time. At 60 Hz this is about four seconds a run. */
const FRAMES = 240;

/**
 * The most the decks may add to a frame, in milliseconds.
 *
 * Measured at 0.1-0.3 ms on this machine with two decks playing, so this is
 * ten times the observed cost: it catches a waveform redraw that has started
 * costing whole milliseconds without failing on ordinary noise.
 */
const ALLOWANCE = 3;

async function medianFrame(page: Page): Promise<number> {
  const frames = await page.evaluate(async (count: number) => {
    const deltas: number[] = [];
    let last = performance.now();
    await new Promise<void>((done) => {
      const tick = () => {
        const now = performance.now();
        deltas.push(now - last);
        last = now;
        if (deltas.length < count) requestAnimationFrame(tick);
        else done();
      };
      requestAnimationFrame(tick);
    });
    return deltas;
  }, FRAMES);
  const sorted = [...frames].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] ?? 0;
}

async function choose(page: Page, label: string) {
  await page.getByRole("button", { name: "Layout" }).click();
  await page.getByRole("menuitemradio", { name: label }).click();
}

test("two decks playing cost no more per frame than none at all", async ({ page }) => {
  await page.setViewportSize({ width: 1800, height: 1130 });
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");

  // The baseline: the same window, the same list, no deck drawn.
  await choose(page, "FULL BROWSER");
  const empty = await medianFrame(page);

  await choose(page, "2 PLAYER");
  await page.locator('[role="gridcell"][data-col="title"]').first().dblclick();
  const cell = page.locator('[role="gridcell"][data-col="title"]').nth(1);
  await cell.click({ button: "right" });
  const menu = page.getByRole("menu", { name: "Track" });
  await menu.getByRole("menuitem", { name: "Load", exact: true }).hover();
  await menu.getByRole("menuitem", { name: "Load track to player 2" }).click();
  // The transport is drawn in the column the decks share, so it is found by
  // its own name rather than inside the deck's region.
  for (const name of ["Deck A transport", "Deck B transport"]) {
    await page.getByRole("group", { name }).getByRole("button", { name: "Play" }).click();
  }
  const both = await medianFrame(page);

  expect(both - empty).toBeLessThan(ALLOWANCE);
});

test("scrolling the track list costs no more per frame than sitting still", async ({ page }) => {
  // The other place a list app drops frames. Rows are virtualised and keyed by
  // id, and this is the gate that says so: a hard scroll — a screenful and a
  // half every frame, all the way down — against the same window sitting
  // still.
  await page.setViewportSize({ width: 1800, height: 1130 });
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  // The whole collection, not the playlist the app opens on: thirty rows fit
  // on screen and a list that does not scroll measures nothing.
  await page.getByRole("treeitem", { name: /All Tracks/ }).click();
  await expect(page.getByTestId("browser-title")).toContainText("All Tracks");
  const still = await medianFrame(page);

  const scrolling = await page.evaluate(async (count: number) => {
    const el = document.querySelector('[data-testid="track-scroll"]');
    if (!(el instanceof HTMLElement)) return 0;
    const deltas: number[] = [];
    let last = performance.now();
    let y = 0;
    await new Promise<void>((done) => {
      const tick = () => {
        const now = performance.now();
        deltas.push(now - last);
        last = now;
        y += 900;
        el.scrollTop = y;
        if (deltas.length < count) requestAnimationFrame(tick);
        else done();
      };
      requestAnimationFrame(tick);
    });
    const sorted = [...deltas].sort((a, b) => a - b);
    return sorted[Math.floor(sorted.length / 2)] ?? 0;
  }, FRAMES);

  // It really moved, rather than sitting at the bottom for most of the run.
  const reached = await page
    .locator('[data-testid="track-scroll"]')
    .evaluate((el) => el.scrollTop);
  expect(reached).toBeGreaterThan(10_000);
  expect(scrolling - still).toBeLessThan(ALLOWANCE);
});
