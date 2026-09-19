import { expect, test, type Page } from "@playwright/test";

/**
 * The Sync Manager on the mock backend: opened from the foot of the rail,
 * playlists ticked on the left and both sticks on the right, SYNC writes to
 * both and reports on each, and Close puts it away.
 */
async function openManager(page: Page) {
  await page.goto("/");
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  await page.getByRole("button", { name: "Sync Manager" }).click();
  const dialog = page.getByRole("dialog", { name: "Sync Manager" });
  await expect(dialog).toBeVisible();
  return dialog;
}

test("the rail opens it with the library's playlists on the left and the devices on the right", async ({ page }) => {
  const dialog = await openManager(page);
  const tree = dialog.getByRole("tree", { name: "Playlists" });
  // The mock's top folders, open one level: their playlists show, the third
  // folder is empty.
  await expect(tree.getByRole("treeitem").first()).toHaveText(/CURRENT/);
  await expect(tree.getByRole("checkbox", { name: "Melodic Vox" })).toBeVisible();
  await expect(tree.getByRole("treeitem", { name: /All Tracks/ })).toHaveCount(0);
  const devices = dialog.getByRole("list", { name: "Devices" });
  await expect(devices.getByRole("checkbox", { name: "DJ STICK", exact: true })).toBeVisible();
  await expect(devices.getByRole("checkbox", { name: "TEST", exact: true })).toBeVisible();
  // Nothing ticked yet, so nothing to sync.
  await expect(dialog.getByRole("button", { name: "SYNC" })).toBeDisabled();
});

test("ticking a folder ticks its playlists, and a device shows what it holds", async ({ page }) => {
  const dialog = await openManager(page);
  const tree = dialog.getByRole("tree", { name: "Playlists" });
  await tree.getByRole("checkbox", { name: "CURRENT" }).check();
  await expect(tree.getByRole("checkbox", { name: "Melodic Vox" })).toBeChecked();
  await expect(tree.getByRole("checkbox", { name: "Hardstyle" })).toBeChecked();
  await tree.getByRole("checkbox", { name: "Hardstyle" }).uncheck();
  await expect(tree.getByRole("checkbox", { name: "CURRENT" })).toHaveAttribute("aria-checked", "mixed");

  // TEST is rekordbox's stick: it holds playlists but remembers no selection.
  await dialog.getByRole("checkbox", { name: "TEST", exact: true }).check();
  const library = dialog.getByLabel("TEST library");
  await expect(library).toContainText("Device Library");
  await expect(library).toContainText("Main Set");
  await expect(library).toContainText("free of");
});

test("SYNC writes the ticked playlists to both sticks, reports on each, and Close closes it", async ({ page }) => {
  const dialog = await openManager(page);
  const tree = dialog.getByRole("tree", { name: "Playlists" });
  await tree.getByRole("checkbox", { name: "Melodic Vox" }).check();
  await dialog.getByRole("checkbox", { name: "DJ STICK", exact: true }).check();
  await dialog.getByRole("checkbox", { name: "TEST", exact: true }).check();
  const sync = dialog.getByRole("button", { name: "SYNC" });
  await expect(sync).toBeEnabled();
  await sync.click();

  const status = dialog.getByRole("status");
  await expect(status).toContainText(/Exported \d+ tracks to DJ STICK/);
  await expect(status).toContainText(/Exported \d+ tracks to TEST/);
  // Both sticks now hold the one playlist, and say so.
  await expect(dialog.getByLabel("DJ STICK library")).toContainText("Melodic Vox");
  await expect(dialog.getByLabel("TEST library")).toContainText("Melodic Vox");
  await expect(dialog.getByLabel("TEST library")).not.toContainText("Main Set");

  // The footer's Close, not the ✕ in the title bar, which is also named Close.
  await dialog.getByRole("button", { name: "Close" }).filter({ hasText: "Close" }).click();
  await expect(dialog).toBeHidden();

  // The shell's device tree saw the write: DJ STICK now holds an export.
  await page.getByRole("tablist", { name: "Library sources" }).getByRole("tab", { name: "Devices" }).click();
  await page.getByRole("treeitem", { name: /DJ STICK/ }).click();
  await expect(page.getByRole("region", { name: "Device DJ STICK" })).toContainText("last synced");
});

test("ticking a stick again brings back what it was last synced with", async ({ page }) => {
  const dialog = await openManager(page);
  const tree = dialog.getByRole("tree", { name: "Playlists" });
  await tree.getByRole("checkbox", { name: "Hardstyle" }).check();
  await dialog.getByRole("checkbox", { name: "DJ STICK", exact: true }).check();
  await dialog.getByRole("button", { name: "SYNC" }).click();
  await expect(dialog.getByRole("status")).toContainText(/DJ STICK/);

  // Untick everything, tick the stick: its last selection comes back.
  await dialog.getByRole("checkbox", { name: "DJ STICK", exact: true }).uncheck();
  await tree.getByRole("checkbox", { name: "Hardstyle" }).uncheck();
  await expect(tree.getByRole("checkbox", { name: "Hardstyle" })).not.toBeChecked();
  await dialog.getByRole("checkbox", { name: "DJ STICK", exact: true }).check();
  await expect(tree.getByRole("checkbox", { name: "Hardstyle" })).toBeChecked();
});
