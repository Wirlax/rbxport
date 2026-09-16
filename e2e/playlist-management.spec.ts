import { expect, test, type Locator, type Page } from "@playwright/test";

/**
 * Managing playlists from the interface: made from the tree's menu, filled
 * by a drop, emptied from the track's menu, and deleted again. The mock
 * backend owns the tree and the memberships the way Rust does, so what the
 * list shows after each step is the backend's answer, not the view's guess.
 */

/** The mock stands in for a library rekordbox holds unless told otherwise. */
async function open(page: Page) {
  await page.goto("/?writable=1");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
}

function item(page: Page, name: string): Locator {
  return page.getByRole("treeitem").filter({ hasText: name }).first();
}

/** The indent the tree draws for a node, which is how deep it is. */
async function depthOf(node: Locator): Promise<number> {
  const padding = await node.evaluate((el) => parseFloat(getComputedStyle(el).paddingLeft));
  return (padding - 14) / 20;
}

async function chooseFromTreeMenu(page: Page, node: Locator, entry: string) {
  await node.click({ button: "right" });
  const menu = page.getByRole("menu", { name: /^(Playlist|Folder)$/ });
  await expect(menu).toBeVisible();
  await menu.getByRole("menuitem", { name: entry, exact: true }).click();
  await expect(menu).toBeHidden();
}

const rows = (page: Page) => page.getByRole("row").filter({ has: page.getByRole("gridcell") });

test("a playlist made from another's menu lands in the same folder, empty", async ({ page }) => {
  await open(page);
  const sibling = item(page, "Melodic Vox");
  const siblingDepth = await depthOf(sibling);

  await chooseFromTreeMenu(page, sibling, "Create New Playlist");
  await expect(page.getByRole("contentinfo")).toContainText("Created New playlist.");

  // rekordbox's default name, beside the playlist the menu was opened on
  // rather than at the top of the tree.
  const made = item(page, "New playlist");
  await expect(made).toBeVisible();
  expect(await depthOf(made)).toBe(siblingDepth);

  await made.click();
  await expect(page.getByTestId("browser-title")).toHaveText("New playlist (0 Tracks)");
  await expect(rows(page)).toHaveCount(0);
});

test("a folder made from a folder's menu goes inside it", async ({ page }) => {
  await open(page);
  const folder = item(page, "CURRENT");
  const folderDepth = await depthOf(folder);

  await chooseFromTreeMenu(page, folder, "Create New Folder");
  await expect(page.getByRole("contentinfo")).toContainText("Created New folder.");
  const made = item(page, "New folder");
  await expect(made).toBeVisible();
  expect(await depthOf(made)).toBe(folderDepth + 1);
});

test("a track dropped on a new playlist is in it, and its menu takes it out again", async ({
  page,
}) => {
  await open(page);
  await chooseFromTreeMenu(page, item(page, "Hardstyle"), "Create New Playlist");
  const made = item(page, "New playlist");
  await expect(made).toBeVisible();

  // From All Tracks, the second row, so the title is known before the drop.
  const source = rows(page).nth(1);
  const title = (await source.locator('[data-col="title"]').innerText()).trim();
  await source.dragTo(made);
  await expect(page.getByRole("contentinfo")).toContainText("Added 1 track to New playlist.");
  // Dropping it again changes nothing: a playlist holds a track once.
  await source.dragTo(made);
  await expect(page.getByRole("contentinfo")).toContainText("Added 1 track to New playlist.");

  await made.click();
  await expect(page.getByTestId("browser-title")).toHaveText("New playlist (1 Tracks)");
  const only = rows(page).first();
  await expect(only.locator('[data-col="title"]')).toHaveText(title);
  // Numbered by its place in the playlist, not the collection.
  await expect(only.locator('[data-col="trackNo"]')).toHaveText("1");

  // Out again from the track's own menu, which is live only inside a playlist.
  await rows(page).first().locator('[data-col="title"]').click({ button: "right" });
  const menu = page.getByRole("menu", { name: "Track" });
  const remove = menu.getByRole("menuitem", { name: "Remove from Playlist" });
  await expect(remove).toBeEnabled();
  await remove.click();
  await expect(page.getByRole("contentinfo")).toContainText("Removed 1 track.");
  await expect(rows(page)).toHaveCount(0);
  await expect(page.getByTestId("browser-title")).toHaveText("New playlist (0 Tracks)");
});

test("dragging a selection drops every row in it, not just the one under the hand", async ({ page }) => {
  await open(page);
  await chooseFromTreeMenu(page, item(page, "Hardstyle"), "Create New Playlist");
  const made = item(page, "New playlist");
  await expect(made).toBeVisible();

  // Three rows, shift-click; the press that starts the drag lands on a row
  // already selected, and must not collapse the selection to it.
  await rows(page).nth(1).locator('[data-col="title"]').click();
  await rows(page).nth(3).locator('[data-col="title"]').click({ modifiers: ["Shift"] });
  await expect(page.getByText("Selected: 3 Tracks")).toBeVisible();
  await rows(page).nth(2).dragTo(made);
  await expect(page.getByRole("contentinfo")).toContainText("Added 3 tracks to New playlist.");
  await expect(page.getByText("Selected: 3 Tracks")).toBeVisible();

  // A plain click on a selected row, released without a drag, does collapse
  // to it — the ordinary click still works.
  await rows(page).nth(2).locator('[data-col="title"]').click();
  await expect(page.getByText("Selected: 1 Track")).toBeVisible();

  await made.click();
  await expect(page.getByTestId("browser-title")).toHaveText("New playlist (3 Tracks)");
});

test("removing from an existing playlist shortens it by exactly the selection", async ({ page }) => {
  await open(page);
  const playlist = item(page, "Eurodance");
  await playlist.click();
  const title = page.getByTestId("browser-title");
  await expect(title).toContainText("Eurodance (");
  const before = Number(/\((\d+) Tracks\)/.exec(await title.innerText())?.[1]);
  expect(before).toBeGreaterThan(2);

  const first = rows(page).first();
  const second = rows(page).nth(1);
  const third = (await rows(page).nth(2).locator('[data-col="title"]').innerText()).trim();

  // Two rows, shift-click, and a right-click on the selection keeps it so
  // the menu acts on both — not on the one under the pointer.
  await first.locator('[data-col="title"]').click();
  await second.locator('[data-col="title"]').click({ modifiers: ["Shift"] });
  await expect(page.getByText("Selected: 2 Tracks")).toBeVisible();
  await second.locator('[data-col="title"]').click({ button: "right" });
  await expect(page.getByText("Selected: 2 Tracks")).toBeVisible();
  await page.getByRole("menu", { name: "Track" }).getByRole("menuitem", { name: "Remove from Playlist" }).click();

  await expect(page.getByRole("contentinfo")).toContainText("Removed 2 tracks.");
  await expect(title).toHaveText(`Eurodance (${before - 2} Tracks)`);
  // What was third is first: the two above it went, and nothing else moved.
  await expect(rows(page).first().locator('[data-col="title"]')).toHaveText(third);
  await expect(rows(page).first().locator('[data-col="trackNo"]')).toHaveText("1");
});

test("deleting a playlist takes it out of the tree and the view moves off it", async ({ page }) => {
  await open(page);
  await chooseFromTreeMenu(page, item(page, "Melodic Vox"), "Create New Playlist");
  const made = item(page, "New playlist");
  await made.click();
  await expect(page.getByTestId("browser-title")).toHaveText("New playlist (0 Tracks)");

  await chooseFromTreeMenu(page, made, "Delete Playlist");
  await expect(page.getByRole("contentinfo")).toContainText("Deleted New playlist.");
  await expect(page.getByRole("treeitem").filter({ hasText: "New playlist" })).toHaveCount(0);
  // The selection does not stay on a list that no longer exists.
  await expect(page.getByTestId("browser-title")).not.toContainText("New playlist");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");

  // A playlist that was there from the start goes the same way.
  await chooseFromTreeMenu(page, item(page, "Drum and Bass"), "Delete Playlist");
  await expect(page.getByRole("treeitem").filter({ hasText: "Drum and Bass" })).toHaveCount(0);
  await expect(item(page, "Eurodance")).toBeVisible();
});

test("a folder's menu deletes the folder", async ({ page }) => {
  await open(page);
  await chooseFromTreeMenu(page, item(page, "CURRENT"), "Create New Folder");
  const made = item(page, "New folder");
  await expect(made).toBeVisible();
  await made.click({ button: "right" });
  const menu = page.getByRole("menu", { name: "Folder" });
  await expect(menu.getByRole("menuitem", { name: "Delete Folder" })).toBeVisible();
  await menu.getByRole("menuitem", { name: "Delete Folder" }).click();
  await expect(page.getByRole("treeitem").filter({ hasText: "New folder" })).toHaveCount(0);
});

test("with the library protected, the tree menu will not create or delete", async ({ page }) => {
  await open(page);
  await page.getByRole("banner").getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Preferences" });
  await dialog.getByRole("tab", { name: "Advanced" }).click();
  await dialog.getByRole("tab", { name: "Browse", exact: true }).click();
  await dialog.getByRole("switch", { name: "Protect library edit." }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("contentinfo")).toContainText(/read-only/i);

  await item(page, "Melodic Vox").click({ button: "right" });
  const menu = page.getByRole("menu", { name: "Playlist" });
  for (const label of ["Create New Playlist", "Create New Folder", "Delete Playlist"]) {
    await expect(menu.getByRole("menuitem", { name: label, exact: true })).toBeDisabled();
  }
  // Reading is still fine.
  await expect(menu.getByRole("menuitem", { name: "Export Playlist" })).toBeEnabled();
  await page.keyboard.press("Escape");

  await item(page, "Melodic Vox").click();
  await rows(page).first().locator('[data-col="title"]').click({ button: "right" });
  await expect(
    page.getByRole("menu", { name: "Track" }).getByRole("menuitem", { name: "Remove from Playlist" }),
  ).toBeDisabled();
});

test("a rating set after a playlist edit stays set once the reload lands", async ({ page }) => {
  // A playlist edit and a rating both end in a reload the backend announces
  // with its generation. The view must key on that number alone: a count the
  // interface bumped itself on the playlist edit could land on the same value
  // the backend then announced for the rating, and a generation that does not
  // change is a page that is not refetched — the star lit for a moment and
  // went out when the overlay was dropped.
  await open(page);
  await chooseFromTreeMenu(page, item(page, "Melodic Vox"), "Create New Playlist");
  await expect(page.getByRole("contentinfo")).toContainText("Created New playlist.");
  await chooseFromTreeMenu(page, item(page, "CURRENT"), "Create New Folder");
  await expect(page.getByRole("contentinfo")).toContainText("Created New folder.");

  // A star the row does not already have: clicking the set one clears it.
  const stars = rows(page).nth(3).locator('[data-col="rating"] [role="radiogroup"]');
  const had = (await stars.getByRole("radio", { checked: true }).count()) > 0
    ? await stars.getByRole("radio", { checked: true }).getAttribute("aria-label")
    : null;
  const pick = had === "3 of 5" ? 2 : 3;
  await stars.getByRole("radio", { name: `${pick} of 5` }).click();
  await expect(page.getByRole("contentinfo")).toContainText(`Rated ${pick} of 5`);
  // Past the moment the optimistic overlay is dropped for the re-read rows.
  await page.waitForTimeout(500);
  await expect(stars.getByRole("radio", { name: `${pick} of 5` })).toHaveAttribute("aria-checked", "true");
  await expect(stars).toHaveText(`${"★".repeat(pick)}${"☆".repeat(5 - pick)}`);
});

test("a playlist is renamed in the row, and Escape puts the old name back", async ({ page }) => {
  await open(page);
  await chooseFromTreeMenu(page, item(page, "Melodic Vox"), "Rename Playlist");

  // The name becomes a field with the whole of it selected, so typing
  // replaces it rather than appending to it.
  const field = page.getByRole("textbox", { name: "Rename Melodic Vox" });
  await expect(field).toBeVisible();
  await expect(field).toHaveValue("Melodic Vox");

  await field.fill("Melodic Vox 2026");
  await field.press("Enter");
  await expect(page.getByRole("contentinfo")).toContainText("Renamed to Melodic Vox 2026.");
  await expect(item(page, "Melodic Vox 2026")).toBeVisible();
  await expect(field).toHaveCount(0);

  // Escape is a cancel: the typing goes, the name stays.
  await chooseFromTreeMenu(page, item(page, "Melodic Vox 2026"), "Rename Playlist");
  const again = page.getByRole("textbox", { name: "Rename Melodic Vox 2026" });
  await again.fill("Something else entirely");
  await again.press("Escape");
  await expect(again).toHaveCount(0);
  await expect(item(page, "Melodic Vox 2026")).toBeVisible();
  await expect(page.getByRole("treeitem").filter({ hasText: "Something else entirely" })).toHaveCount(0);
});

test("renaming is refused while rekordbox holds the library", async ({ page }) => {
  // Every write is greyed rather than raced, which is the rule the rest of
  // the menu follows.
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  await item(page, "Melodic Vox").click({ button: "right" });
  const menu = page.getByRole("menu", { name: "Playlist" });
  await expect(menu.getByRole("menuitem", { name: "Rename Playlist" })).toBeDisabled();
});

/** The tree's node names, without the track counts. */
const treeNames = async (page: Page) =>
  (await page.getByRole("treeitem").allTextContents()).map((s) =>
    s.replace(/\(\d+\)$/, "").trim(),
  );

test("a playlist dragged down the tree takes the place it was dropped at", async ({ page }) => {
  await open(page);
  const before = await treeNames(page);
  const from = before.indexOf("Melodic Vox");
  expect(from).toBeGreaterThan(0);
  // Two rows down, which is a sibling at the same level.
  const target = before[from + 2]!;

  const dst = item(page, target);
  const box = await dst.boundingBox();
  if (!box) throw new Error("the target row is not on screen");
  await item(page, "Melodic Vox").dragTo(dst, {
    targetPosition: { x: 40, y: box.height - 2 },
  });

  await expect(page.getByRole("contentinfo")).toContainText("Moved Melodic Vox.");
  await expect
    .poll(async () => (await treeNames(page)).indexOf("Melodic Vox"))
    .toBe(before.indexOf(target));
  // Nothing was lost: the same nodes, in a different order.
  expect([...(await treeNames(page))].sort()).toEqual([...before].sort());
});

test("a playlist dropped into a folder goes inside it", async ({ page }) => {
  await open(page);
  await chooseFromTreeMenu(page, item(page, "Melodic Vox"), "Create New Folder");
  await expect(page.getByRole("contentinfo")).toContainText("Created New folder.");

  const folder = item(page, "New folder");
  const depthBefore = await depthOf(item(page, "Melodic Vox"));
  const box = await folder.boundingBox();
  if (!box) throw new Error("the folder is not on screen");
  // The middle of a folder means inside it; its edges mean beside it.
  await item(page, "Melodic Vox").dragTo(folder, {
    targetPosition: { x: 40, y: box.height / 2 },
  });

  await expect(page.getByRole("contentinfo")).toContainText("Moved Melodic Vox.");
  await expect.poll(async () => depthOf(item(page, "Melodic Vox"))).toBe(depthBefore + 1);
});

test("the tree will not be rearranged while rekordbox holds the library", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  // Undraggable rather than refused on the drop: a write that cannot happen
  // should not be offered in the first place.
  await expect(item(page, "Melodic Vox")).not.toHaveAttribute("draggable", "true");
});
