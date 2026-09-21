/**
 * The GRID panel: editing the loaded track's beat grid.
 *
 * The mock backend keeps a grid per track and applies the same edits the
 * Rust module does, so every button here is a real round trip: written,
 * announced through `onGridChanged`, refetched, and the BPM field, the beat
 * lines and the undo/redo buttons redrawn from what came back. A tempo
 * change also re-reads the library, as it does for real, so the row in the
 * browser shows the new BPM too.
 */
import { expect, test, type Page } from "@playwright/test";

const player = (page: Page) => page.getByRole("region", { name: "Preview player" });
const gridPanel = (page: Page) => player(page).getByRole("region", { name: "Beat grid" });
const button = (page: Page, label: string) => gridPanel(page).getByRole("button", { name: label, exact: true });
const bpmField = (page: Page) => page.getByTestId("grid-bpm");

/** The BPM the field prints, as a number. */
async function bpm(page: Page): Promise<number> {
  return Number.parseFloat((await bpmField(page).inputValue()) ?? "0");
}

/** Loads the fourth row — analysed, so it has a grid — and opens GRID. */
async function load(page: Page, query = "?writable=1") {
  await page.addInitScript(() => localStorage.setItem("rbl.preferences", JSON.stringify({ view: { tooltips: true } })));
  await page.goto(`/${query}`);
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  await page.locator('[role="gridcell"][data-col="title"]').nth(3).dblclick();
  await expect(player(page).getByRole("button", { name: "Play", exact: true })).toBeEnabled();
  await player(page).getByRole("tab", { name: "GRID" }).click();
  await expect(gridPanel(page)).toBeVisible();
}

/** Runs the deck for a moment and pauses it, so the playhead is somewhere in the track. */
async function playAWhile(page: Page) {
  await player(page).getByRole("button", { name: "Play", exact: true }).click();
  await page.waitForTimeout(700);
  await player(page).getByRole("button", { name: "Pause", exact: true }).click();
}

test("doubling and halving the tempo write the grid, and undo and redo walk it back and forth", async ({ page }) => {
  await load(page);
  const original = await bpm(page);
  expect(original).toBeGreaterThan(0);
  await expect(button(page, "Undo the last grid edit")).toBeDisabled();
  await expect(button(page, "Redo the last grid edit")).toBeDisabled();

  await button(page, "Double the tempo").click();
  await expect(bpmField(page)).toHaveValue(`${(original * 2).toFixed(2)}`);
  await expect(button(page, "Undo the last grid edit")).toBeEnabled();
  // The row in the browser follows: the library was re-read.
  await expect(page.locator('[role="gridcell"][data-col="bpm"]').nth(3)).toHaveText(`${(original * 2).toFixed(2)}`);

  await button(page, "Halve the tempo").click();
  await expect(bpmField(page)).toHaveValue(`${original.toFixed(2)}`);

  await button(page, "Undo the last grid edit").click();
  await expect(bpmField(page)).toHaveValue(`${(original * 2).toFixed(2)}`);
  await expect(button(page, "Redo the last grid edit")).toBeEnabled();
  await button(page, "Redo the last grid edit").click();
  await expect(bpmField(page)).toHaveValue(`${original.toFixed(2)}`);
  await expect(button(page, "Redo the last grid edit")).toBeDisabled();
});

test("widen and narrow change the target spacing, and the shift keys leave it alone", async ({ page }) => {
  await load(page);
  const original = await bpm(page);
  await button(page, "Speed the grid up").click();
  await expect.poll(() => bpm(page)).toBeGreaterThan(original);
  await button(page, "Undo the last grid edit").click();
  await expect(bpmField(page)).toHaveValue(`${original.toFixed(2)}`);

  // The deck takes the arrows once it has been clicked.
  await page.getByTestId("player-detail").click();
  await page.keyboard.press("ControlOrMeta+ArrowRight");
  await expect(button(page, "Undo the last grid edit")).toBeEnabled();
  await expect(bpmField(page)).toHaveValue(`${original.toFixed(2)}`);
  await page.keyboard.press("ControlOrMeta+ArrowLeft");
  await expect(bpmField(page)).toHaveValue(`${original.toFixed(2)}`);
});

test("the lock greys the editing buttons and holds across the panel's redraws", async ({ page }) => {
  await load(page);
  const lock = button(page, "Lock the grid");
  await expect(lock).toHaveAttribute("aria-pressed", "false");
  await lock.click();
  await expect(lock).toHaveAttribute("aria-pressed", "true");
  await expect(button(page, "Double the tempo")).toBeDisabled();
  await expect(button(page, "Double the tempo")).toHaveAttribute("title", "The beat grid is locked");
  await expect(button(page, "Tap the tempo")).toBeDisabled();
  // Switching pad modes and back does not lose it: the state is the backend's.
  await player(page).getByRole("tab", { name: "CUE/LOOP" }).click();
  await player(page).getByRole("tab", { name: "GRID" }).click();
  await expect(button(page, "Lock the grid")).toHaveAttribute("aria-pressed", "true");
  await lock.click();
  await expect(button(page, "Double the tempo")).toBeEnabled();
});

test("scope begins at the playhead and clears with Adjust all beats", async ({ page }) => {
  await load(page);
  await playAWhile(page);
  const cut = button(page, "Adjust beats from here");
  await cut.click();
  await expect(cut).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("grid-edit-start")).toBeVisible();
  await expect(button(page, "Double the tempo")).toHaveAttribute("title", /from the selected beat on/);
  // A tempo edit from the cut on leaves the grid's own tempo — the first
  // beat's — where it was, which is what the browser's row shows.
  const original = await bpm(page);
  await button(page, "Double the tempo").click();
  await expect(button(page, "Undo the last grid edit")).toBeEnabled();
  await expect(page.locator('[role="gridcell"][data-col="bpm"]').nth(3)).toHaveText(
    `${original.toFixed(2)}`,
  );
  // The field, though, reads the tempo under the playhead: run on past the
  // cut and it is the doubled half's.
  await playAWhile(page);
  await expect(bpmField(page)).toHaveValue(`${(original * 2).toFixed(2)}`);
  await button(page, "Adjust all beats").click();
  await expect(cut).toHaveAttribute("aria-pressed", "false");
  await expect(page.getByTestId("grid-edit-start")).toHaveCount(0);
});

test("the BPM field reads the tempo under the playhead, not the grid's first beat", async ({ page }) => {
  await load(page);
  const original = await bpm(page);
  const overview = await page.getByTestId("player-overview").boundingBox();
  /** Seeks by clicking the overview `fraction` of the way along it. */
  const seekTo = async (fraction: number) => {
    await page.mouse.click(
      (overview?.x ?? 0) + (overview?.width ?? 0) * fraction,
      (overview?.y ?? 0) + (overview?.height ?? 0) / 2,
    );
  };

  // Half way in, cut, and double from there: the second half of the track
  // now runs at twice the tempo the first half does.
  await seekTo(0.5);
  await button(page, "Adjust beats from here").click();
  await button(page, "Double the tempo").click();
  await expect(button(page, "Undo the last grid edit")).toBeEnabled();

  await seekTo(0.75);
  await expect(bpmField(page)).toHaveValue(`${(original * 2).toFixed(2)}`);
  await seekTo(0.1);
  await expect(bpmField(page)).toHaveValue(`${original.toFixed(2)}`);
});

test("tapping shows the taps' tempo and writes it during the tapping run", async ({ page }) => {
  await load(page);
  const now = new Date("2026-09-21T12:00:00Z");
  await page.clock.install({time: now});
  await page.clock.pauseAt(now);
  const tap = button(page, "Tap the tempo");
  // Four taps at 100 BPM: 600 ms apart.
  for (let i = 0; i < 4; i++) {
    await tap.click();
    if (i < 3) await page.clock.runFor(600);
  }
  await expect(bpmField(page)).toHaveAttribute("data-tapping", "true");
  // Around 100: each click costs the driver a little, so the taps land late.
  const shown = await bpm(page);
  expect(shown).toBeGreaterThan(80);
  expect(shown).toBeLessThan(115);
  // The run ends after the gap, and the grid takes the tempo.
  await page.clock.runFor(901);
  await expect(bpmField(page)).not.toHaveAttribute("data-tapping", "true");
  const written = await bpm(page);
  expect(written).toBeGreaterThan(80);
  expect(written).toBeLessThan(115);
  await expect(button(page, "Undo the last grid edit")).toBeEnabled();
});

test("a read-only library greys edits and the lock but permits the metronome", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("rbl.preferences", JSON.stringify({ view: { tooltips: true } })));
  await load(page, "");
  await expect(button(page, "Double the tempo")).toBeDisabled();
  await expect(button(page, "Shift the grid earlier")).toBeDisabled();
  await expect(button(page, "Double the tempo")).toHaveAttribute("title", /read-only/);
  await expect(button(page, "Toggle metronome")).toBeEnabled();
  await expect(button(page, "Lock the grid")).toBeDisabled();
});

test("typed BPM and separate metronome controls work", async ({page}) => {
  await load(page);
  await bpmField(page).fill("125.5"); await bpmField(page).press("Enter");
  await expect(bpmField(page)).toHaveValue("125.50");
  const toggle=button(page,"Toggle metronome");
  await toggle.click(); await expect(toggle).toHaveAttribute("aria-pressed","true");
  await gridPanel(page).getByRole("button",{name:/^Metronome volume:/}).click();
  await expect(toggle).toHaveAttribute("aria-pressed","true");
  await toggle.click(); await expect(toggle).toHaveAttribute("aria-pressed","false");
});

test("two-player grid controls edit the loaded track and sound selection is available", async ({page}) => {
  await load(page);
  await button(page,"Toggle metronome").click({button:"right"});
  await page.getByRole("menu",{name:"Metronome sound"}).getByRole("menuitem",{name:"Sound 2"}).click();
  await page.getByRole("button",{name:"Layout",exact:true}).click();
  await page.getByRole("menuitemradio",{name:"2 PLAYER",exact:true}).click();
  const controls=page.getByTestId("player-controls").first();
  await controls.getByRole("button",{name:"Shift the grid earlier",exact:true}).click();
  await controls.getByRole("button",{name:"Mark the downbeat here",exact:true}).click();
  await controls.getByRole("button",{name:"Shift the grid later",exact:true}).click();
  await page.getByRole("button",{name:"Layout",exact:true}).click();
  await page.getByRole("menuitemradio",{name:"1 PLAYER",exact:true}).click();
  await player(page).getByRole("tab",{name:"GRID",exact:true}).click();
  await expect(button(page,"Undo the last grid edit")).toBeEnabled();
});
