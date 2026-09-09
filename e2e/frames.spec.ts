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
  for (const deck of await page.getByRole("region", { name: /^Preview player/ }).all()) {
    await deck.getByRole("button", { name: "Play" }).click();
  }
  const both = await medianFrame(page);

  expect(both - empty).toBeLessThan(ALLOWANCE);
});
