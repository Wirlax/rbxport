/**
 * The Content-Security-Policy the shell ships, tested against the built app.
 *
 * A wrong CSP is a blank window, and one was written and reverted once
 * already because there was no way to check it: the packaged Tauri window
 * cannot be screenshotted on this machine. This checks the half that actually
 * caused that — whether the policy permits what the app's own code does —
 * without needing the packaged app at all.
 *
 * What it does not cover: the `rbl:` and `ipc:` sources, which only exist
 * inside the Tauri webview, and Tauri's own injection of the policy into the
 * page. Those are listed in the policy from the scheme the shell registers,
 * and are not exercised here.
 */
import { readFileSync } from "node:fs";

import { expect, test } from "@playwright/test";

/** The policy the shell ships, read from the config so the two cannot drift. */
function shippedPolicy(): string {
  const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")) as {
    app?: { security?: { csp?: string | null } };
  };
  const csp = config.app?.security?.csp;
  if (typeof csp !== "string" || csp.length === 0) {
    throw new Error("tauri.conf.json has no csp to test");
  }
  return csp;
}

test("the built app runs under the policy the shell ships", async ({ page }) => {
  const violations: string[] = [];
  const policy = shippedPolicy();

  // Injected into the document itself: a policy delivered any later than the
  // first byte of the HTML does not govern the scripts in it.
  await page.route("http://localhost:1421/", async (route) => {
    const response = await route.fetch();
    const html = await response.text();
    await route.fulfill({
      response,
      body: html.replace(
        "<head>",
        `<head><meta http-equiv="Content-Security-Policy" content="${policy}">`,
      ),
    });
  });

  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (event) => {
      const record = `${event.violatedDirective} blocked ${event.blockedURI}`;
      ((window as unknown as { __csp: string[] }).__csp ??= []).push(record);
    });
  });

  await page.goto("http://localhost:1421/");

  // The app has to actually run: a policy that blocks the bundle produces a
  // blank page and no violations worth the name.
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  await expect(page.locator('[role="row"]').first()).toBeVisible();

  // Style attributes have to have applied, not merely not complained. Rows are
  // positioned by an inline transform from the virtualizer, so if the policy
  // blocked style attributes every row would sit at the same offset and the
  // list would look like one row — visible, and completely wrong.
  const offsets = await page.locator('[role="row"]').evaluateAll((rows) =>
    rows.slice(0, 4).map((row) => getComputedStyle(row).transform),
  );
  expect(new Set(offsets).size, `rows share a transform: ${offsets.join(" | ")}`).toBeGreaterThan(1);

  // And the parts that draw to canvas.
  await page.locator('[role="gridcell"][data-col="title"]').nth(3).click();
  await expect(page.getByTestId("player-detail").locator("canvas")).toBeVisible();

  violations.push(
    ...(await page.evaluate(() => (window as unknown as { __csp?: string[] }).__csp ?? [])),
  );
  expect(violations, `CSP violations: ${violations.join(", ")}`).toEqual([]);
});
