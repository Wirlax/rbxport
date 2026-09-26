/**
 * The hot cue pads and the HOT CUE list beside the deck.
 *
 * rekordbox's own arrangement: an empty pad sets `Hot Cue <letter>` at the
 * playhead (`Set Hot Cue A`, on `1`-`3` for the first three from the Export
 * key map), a set pad calls its cue, and each set row of the HOT CUE tab has
 * a ✕ (`Clear Hot Cue A`, on `command + 1`-`3`). The mock backend keeps cues
 * per track, so an edit here is a real round trip: written, announced,
 * refetched, and the pad, the badges on both waveforms, the list and the
 * browser row's CUE mark all redrawn from the one array.
 */
import { expect, test, type Page } from "@playwright/test";
import { enableTooltips } from "./helpers";

const player = (page: Page) => page.getByRole("region", { name: "Preview player" });
// The pads and the list's rows share a name, so each is found in its own box.
const pad = (page: Page, letter: string) =>
  player(page).locator('[aria-label="Hot cues"]').getByRole("button", { name: `Hot cue ${letter}`, exact: true });
const panel = (page: Page) => page.getByRole("complementary", { name: "Cue list" });
const listRow = (page: Page, letter: string) =>
  panel(page).getByRole("button", { name: `Hot cue ${letter}`, exact: true });
const clearButton = (page: Page, letter: string) =>
  panel(page).getByRole("button", { name: `Clear hot cue ${letter}` });
const overviewBadge = (page: Page, letter: string) =>
  page.getByTestId("player-overview").locator(`[data-cue="${letter}"]`);
const detailBadge = (page: Page, letter: string) =>
  page.getByTestId("player-detail").locator(`[data-cue="${letter}"]`);
/** The browser's attribute cell for the loaded row: CUE when the track has hot cues. */
const cueMark = (page: Page) => page.locator('[role="gridcell"][data-col="attr"]').nth(3);
const elapsed = (page: Page) => player(page).locator('[class*="elapsed"]');

/** Loads the fourth row — analysed, so it carries the mock's four hot cues. */
async function load(page: Page, query = "") {
  await page.goto(`/${query}`);
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  await page.locator('[role="gridcell"][data-col="title"]').nth(3).dblclick();
  await expect(player(page).getByRole("button", { name: "Play", exact: true })).toBeEnabled();
  await panel(page).getByRole("tab", { name: "HOT CUE" }).click();
}

/** Runs the deck for a moment and pauses it, so the playhead is somewhere in the track. */
async function playAWhile(page: Page) {
  await player(page).getByRole("button", { name: "Play", exact: true }).click();
  await page.waitForTimeout(700);
  await player(page).getByRole("button", { name: "Pause", exact: true }).click();
}

test("an empty pad sets the hot cue at the playhead, and its ✕ in the list clears it", async ({ page }) => {
  await enableTooltips(page);
  await load(page, "?writable=1");

  // The mock's track has A to D; E is the first empty pad.
  await expect(pad(page, "D")).toHaveAttribute("aria-pressed", "true");
  await expect(pad(page, "E")).toHaveAttribute("aria-pressed", "false");
  await expect(pad(page, "E")).toHaveAttribute("title", "Set Hot Cue E");
  await expect(listRow(page, "E")).toHaveAttribute("aria-disabled", "true");
  await expect(overviewBadge(page, "E")).toHaveCount(0);

  await playAWhile(page);
  await pad(page, "E").click();

  // Lit, listed with a time and a ✕, and badged on both waveforms.
  await expect(pad(page, "E")).toHaveAttribute("aria-pressed", "true");
  await expect(listRow(page, "E")).not.toHaveAttribute("aria-disabled", "true");
  await expect(listRow(page, "E")).toContainText(/\d\d:\d\d/);
  await expect(clearButton(page, "E")).toBeEnabled();
  await expect(overviewBadge(page, "E")).toHaveCount(1);
  await expect(detailBadge(page, "E")).toHaveCount(1);
  await expect(overviewBadge(page, "E").locator("b")).toHaveText("E");

  // A set pad calls its cue rather than setting over it: the list does not
  // change when it is pressed again from elsewhere in the track.
  await playAWhile(page);
  const before = await listRow(page, "E").textContent();
  await pad(page, "E").click();
  await page.waitForTimeout(200);
  await expect(listRow(page, "E")).toHaveText(before ?? "");

  // ✕ clears it, and the pad, the list and both waveforms let go together.
  await clearButton(page, "E").click();
  await expect(pad(page, "E")).toHaveAttribute("aria-pressed", "false");
  await expect(listRow(page, "E")).toHaveAttribute("aria-disabled", "true");
  await expect(clearButton(page, "E")).toHaveCount(0);
  await expect(overviewBadge(page, "E")).toHaveCount(0);
  await expect(detailBadge(page, "E")).toHaveCount(0);
});

test("the simple player's overview wears the badge too", async ({ page }) => {
  // The strip draws from the same cue array as the full deck, so a hot cue
  // set here is a badge there — the 9.03.41 PM capture shows them.
  await load(page, "?writable=1");
  await playAWhile(page);
  await pad(page, "E").click();
  await expect(overviewBadge(page, "E")).toHaveCount(1);

  await page.getByRole("button", { name: "Layout" }).click();
  await page.getByRole("menuitemradio", { name: "SIMPLE PLAYER" }).click();
  const strip = page.getByTestId("simple-player-overview");
  await expect(strip.locator('[data-cue="D"]')).toHaveCount(1);
  await expect(strip.locator('[data-cue="E"]')).toHaveCount(1);
});

test("the browser row's CUE mark follows the deck without a reload", async ({ page }) => {
  await load(page, "?writable=1");
  // The Attribute column is off by default; the header menu turns it on.
  await page.getByRole("columnheader", { name: /BPM/ }).click({ button: "right" });
  await page.getByRole("menu", { name: "Columns" }).getByRole("menuitemcheckbox", { name: "Attribute" }).click();
  await expect(cueMark(page)).toContainText("CUE");
  const rows = page.locator('[role="row"]');
  const rowCount = await rows.count();

  // Clearing all four hot cues empties the row's letters, and the mark goes.
  for (const letter of ["A", "B", "C", "D"]) {
    await clearButton(page, letter).click();
    await expect(clearButton(page, letter)).toHaveCount(0);
  }
  await expect(cueMark(page)).not.toContainText("CUE");
  // No reload: the table kept its rows.
  await expect(rows).toHaveCount(rowCount);

  // `1` is `Set Hot Cue A`; the mark comes back with the letter.
  await playAWhile(page);
  await page.keyboard.press("1");
  await expect(pad(page, "A")).toHaveAttribute("aria-pressed", "true");
  await expect(cueMark(page)).toContainText("CUE");
  // `command + 1` is `Clear Hot Cue A`.
  await page.keyboard.press("Meta+1");
  await expect(pad(page, "A")).toHaveAttribute("aria-pressed", "false");
  await expect(cueMark(page)).not.toContainText("CUE");
});

test("with rekordbox holding the library, setting and clearing are refused and calling is not", async ({ page }) => {
  // The mock's library is read-only unless asked otherwise.
  await enableTooltips(page);
  await load(page);
  await expect(page.getByRole("contentinfo")).toContainText("Library read-only");

  // An empty pad is dead, and says why.
  await expect(pad(page, "E")).toBeDisabled();
  await expect(pad(page, "E")).toHaveAttribute("title", /read-only/);
  // A set pad still calls its cue, and its ✕ is drawn but dead.
  await expect(pad(page, "A")).toBeEnabled();
  await expect(pad(page, "A")).not.toHaveAttribute("title", /read-only/);
  await expect(clearButton(page, "A")).toBeDisabled();
  await expect(clearButton(page, "A")).toHaveAttribute("title", /read-only/);

  // Calling moves the playhead to the cue's time, which the list shows.
  const time = (await listRow(page, "A").textContent())?.match(/\d\d:\d\d/)?.[0] ?? "";
  expect(time).not.toBe("");
  await pad(page, "D").click();
  await expect(elapsed(page)).not.toContainText(time);
  await pad(page, "A").click();
  await expect(elapsed(page)).toContainText(time);
  // The key calls it too, and clears nothing.
  await pad(page, "D").click();
  await page.keyboard.press("1");
  await expect(elapsed(page)).toContainText(time);
  await page.keyboard.press("Meta+1");
  await page.waitForTimeout(200);
  await expect(pad(page, "A")).toHaveAttribute("aria-pressed", "true");
});
