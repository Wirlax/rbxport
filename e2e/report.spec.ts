import { expect, test } from "@playwright/test";

test("bug reports open the attachment externally and honor the usage-statistics default", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("rbl.preferences", JSON.stringify({ advanced: { usageStats: false } })));
  await page.goto("/");
  await page.getByRole("contentinfo").getByRole("button", { name: "Report bug", exact: true }).click();
  const report = page.getByRole("dialog", { name: "Report bug", exact: true });
  await expect(report.getByRole("checkbox")).not.toBeChecked();
  await expect(report.getByRole("button", { name: "Save report ZIP…" })).toBeDisabled();
  await report.getByLabel("What happened?").fill("Playback stopped after loading a track.");
  await expect(report.getByRole("button", { name: "Save report ZIP…" })).toBeEnabled();
  await report.getByRole("checkbox").check();
  await report.getByRole("button", { name: "Open attachment" }).click();
  await expect(report.getByRole("alert")).toHaveText("Opening the text editor requires the desktop app.");
  await report.getByRole("checkbox").uncheck();
  await expect(report.getByRole("button", { name: "Open attachment" })).toBeDisabled();
  await report.getByRole("button", { name: "Close", exact: true }).click();
  await expect(report).toHaveCount(0);
});

test("Preferences offers Report bug at the bottom of its sidebar", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("banner").getByRole("button", { name: "Settings" }).click();
  const preferences = page.getByRole("dialog", { name: "Preferences", exact: true });
  await preferences.getByRole("navigation", { name: "Preference panes" }).getByRole("button", { name: "Report bug" }).click();
  const report = page.getByRole("dialog", { name: "Report bug", exact: true });
  await expect(report).toBeVisible();
  await report.getByRole("button", { name: "Close report" }).click();
  await expect(report).toHaveCount(0);
  await expect(preferences).toBeVisible();
});
