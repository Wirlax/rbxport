import { expect, test } from "@playwright/test";

test("bug reports preview the attachment and honor the usage-statistics default", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("rbl.preferences", JSON.stringify({ advanced: { usageStats: false } })));
  await page.goto("/");
  await page.getByRole("button", { name: "Report bug", exact: true }).click();
  const report = page.getByRole("dialog", { name: "Report bug", exact: true });
  await expect(report.getByRole("checkbox")).not.toBeChecked();
  await expect(report.getByRole("button", { name: "Save report ZIP…" })).toBeDisabled();
  await report.getByLabel("What happened?").fill("Playback stopped after loading a track.");
  await expect(report.getByRole("button", { name: "Save report ZIP…" })).toBeEnabled();
  await report.getByRole("checkbox").check();
  await report.getByRole("button", { name: "Show exactly what will be attached" }).click();
  await expect(report.getByLabel("Report attachment")).toContainText("System information");
  await report.getByRole("checkbox").uncheck();
  await expect(report.getByLabel("Report attachment")).toHaveCount(0);
  await report.getByRole("button", { name: "Close", exact: true }).click();
  await expect(report).toHaveCount(0);
});
