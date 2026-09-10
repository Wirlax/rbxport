/**
 * The MEMORY cluster and the memory list beside the deck.
 *
 * rekordbox's own arrangement: `Set Memory Cue` stores the cue point the
 * transport's CUE set, `Call Previous/Next Memory Cue` move to the cue either
 * side of the playhead, `Delete Memory Cue` takes the one it is on, and each
 * row of the MEMORY tab has a ✕. On `M`, `B`, `N` and `X` from the Export key
 * map. The mock backend keeps cues per track, so an edit here is a real round
 * trip: written, announced, refetched, redrawn.
 */
import { expect, test, type Page } from "@playwright/test";

const player = (page: Page) => page.getByRole("region", { name: "Preview player" });
const memoryRows = (page: Page) =>
  page.getByRole("complementary", { name: "Cue list" }).getByRole("button", { name: /^Delete memory cue \d\d:\d\d:\d\d\d$/ });
const elapsed = (page: Page) => player(page).locator('[class*="elapsed"]');

/** Loads the fourth row — analysed, so it carries the mock's cues. */
async function load(page: Page, query = "") {
  await page.goto(`/${query}`);
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  await page.locator('[role="gridcell"][data-col="title"]').nth(3).dblclick();
  await expect(player(page).getByRole("button", { name: "Play", exact: true })).toBeEnabled();
}

/** Runs the deck for a moment, pauses it, and sets the cue point there. */
async function cueSomewhereIn(page: Page) {
  await player(page).getByRole("button", { name: "Play", exact: true }).click();
  await page.waitForTimeout(700);
  await player(page).getByRole("button", { name: "Pause", exact: true }).click();
  // Paused away from the cue point, CUE sets it here (snapped to the beat).
  await page.keyboard.press("c");
}

test("with rekordbox holding the library, storing and deleting are refused and calling is not", async ({ page }) => {
  // The mock's library is read-only unless asked otherwise, which is the
  // state every other write is tested against.
  await load(page);
  const deck = player(page);
  await expect(page.getByRole("contentinfo")).toContainText("Read-only");

  const store = deck.getByRole("button", { name: "Set memory cue" });
  await expect(store).toBeDisabled();
  await expect(store).toHaveAttribute("title", /read-only/);
  await expect(deck.getByRole("button", { name: "Delete memory cue", exact: true })).toBeDisabled();
  // A row is listed, and its ✕ is drawn but dead.
  await expect(memoryRows(page)).toHaveCount(1);
  await expect(memoryRows(page).first()).toBeDisabled();

  // Calling a cue moves the playhead and writes nothing, so it still works.
  await expect(deck.getByRole("button", { name: "Next memory cue" })).toBeEnabled();
  await expect(deck.getByRole("button", { name: "Previous memory cue" })).toBeEnabled();
  // And the key does nothing rather than something it should not.
  await page.keyboard.press("m");
  await expect(memoryRows(page)).toHaveCount(1);
});

test("MEMORY stores the cue point, and the list and both waveforms show it", async ({ page }) => {
  await load(page, "?writable=1");
  const deck = player(page);
  const overview = page.getByTestId("player-overview");
  await expect(overview.locator('[title="Memory cue"]')).toHaveCount(1);
  await expect(memoryRows(page)).toHaveCount(1);

  await cueSomewhereIn(page);
  await deck.getByRole("button", { name: "Set memory cue" }).click();

  // The row appears, in position order, and the overview marks it. No
  // reload: the table underneath keeps its rows.
  await expect(memoryRows(page)).toHaveCount(2);
  await expect(overview.locator('[title="Memory cue"]')).toHaveCount(2);
  const rows = page.getByRole("complementary", { name: "Cue list" }).getByText(/^\d\d:\d\d:\d\d\d$/);
  const times = await rows.allTextContents();
  expect(times).toHaveLength(2);
  expect(times[0]! < times[1]!).toBe(true);

  // Pressed again on the same point it stores nothing twice.
  await page.keyboard.press("m");
  await page.waitForTimeout(200);
  await expect(memoryRows(page)).toHaveCount(2);

  // ✕ in the cluster deletes the one under the playhead — the one just set —
  // and the marker goes with the row.
  await deck.getByRole("button", { name: "Delete memory cue", exact: true }).click();
  await expect(memoryRows(page)).toHaveCount(1);
  await expect(overview.locator('[title="Memory cue"]')).toHaveCount(1);
});

test("the M and X keys are MEMORY and its ✕, and a row's ✕ deletes that row", async ({ page }) => {
  await load(page, "?writable=1");
  await cueSomewhereIn(page);

  await page.keyboard.press("m");
  await expect(memoryRows(page)).toHaveCount(2);
  await page.keyboard.press("x");
  await expect(memoryRows(page)).toHaveCount(1);

  // The first row's ✕ takes the mock's own memory cue, leaving nothing.
  await page.keyboard.press("m");
  await expect(memoryRows(page)).toHaveCount(2);
  await memoryRows(page).first().click();
  await expect(memoryRows(page)).toHaveCount(1);
  await memoryRows(page).first().click();
  await expect(memoryRows(page)).toHaveCount(0);
  await expect(page.getByTestId("player-overview").locator('[title="Memory cue"]')).toHaveCount(0);
});

test("◀ and ▶ call the memory cue either side of the playhead", async ({ page }) => {
  await load(page, "?writable=1");
  const deck = player(page);
  await cueSomewhereIn(page);
  await page.keyboard.press("m");
  await expect(memoryRows(page)).toHaveCount(2);

  // Two memory cues now; their positions from the list, to the second.
  const listed = await page
    .getByRole("complementary", { name: "Cue list" })
    .getByText(/^\d\d:\d\d:\d\d\d$/)
    .allTextContents();
  const [first, second] = listed.map((t) => t.slice(0, 5)) as [string, string];

  // The playhead is on the cue just stored, which is the earlier of the two:
  // CUE snapped it to a beat under a second in, and the mock's own sits at
  // 2% of the track. ▶ from there is the second; before the first, ◀ stays.
  await expect(elapsed(page)).toContainText(first);
  await deck.getByRole("button", { name: "Previous memory cue" }).click();
  await expect(elapsed(page)).toContainText(first);
  await deck.getByRole("button", { name: "Next memory cue" }).click();
  await expect(elapsed(page)).toContainText(second);
  // Past the last, ▶ stays put.
  await deck.getByRole("button", { name: "Next memory cue" }).click();
  await expect(elapsed(page)).toContainText(second);

  // ◀ walks back the same way, on the key as on the button.
  await page.keyboard.press("b");
  await expect(elapsed(page)).toContainText(first);
  await page.keyboard.press("n");
  await expect(elapsed(page)).toContainText(second);

  // Called, the cue is the cue point: C from here previews rather than
  // setting a new one, so the list does not grow when M follows.
  await page.keyboard.press("m");
  await page.waitForTimeout(200);
  await expect(memoryRows(page)).toHaveCount(2);
});
