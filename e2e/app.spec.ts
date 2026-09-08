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
    .poll(async () => Math.round(await scroll.evaluate((el) => el.scrollLeft)))
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

test("the top bar offers settings on the right, and not rekordbox's export controls", async ({ page }) => {
  // A deliberate divergence: no EXPORT dropdown, no layout or record buttons.
  await page.goto("/");
  const settings = page.getByRole("button", { name: "Settings" });
  await expect(settings).toBeVisible();
  await expect(page.getByText("EXPORT", { exact: true })).toHaveCount(0);

  // At the right-hand end, past the clock.
  const bar = await page.getByRole("banner").boundingBox();
  const gear = await settings.boundingBox();
  expect((gear?.x ?? 0) - (bar?.x ?? 0)).toBeGreaterThan((bar?.width ?? 0) / 2);
});

test("the player's cue and play sit at the foot of the transport", async ({ page }) => {
  // Measured off design/reference/macos/playlist-player@2x.png: the skip
  // buttons are at the top of the column, then a gap, then the two circles.
  await page.goto("/");
  const player = await page.getByRole("region", { name: "Preview player" }).boundingBox();
  const cue = await page.getByRole("button", { name: "Cue" }).boundingBox();
  const play = await page
    .getByRole("region", { name: "Preview player" })
    .getByRole("button", { name: "Play", exact: true })
    .boundingBox();

  expect((cue?.y ?? 0) - (player?.y ?? 0)).toBeGreaterThan((player?.height ?? 0) / 2);
  expect(play?.y ?? 0).toBeGreaterThan(cue?.y ?? 0);
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

test("a connected device appears under Devices", async ({ page }) => {
  // A missing Devices button reads as a broken app, and a dimmed one reads as
  // nothing plugged in — so with a stick connected it must be neither.
  await page.goto("/");
  const devices = page.getByRole("tablist", { name: "Library sources" })
    .getByRole("tab", { name: "Devices" });
  await expect(devices).toBeVisible();
  await expect(devices).not.toHaveAttribute("data-empty", "true");

  await devices.click();
  await expect(page.getByRole("treeitem", { name: /DJ STICK/ })).toBeVisible();
});

test("a device shows what is on it, and a second write only syncs the difference", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("tablist", { name: "Library sources" })
    .getByRole("tab", { name: "Devices" })
    .click();
  await page.getByRole("treeitem", { name: /DJ STICK/ }).click();

  const panel = page.getByRole("region", { name: "Device DJ STICK" });
  await expect(panel).toBeVisible();
  await expect(panel).toContainText("No export on this device yet.");
  await expect(panel.getByRole("progressbar", { name: "Space used" })).toHaveAttribute(
    "aria-valuenow",
    "25",
  );

  // First write: everything goes.
  await expect(panel.getByRole("button", { name: "Export" })).toBeVisible();
  await panel.getByRole("button", { name: "Export" }).click();
  await expect(page.getByRole("contentinfo")).toContainText(/Exported \d+ tracks to DJ STICK/);
  await expect(panel).toContainText("Only what changed will be copied.");

  // Second write to the same stick: nothing changed, so nothing is copied.
  await panel.getByRole("button", { name: "Sync" }).click();
  await expect(page.getByRole("contentinfo")).toContainText(/Synced DJ STICK: \d+ unchanged/);
});

test("a track loads into the player on a double-click, not a click", async ({ page }) => {
  await page.goto("/");
  const title = page.getByTestId("player-title");
  await expect(title).toHaveText("No track loaded");

  const firstTitle = page.locator('[role="gridcell"][data-col="title"]').first();
  const text = await firstTitle.innerText();

  // Selecting and playing are different intentions: arrowing down a playlist
  // to see what is in it must not load every track on the way past.
  await firstTitle.click();
  await expect(page.locator('[role="row"][data-selected]')).toHaveCount(1);
  await expect(title).toHaveText("No track loaded");

  await firstTitle.dblclick();
  await expect(title).toHaveText(text);
});

test("editing a comment does not also load the track", async ({ page }) => {
  // The comment cell opens its editor on a double-click, which would otherwise
  // reach the row underneath and start playback.
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  await page.locator('[role="gridcell"][data-col="comment"]').first().dblclick();
  await expect(page.locator('[role="gridcell"][data-col="comment"] input')).toBeVisible();
  await expect(page.getByTestId("player-title")).toHaveText("No track loaded");
});

test("a playlist numbers its rows in the order they are in", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  await page.getByRole("treeitem").filter({ hasText: "Melodic Vox" }).first().click();

  // The position is what makes a playlist's order readable, so it is always
  // there — rekordbox does not offer it in the header menu either.
  const numbers = page.locator('[role="gridcell"][data-col="trackNo"]');
  await expect(numbers.first()).toHaveText("1");
  await expect(numbers.nth(1)).toHaveText("2");
  await expect(numbers.nth(2)).toHaveText("3");

  // And it cannot be turned off.
  await page.getByRole("columnheader", { name: "Track Title" }).click({ button: "right" });
  await expect(page.getByRole("menu")).toBeVisible();
  await expect(page.getByRole("menuitemcheckbox", { name: "#" })).toHaveCount(0);
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
  await page.locator('[role="gridcell"][data-col="title"]').first().dblclick();
  // Position over total, from the track's own length before any file loads.
  await expect(page.getByTestId("player-time")).toHaveText(/^\d?\d:\d\d \/ \d?\d:\d\d$/);
});

test("the waveform is a seek target", async ({ page }) => {
  await page.goto("/");
  await page.locator('[role="gridcell"][data-col="title"]').first().dblclick();
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
  await page.locator('[role="gridcell"][data-col="title"]').nth(3).dblclick();

  // The overview spans the whole track, so it holds every cue. The detail is a
  // window and holds only what falls inside it.
  const overview = page.getByTestId("player-overview");
  await expect.poll(async () => overview.locator('[title^="Hot cue"]').count()).toBe(4);
  await expect(overview.locator('[title="Memory cue"]')).toHaveCount(1);
  // The detail starts at the top of the track, spanning its first 8%. The
  // mock's memory cue sits at 2% and its first hot cue at 12%, so exactly one
  // marker belongs there — which is what makes it a window rather than a
  // second copy of the overview.
  const detail = page.getByTestId("player-detail");
  await expect(detail.locator('[title="Memory cue"]')).toHaveCount(1);
  await expect(detail.locator('[title^="Hot cue"]')).toHaveCount(0);
});

test("settings offers adding music, and says what it will not do", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();

  const section = page.getByRole("region", { name: "Add music" });
  await expect(section.getByRole("button", { name: /add files/i })).toBeVisible();
  // Honest about not running analysis, rather than leaving it a surprise.
  await expect(section).toContainText("Analysis is not run");
});

test("a rating appears at once rather than waiting for the reload", async ({ page }) => {
  // A write makes the backend re-read the library; waiting for that before the
  // star fills in feels broken.
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");

  const stars = page.locator('[data-col="rating"] [role="radiogroup"]').nth(3);
  await stars.getByRole("radio", { name: "5 of 5" }).click();
  await expect(stars.getByRole("radio", { name: "5 of 5" })).toHaveAttribute("aria-checked", "true");
});

test("an edited comment shows before the backend catches up", async ({ page }) => {
  await page.goto("/");
  const cell = page.locator('[role="gridcell"][data-col="comment"]').nth(3);
  await cell.dblclick();
  const field = page.getByRole("textbox", { name: "Comment" });
  await field.fill("shown at once");
  await field.press("Enter");

  await expect(page.locator('[role="gridcell"][data-col="comment"]').nth(3))
    .toHaveText("shown at once");
});

test("each kind of table remembers its own columns", async ({ page }) => {
  // rekordbox keys these by context in browseSetting.xml, not per playlist, so
  // browsing a second playlist does not start from scratch.
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");

  // Add Genre while a playlist is selected.
  await page.getByRole("columnheader", { name: /BPM/ }).click({ button: "right" });
  await page.getByRole("menu", { name: "Columns" })
    .getByRole("menuitemcheckbox", { name: "Genre" }).click();
  await expect(page.getByRole("columnheader", { name: /^Genre/ })).toBeVisible();

  // The collection is a different context and keeps the defaults.
  await page.getByRole("treeitem", { name: /All Tracks/ }).click();
  await expect(page.getByRole("columnheader", { name: /^Genre/ })).toHaveCount(0);

  // Back to a playlist and the change is still there.
  await page.getByRole("treeitem").filter({ hasText: "Melodic Vox" }).first().click();
  await expect(page.getByRole("columnheader", { name: /^Genre/ })).toBeVisible();
});

test("analysing a selection reports progress and can be stopped", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");

  // Select a run of rows, then analyse them.
  const rows = page.getByRole("row").filter({ has: page.getByRole("gridcell") });
  await rows.nth(2).click();
  await rows.nth(9).click({ modifiers: ["Shift"] });
  await page.keyboard.press("a");

  const status = page.getByRole("contentinfo");
  await expect(status).toContainText(/Analyzing: \d+ of \d+/);
  await expect(status.getByRole("button", { name: "Stop" })).toBeVisible();

  await status.getByRole("button", { name: "Stop" }).click();
  // The running track finishes, then the run ends and the readout goes back.
  await expect(status.getByRole("button", { name: "Stop" })).toHaveCount(0);
});

test("a track that cannot be analysed does not stop the run", async ({ page }) => {
  // The mock fails every seventh track, so a long enough run hits one.
  await page.goto("/");
  const rows = page.getByRole("row").filter({ has: page.getByRole("gridcell") });
  await rows.nth(0).click();
  await rows.nth(14).click({ modifiers: ["Shift"] });
  await page.keyboard.press("a");

  const status = page.getByRole("contentinfo");
  await expect(status).toContainText("failed", { timeout: 15_000 });
  // And it carried on rather than stopping there.
  await expect(status.getByRole("button", { name: "Stop" })).toHaveCount(0, { timeout: 15_000 });
});

test("settings can look for link devices, and says why a browser cannot", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();

  const section = page.getByRole("region", { name: "Link" });
  // Honest about listening only, rather than implying it appears as a source.
  await expect(section).toContainText("Nothing is sent");

  await section.getByRole("button", { name: /look for devices/i }).click();
  await expect(section).toContainText("browser has no access to the network");
});

test("the detail waveform shows a window, not the whole track again", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  await page.locator('[role="gridcell"][data-col="title"]').nth(3).dblclick();

  const overview = page.getByTestId("player-overview");
  const detail = page.getByTestId("player-detail");

  // The overview holds every cue; the detail holds only those inside its
  // window, which is a small slice of the track.
  await expect.poll(async () => overview.locator('[title^="Hot cue"]').count()).toBe(4);
  await expect
    .poll(async () => detail.locator('[title^="Hot cue"], [title="Memory cue"]').count())
    .toBeLessThan(5);
});

test("the detail waveform draws a beat grid with heavier downbeats", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  await page.locator('[role="gridcell"][data-col="title"]').nth(3).dblclick();

  const detail = page.getByTestId("player-detail");
  // A window of 8% of a ~5 minute track at ~128 BPM is on the order of 50
  // beats: enough to be a grid, few enough to be readable.
  await expect.poll(async () => detail.locator("span").count()).toBeGreaterThan(10);

  // Downbeats are a distinct, heavier mark rather than every line the same.
  const weights = await detail.locator("span").evaluateAll((els) =>
    [...new Set(els.map((e) => getComputedStyle(e).backgroundColor))],
  );
  expect(weights.length).toBeGreaterThan(1);
});

test("the waveforms follow the window rather than stretching a fixed canvas", async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 900 });
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  await page.locator('[role="gridcell"][data-col="title"]').nth(3).dblclick();

  const canvas = page.getByTestId("player-detail").locator("canvas");
  await expect(canvas).toBeVisible();
  const narrow = await canvas.evaluate((el: HTMLCanvasElement) => el.width);
  expect(narrow).toBeGreaterThan(0);

  // A canvas whose backing store does not grow is simply upscaled by CSS, and
  // the waveform goes soft on a wide window.
  await page.setViewportSize({ width: 1800, height: 1130 });
  await expect
    .poll(async () => canvas.evaluate((el: HTMLCanvasElement) => el.width))
    .toBeGreaterThan(narrow);
});

test("a playlist offers export, and a folder does not", async ({ page }) => {
  // A folder holds playlists, so exporting one would have to invent which.
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");

  // The mock has no filesystem, so the picker resolves to nothing and the
  // status bar goes quiet again rather than claiming an export happened.
  const playlist = page.getByRole("treeitem").filter({ hasText: "Melodic Vox" }).first();
  await playlist.click({ button: "right" });
  await expect(page.getByRole("contentinfo")).not.toContainText("Exported");

  const folder = page.getByRole("treeitem").filter({ hasText: "CURRENT" }).first();
  await folder.click({ button: "right" });
  await expect(page.getByRole("contentinfo")).not.toContainText("Exporting");
});

test("the information window shows the focused track and closes again", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");

  // Closed by default, which is what browseSetting.xml records for the real
  // rekordbox — so the default view is the one the captures were taken of.
  const panel = page.getByRole("complementary", { name: "Information" });
  await expect(panel).toBeHidden();

  const title = await page.locator('[role="gridcell"][data-col="title"]').nth(3).innerText();
  await page.locator('[role="gridcell"][data-col="title"]').nth(3).dblclick();

  // Fired the way the native menu bar fires it; a browser has none.
  await page.evaluate(() => (window as unknown as { __menu: (id: string) => void }).__menu("info"));
  await expect(panel).toBeVisible();
  await expect(panel).toContainText(title);
  await expect(panel).toContainText("BPM");

  // It gives up its width to the browser rather than overlaying it.
  const browser = page.getByTestId("browser-title");
  await expect(browser).toBeVisible();

  await panel.getByRole("button", { name: "Close" }).click();
  await expect(panel).toBeHidden();
});

test("the sub-browser keeps its own selection, separate from the main one", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");

  // Closed by default, as browseSetting.xml records for the real rekordbox.
  const sub = page.getByRole("region", { name: "Sub-Browser" });
  await expect(sub).toBeHidden();
  await page.evaluate(() => (window as unknown as { __menu: (id: string) => void }).__menu("sub"));
  await expect(sub).toBeVisible();

  // Pick a playlist in the main tree, and a different one in the sub-browser's.
  const main = page.getByRole("tree").first();
  await main.getByRole("treeitem").filter({ hasText: "Melodic Vox" }).first().click();
  await expect(page.getByTestId("browser-title").first()).toContainText("Melodic Vox");

  const subTree = sub.getByRole("tree");
  const other = subTree.getByRole("treeitem").filter({ hasText: "All Tracks" }).first();
  await other.click();

  // The point of a sub-browser: two selections at once. The main browser must
  // not have followed the sub-browser's click.
  await expect(page.getByTestId("browser-title").first()).toContainText("Melodic Vox");
  await expect(sub.getByTestId("browser-title")).toContainText("All Tracks");

  await sub.getByRole("button", { name: "Close" }).click();
  await expect(sub).toBeHidden();
});

test("a library that is still loading arrives when it is ready", async ({ page }) => {
  // The backend loads on its own thread, so the first request can easily
  // arrive before there is anything to answer with. `?slow` holds the mock's
  // library back until it is released, which is that race made deliberate.
  await page.goto("/?slow=1");

  const status = page.getByRole("contentinfo");
  await expect(status).toContainText("Loading the library…");

  // The mock is built on the first backend call, so the release hook appears
  // a beat after the page does.
  await page.waitForFunction(() => "__libraryReady" in window);
  await page.evaluate(() => (window as unknown as { __libraryReady: () => void }).__libraryReady());

  // Without a retry on the ready event this stays on "Loading…" forever, which
  // is what a 38,681-track collection in a debug build actually did.
  await expect(status).toContainText("Tracks");
  await expect(page.locator('[role="row"]').first()).toBeVisible();
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
});
