import { defineConfig, devices } from "@playwright/test";

/**
 * Runs the real components against the mock backend in a plain browser.
 * `webkit` approximates WKWebView (macOS shell), `chromium` approximates
 * WebView2 (Windows shell).
 */
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  // Locally a failure is a failure. On a shared runner WebKit dies of its own
  // accord — `page.goto: WebKit encountered an internal error`, a different
  // test each run — and a retry is the difference between reporting that and
  // reporting the app. Two, and `trace: "on-first-retry"` below finally has
  // something to attach: a test that fails all three times is real, and comes
  // with its trace.
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? "list" : [["list"]],
  use: {
    baseURL: "http://localhost:1420",
    trace: "on-first-retry",
    // Matches the reference captures so screenshots compare like for like.
    viewport: { width: 1800, height: 1130 },
    deviceScaleFactor: 2,
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "webkit", use: { ...devices["Desktop Safari"] } },
  ],
  webServer: [
    {
      command: "pnpm exec vite --port 1420 --strictPort",
      url: "http://localhost:1420",
      reuseExistingServer: true,
      timeout: 60_000,
    },
    {
      // The built assets, which is what the shell actually ships. The dev
      // server injects inline scripts of its own, so a policy tested against
      // it would be testing Vite rather than the app.
      command: "pnpm exec vite preview --port 1421 --strictPort --outDir dist",
      url: "http://localhost:1421",
      reuseExistingServer: true,
      timeout: 60_000,
    },
  ],
});
