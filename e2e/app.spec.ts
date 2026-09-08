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
  // By column key, not by position: columns can be reordered and hidden, so an
  // index into the row was only ever right for one particular layout.
  const titles = async () =>
    page.locator('[role="gridcell"][data-col="title"]')
        .evaluateAll((cells) => cells.slice(0, 12).map((c) => c.textContent ?? ""));

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
      scroller.locator('[role="gridcell"][data-col="title"]').first().innerText().catch(() => ""),
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
  // Wait for the title to settle rather than for one appearance of a count:
  // the view opens asynchronously, so a single read can catch it mid-change.
  await expect(title).toContainText("Tracks)");
  let before = "";
  await expect
    .poll(async () => {
      const now = (await title.textContent()) ?? "";
      const stable = now === before;
      before = now;
      return stable;
    })
    .toBe(true);

  const folder = page.getByRole("treeitem").filter({ hasText: "CURRENT" }).first();
  await folder.getByRole("button").click();

  await expect(title).toHaveText(before);
});

test("the top bar offers settings, and not rekordbox's export controls", async ({ page }) => {
  // A deliberate divergence: no EXPORT dropdown, no layout or record buttons.
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  await expect(page.getByText("EXPORT", { exact: true })).toHaveCount(0);
});

test("the tree can be resized by dragging the splitter", async ({ page }) => {
  await page.goto("/");
  const tree = page.getByRole("navigation", { name: "Library" });
  const splitter = page.getByRole("separator", { name: /resize the library tree/i });
  await expect(splitter).toBeVisible();

  const before = (await tree.boundingBox())?.width ?? 0;
  const handle = await splitter.boundingBox();
  await page.mouse.move((handle?.x ?? 0) + 2, (handle?.y ?? 0) + 100);
  await page.mouse.down();
  await page.mouse.move((handle?.x ?? 0) + 122, (handle?.y ?? 0) + 100, { steps: 8 });
  await page.mouse.up();

  await expect.poll(async () => (await tree.boundingBox())?.width ?? 0).toBeGreaterThan(before + 80);
});

test("the tree cannot be dragged wide enough to squeeze out the track list", async ({ page }) => {
  await page.goto("/");
  const tree = page.getByRole("navigation", { name: "Library" });
  const splitter = page.getByRole("separator", { name: /resize the library tree/i });

  const handle = await splitter.boundingBox();
  await page.mouse.move((handle?.x ?? 0) + 2, (handle?.y ?? 0) + 100);
  await page.mouse.down();
  // Far past the right edge of the window.
  await page.mouse.move(3000, (handle?.y ?? 0) + 100, { steps: 10 });
  await page.mouse.up();

  const width = (await tree.boundingBox())?.width ?? 0;
  const viewport = page.viewportSize()?.width ?? 1280;
  expect(width).toBeLessThanOrEqual(viewport * 0.5 + 1);
  // And the table is still there and usable.
  await expect(page.getByTestId("track-scroll")).toBeVisible();
});

test("the header menu lists every column and toggles one on", async ({ page }) => {
  await page.goto("/");
  const header = page.getByRole("columnheader", { name: /BPM/ });
  await header.click({ button: "right" });

  const menu = page.getByRole("menu", { name: "Columns" });
  await expect(menu).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: "Auto-size all columns" })).toBeVisible();
  // Transcribed from rekordbox's own menu.
  await expect(menu.getByRole("menuitemcheckbox")).toHaveCount(39);

  await expect(page.getByRole("columnheader", { name: /^Genre/ })).toHaveCount(0);
  await menu.getByRole("menuitemcheckbox", { name: "Genre" }).click();
  await expect(page.getByRole("columnheader", { name: /^Genre/ })).toBeVisible();
});

test("a column can be dragged wider", async ({ page }) => {
  await page.goto("/");
  const header = page.getByRole("columnheader", { name: /BPM/ });
  const before = (await header.boundingBox())?.width ?? 0;

  const grip = header.getByRole("separator", { name: /resize bpm/i });
  const box = await grip.boundingBox();
  await page.mouse.move((box?.x ?? 0) + 2, (box?.y ?? 0) + 5);
  await page.mouse.down();
  await page.mouse.move((box?.x ?? 0) + 82, (box?.y ?? 0) + 5, { steps: 6 });
  await page.mouse.up();

  await expect.poll(async () => (await header.boundingBox())?.width ?? 0).toBeGreaterThan(before + 50);
});

test("resizing a column does not also re-sort the table", async ({ page }) => {
  // The grip lives inside the heading, whose click sorts.
  await page.goto("/");
  const first = page.getByRole("row").nth(1);
  await expect(first).toBeVisible();
  const beforeText = await first.textContent();

  const grip = page.getByRole("columnheader", { name: /BPM/ }).getByRole("separator");
  const box = await grip.boundingBox();
  await page.mouse.move((box?.x ?? 0) + 2, (box?.y ?? 0) + 5);
  await page.mouse.down();
  await page.mouse.move((box?.x ?? 0) + 60, (box?.y ?? 0) + 5, { steps: 4 });
  await page.mouse.up();

  await expect(page.getByRole("row").nth(1)).toHaveText(beforeText ?? "");
});

test("a column can be dragged to a new position", async ({ page }) => {
  await page.goto("/");
  const order = async () =>
    page.getByRole("columnheader").evaluateAll((h) => h.map((c) => c.textContent ?? ""));

  const before = await order();
  const bpm = page.getByRole("columnheader", { name: /BPM/ });
  const key = page.getByRole("columnheader", { name: /^Key/ });
  const from = await bpm.boundingBox();
  const to = await key.boundingBox();

  // Past the threshold, so this is a reorder and not a sort click.
  await page.mouse.move((from?.x ?? 0) + 20, (from?.y ?? 0) + 8);
  await page.mouse.down();
  await page.mouse.move((to?.x ?? 0) + 10, (to?.y ?? 0) + 8, { steps: 8 });
  await page.mouse.up();

  await expect.poll(order).not.toEqual(before);
  // Nothing lost, only moved.
  expect((await order()).sort()).toEqual([...before].sort());
});

test("the column layout survives a reload", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("columnheader", { name: /BPM/ }).click({ button: "right" });
  await page.getByRole("menu", { name: "Columns" })
    .getByRole("menuitemcheckbox", { name: "Genre" }).click();
  await expect(page.getByRole("columnheader", { name: /^Genre/ })).toBeVisible();

  await page.reload();
  await expect(page.getByRole("columnheader", { name: /^Genre/ })).toBeVisible();
});

test("the source rail switches which part of the library the tree shows", async ({ page }) => {
  await page.goto("/");
  const rail = page.getByRole("tablist", { name: "Library sources" });
  await expect(rail.getByRole("tab")).toHaveCount(4);

  // A shortcut, not a filter: the tree keeps showing everything and the rail
  // jumps the selection to that section. browseSetting.xml calls it
  // TreeShortcut, and rekordbox does the same.
  await expect(page.getByRole("treeitem").filter({ hasText: "CURRENT" })).toBeVisible();

  await rail.getByRole("tab", { name: "Collection" }).click();
  await expect(rail.getByRole("tab", { name: "Collection" })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("treeitem", { name: /All Tracks/ })).toHaveAttribute("aria-selected", "true");
  // Still there, not filtered away.
  await expect(page.getByRole("treeitem").filter({ hasText: "CURRENT" })).toBeVisible();
});

test("a source with nothing in it is dimmed rather than hidden", async ({ page }) => {
  // A missing Devices button reads as a broken app; a dimmed one reads as no
  // device plugged in.
  await page.goto("/");
  const devices = page.getByRole("tablist", { name: "Library sources" })
    .getByRole("tab", { name: "Devices" });
  await expect(devices).toBeVisible();
  await expect(devices).toHaveAttribute("data-empty", "true");
});

test("the player shows the track that was clicked", async ({ page }) => {
  await page.goto("/");
  const title = page.getByTestId("player-title");
  await expect(title).toHaveText("No track loaded");

  const firstTitle = page.locator('[role="gridcell"][data-col="title"]').first();
  const text = await firstTitle.innerText();
  await firstTitle.click();

  await expect(title).toHaveText(text);
});

test("the player draws the transport, disabled where there is no backend", async ({ page }) => {
  // In a browser there is no rbl:// scheme to stream from, so the transport is
  // drawn and inert: a player waiting for a backend, not an unfinished panel.
  await page.goto("/");
  const player = page.getByRole("region", { name: "Preview player" });
  await expect(player.getByRole("button", { name: "Play" })).toBeDisabled();
  await expect(player.getByRole("button", { name: "Cue" })).toBeDisabled();
  await expect(page.getByTestId("player-overview")).toBeVisible();
  await expect(page.getByTestId("player-detail")).toBeVisible();
});

test("the player shows a position and a total once a track is chosen", async ({ page }) => {
  await page.goto("/");
  await page.locator('[role="gridcell"][data-col="title"]').first().click();
  // Position over total, from the track's own length before any file loads.
  await expect(page.getByTestId("player-time")).toHaveText(/^\d?\d:\d\d \/ \d?\d:\d\d$/);
});

test("the waveform is a seek target", async ({ page }) => {
  await page.goto("/");
  await page.locator('[role="gridcell"][data-col="title"]').first().click();
  const overview = page.getByTestId("player-overview");
  await expect(overview).toHaveAttribute("role", "slider");
  await expect(overview).toHaveAttribute("aria-valuenow", "0");
  // The maximum is the track's length, so the head has a scale to sit on.
  const max = await overview.getAttribute("aria-valuemax");
  expect(Number(max)).toBeGreaterThan(0);
});

test("the gear opens settings, and Escape closes them", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();

  const dialog = page.getByRole("dialog", { name: "Settings" });
  await expect(dialog).toBeVisible();
  // Real information, not placeholder rows.
  await expect(dialog.getByText("Tracks")).toBeVisible();
  await expect(dialog.getByText("Database version")).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
});

test("settings can put the columns back", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("columnheader", { name: /BPM/ }).click({ button: "right" });
  await page.getByRole("menu", { name: "Columns" })
    .getByRole("menuitemcheckbox", { name: "Genre" }).click();
  await expect(page.getByRole("columnheader", { name: /^Genre/ })).toBeVisible();

  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Reset columns" }).click();
  await expect(page.getByRole("columnheader", { name: /^Genre/ })).toHaveCount(0);
});

test("tracks can be dragged from the browser onto a playlist", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");

  const row = page.getByRole("row").filter({ has: page.getByRole("gridcell") }).first();
  const target = page.getByRole("treeitem").filter({ hasText: "Hardstyle" }).first();

  await row.dragTo(target);

  // The status bar reports what happened rather than leaving it silent.
  await expect(page.getByRole("contentinfo")).toContainText(/Added \d+ track/);
});

test("only playlists offer themselves as a drop target", async ({ page }) => {
  // A folder holds playlists, so dropping tracks into one would have to invent
  // which playlist was meant.
  await page.goto("/");
  const row = page.getByRole("row").filter({ has: page.getByRole("gridcell") }).first();

  await row.hover();
  await page.mouse.down();
  await page.mouse.move(200, 400, { steps: 4 });

  const playlists = page.locator('[role="treeitem"][data-droppable]');
  await expect.poll(async () => playlists.count()).toBeGreaterThan(0);
  const folders = page.getByRole("treeitem").filter({ hasText: "CURRENT" });
  await expect(folders.first()).not.toHaveAttribute("data-droppable", "true");
  await page.mouse.up();
});

test("a track can be rated by clicking its stars", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");

  // Not the first row: it sits under the sticky column header.
  const stars = page.locator('[data-col="rating"] [role="radiogroup"]').nth(3);
  await stars.getByRole("radio", { name: "4 of 5" }).click();

  await expect(page.getByRole("contentinfo")).toContainText("Rated 4 of 5");
});

test("clicking the star already set clears the rating", async ({ page }) => {
  // The only way back to no rating, and how rekordbox behaves.
  await page.goto("/");
  const stars = page.locator('[data-col="rating"] [role="radiogroup"]').nth(3);
  // The mock's ratings are deterministic but not zero, so pick a star the row
  // is not already on: clicking the current one clears rather than sets.
  const checked = await stars.locator('[aria-checked="true"]').count();
  const current = checked === 0
    ? 0
    : Number((await stars.locator('[aria-checked="true"]').getAttribute("aria-label"))?.[0] ?? 0);
  const target = current === 2 ? 4 : 2;

  await stars.getByRole("radio", { name: `${target} of 5` }).click();
  await expect(page.getByRole("contentinfo")).toContainText(`Rated ${target} of 5`);

  await stars.getByRole("radio", { name: `${target} of 5` }).click();
  await expect(page.getByRole("contentinfo")).toContainText("Rating cleared");
});

test("a comment is edited in place, and Escape abandons the edit", async ({ page }) => {
  await page.goto("/");
  const cell = page.locator('[role="gridcell"][data-col="comment"]').nth(3);
  const before = await cell.innerText();

  await cell.dblclick();
  const field = page.getByRole("textbox", { name: "Comment" });
  await field.fill("changed my mind");
  await field.press("Escape");

  await expect(page.locator('[role="gridcell"][data-col="comment"]').nth(3)).toHaveText(before);
  await expect(page.getByRole("contentinfo")).not.toContainText("Comment saved");
});

test("a comment commits on Enter", async ({ page }) => {
  await page.goto("/");
  const cell = page.locator('[role="gridcell"][data-col="comment"]').nth(3);
  await cell.dblclick();
  const field = page.getByRole("textbox", { name: "Comment" });
  await field.fill("5A - Am - 128");
  await field.press("Enter");

  await expect(page.getByRole("contentinfo")).toContainText("Comment saved");
});

test("settings can check for missing files", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();

  const section = page.getByRole("region", { name: "Missing files" });
  await expect(section.getByRole("button", { name: /check for missing files/i })).toBeVisible();
  // Looking must not change anything, so it says so before you press it.
  await expect(section).toContainText("Nothing is changed by looking");

  await section.getByRole("button", { name: /check for missing files/i }).click();
  // The mock has no files behind its rows, so nothing can be missing.
  await expect(section).toContainText("where the library expects it");
});

test("the player marks a track's cues on its waveforms", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  await page.locator('[role="gridcell"][data-col="title"]').nth(3).click();

  const detail = page.getByTestId("player-detail");
  // Four hot cues and one memory cue in the mock's data.
  await expect.poll(async () => detail.locator('[title^="Hot cue"]').count()).toBe(4);
  await expect(detail.locator('[title="Memory cue"]')).toHaveCount(1);
  // Hot cues carry their letter on the detail waveform, where there is room.
  await expect(detail.locator('[title="Hot cue A"]')).toHaveText("A");
});
