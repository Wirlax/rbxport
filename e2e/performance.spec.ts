import { expect, test } from "@playwright/test";
import budgets from "../perf-budgets.json" with { type: "json" };

const port = (Number(process.env.E2E_PORT) || 1420) + 1;
const productionUrl = `http://127.0.0.1:${port}`;

test("the production browser paints its first rows inside the budget", async ({ page }, testInfo) => {
  await page.goto(productionUrl);
  await expect.poll(() => page.evaluate(() =>
    performance.getEntriesByName("startup:first-rows-painted")[0]?.startTime ?? 0,
  )).toBeGreaterThan(0);
  const elapsed = await page.evaluate(() =>
    performance.getEntriesByName("startup:first-rows-painted")[0]?.startTime ?? Number.POSITIVE_INFINITY,
  );
  const browser = testInfo.project.name === "webkit" ? "webkit" : "chromium";
  expect(elapsed).toBeLessThan(budgets.gates.browser.firstRowsMs[browser]);
});
