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

test("analysed tracks draw a waveform, unanalysed ones stay blank", async ({ page }) => {
  const canvases = page.locator('[role="row"] canvas');
  await expect.poll(async () => canvases.count()).toBeGreaterThan(5);

  // Every canvas that was drawn must have actual pixels; a blank one means the
  // fetch-and-render path silently failed, which is how this first shipped.
  const painted = await page.evaluate(() => {
    let drawn = 0;
    let blank = 0;
    for (const canvas of document.querySelectorAll<HTMLCanvasElement>('[role="row"] canvas')) {
      const ctx = canvas.getContext("2d");
      if (!ctx) continue;
      const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
      if (data.some((v) => v !== 0)) drawn++;
      else blank++;
    }
    return { drawn, blank };
  });
  expect(painted.drawn).toBeGreaterThan(5);
  expect(painted.blank).toBe(0);
});

test("typing in the search box filters the list, and Escape clears it", async ({ page }) => {
  await page.goto("/");
  const search = page.getByRole("searchbox", { name: /search within/i });
  const title = page.getByTestId("browser-title");

  // A count appears only once the view has settled, so waiting for one is
  // waiting for the load.
  await expect(title).toContainText("Tracks)");
  const before = await title.textContent();

  await search.fill("Extended");
  // Rust does the filtering; the count in the title is what proves it landed.
  await expect(title).toContainText("Tracks)");
  await expect(title).not.toHaveText(before ?? "");

  await search.press("Escape");
  await expect(search).toHaveValue("");
  await expect(title).toHaveText(before ?? "");
});

test("the search shortcut puts the caret in the box from anywhere", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("track-scroll").click();

  const modifier = process.platform === "darwin" ? "Meta" : "Control";
  await page.keyboard.press(`${modifier}+f`);

  const search = page.getByRole("searchbox", { name: /search within/i });
  await expect(search).toBeFocused();
});

test("a search that matches nothing empties the list without breaking it", async ({ page }) => {
  await page.goto("/");
  const search = page.getByRole("searchbox", { name: /search within/i });
  await search.fill("zzzzzzzzzz-no-such-track");

  await expect(page.getByTestId("browser-title")).toContainText("(0 Tracks)");
  // The table must still be there and still scrollable, not collapsed.
  await expect(page.getByTestId("track-scroll")).toBeVisible();
});

test("the column headers stay aligned with the rows when scrolled sideways", async ({ page }) => {
  // As a sibling above the scroller the header stayed put while the rows moved,
  // so every column sheared away from its own heading.
  await page.goto("/");
  const scroll = page.getByTestId("track-scroll");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");

  const columnLeft = async () => {
    const head = page.getByRole("row").first().getByText("BPM", { exact: true });
    const cell = await head.boundingBox();
    return cell?.x ?? 0;
  };

  const before = await columnLeft();
  await scroll.evaluate((el) => {
    el.scrollLeft = 300;
  });
  await expect
    .poll(async () => Math.round((await scroll.evaluate((el) => el.scrollLeft)) as number))
    .toBeGreaterThan(0);

  const after = await columnLeft();
  // The heading has to travel with its column, not stay behind.
  expect(Math.round(before - after)).toBeGreaterThan(200);
});

test("the column header stays visible when scrolled down", async ({ page }) => {
  // Sticky, so moving with the rows sideways must not let it scroll away.
  await page.goto("/");
  const scroll = page.getByTestId("track-scroll");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");

  const heading = page.getByRole("row").first().getByText("BPM", { exact: true });
  const top = await heading.boundingBox();

  await scroll.evaluate((el) => {
    el.scrollTop = 2000;
  });
  await expect.poll(async () => (await heading.boundingBox())?.y ?? -1).toBeGreaterThan(0);

  const after = await heading.boundingBox();
  expect(Math.abs((after?.y ?? 0) - (top?.y ?? 0))).toBeLessThan(2);
});

test("a folder in the tree collapses and expands", async ({ page }) => {
  await page.goto("/");
  const tree = page.getByRole("tree");
  await expect(tree.getByRole("treeitem").first()).toBeVisible();

  // A folder the mock nests playlists under.
  const folder = tree.getByRole("treeitem").filter({ hasText: "CURRENT" }).first();
  const before = await tree.getByRole("treeitem").count();

  await folder.getByRole("button").click();
  await expect.poll(async () => tree.getByRole("treeitem").count()).toBeLessThan(before);

  await folder.getByRole("button").click();
  await expect.poll(async () => tree.getByRole("treeitem").count()).toBe(before);
});

test("collapsing a folder does not change the selected playlist", async ({ page }) => {
  // The twisty and the row are different intentions; opening a folder must not
  // navigate.
  await page.goto("/");
  const title = page.getByTestId("browser-title");
  await expect(title).toContainText("Tracks)");
  const before = await title.textContent();

  const folder = page.getByRole("treeitem").filter({ hasText: "CURRENT" }).first();
  await folder.getByRole("button").click();

  await expect(title).toHaveText(before ?? "");
});
