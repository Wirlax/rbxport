import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("grid")).toBeVisible();
});

test("shows the library tree and a populated track table", async ({ page }) => {
  await expect(page.getByRole("treeitem", { name: /All Tracks/ })).toBeVisible();
  await expect(page.getByRole("treeitem", { name: /Melodic Vox/ })).toBeVisible();
  const rows = page.getByRole("row").filter({ has: page.getByRole("gridcell") });
  expect(await rows.count()).toBeGreaterThan(5);
});

test("virtualizes: only a window of rows is in the DOM", async ({ page }) => {
  await page.getByRole("treeitem", { name: /All Tracks/ }).click();
  const rendered = await page.getByRole("row").filter({ has: page.getByRole("gridcell") }).count();
  // 2000 tracks in the mock; a ~1130px window at 25px rows is well under 100.
  expect(rendered).toBeLessThan(100);
});

test("selects a row, and cmd-click extends the selection", async ({ page }) => {
  const rows = page.getByRole("row").filter({ has: page.getByRole("gridcell") });
  await rows.nth(1).click();
  await expect(rows.nth(1)).toHaveAttribute("aria-selected", "true");

  await rows.nth(3).click({ modifiers: ["ControlOrMeta"] });
  await expect(rows.nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(rows.nth(3)).toHaveAttribute("aria-selected", "true");
  await expect(page.getByText(/Selected: 2 Tracks/)).toBeVisible();
});

test("shift-click selects a contiguous range", async ({ page }) => {
  const rows = page.getByRole("row").filter({ has: page.getByRole("gridcell") });
  await rows.nth(1).click();
  await rows.nth(5).click({ modifiers: ["Shift"] });
  await expect(page.getByText(/Selected: 5 Tracks/)).toBeVisible();
});

test("sorting a column orders the rows and toggles direction", async ({ page }) => {
  const header = page.getByRole("columnheader", { name: /Track Title/ });
  // Read the visible titles rather than just the first row: the unsorted order
  // can coincidentally start with the same track, which made an earlier version
  // of this test pass and fail for the wrong reasons.
  const titles = async () =>
    page.getByRole("row").filter({ has: page.getByRole("gridcell") })
        .evaluateAll((rows) => rows.slice(0, 12).map((r) => r.children[4]?.textContent ?? ""));

  await header.click();
  await expect(header).toHaveAttribute("data-sorted", "true");
  await expect.poll(async () => {
    const t = await titles();
    return t.length > 1 && t.every((v, i) => i === 0 || t[i - 1]!.localeCompare(v) <= 0);
  }).toBe(true);

  await header.click(); // descending
  await expect.poll(async () => {
    const t = await titles();
    return t.length > 1 && t.every((v, i) => i === 0 || t[i - 1]!.localeCompare(v) >= 0);
  }).toBe(true);
});

test("choosing a playlist swaps the view and its title", async ({ page }) => {
  await page.getByRole("treeitem", { name: /Hardstyle/ }).click();
  await expect(page.getByText(/^Hardstyle \(\d+ Tracks\)/)).toBeVisible();
});

test("scrolling loads further rows without leaving gaps", async ({ page }) => {
  await page.getByRole("treeitem", { name: /All Tracks/ }).click();
  const scroller = page.getByTestId("track-scroll");
  await scroller.evaluate((el) => { el.scrollTop = 10_000; });
  await expect
    .poll(async () =>
      scroller.locator('[role="row"] [role="gridcell"]').first().innerText().catch(() => ""),
    )
    .not.toBe("");
});
