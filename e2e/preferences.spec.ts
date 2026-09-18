import { expect, test, type Page } from "@playwright/test";

/**
 * The Preferences window's newer panes and sections: View › Color, the Beat
 * Count Display and waveform click on Display Type, Audio's own sections,
 * the full Keyboard map, and About. Each choice is checked where it lands —
 * the deck, the row, the engine call — not only in the window.
 */

async function open(page: Page, query = "") {
  await page.goto(`/${query}`);
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
}

async function prefs(page: Page) {
  await page.getByRole("banner").getByRole("button", { name: "Settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Preferences" });
  await expect(dialog).toBeVisible();
  return dialog;
}

const player = (page: Page) => page.getByRole("region", { name: "Preview player" });

/** Loads the fourth row — analysed, with the mock's four hot cues and a memory cue. */
async function load(page: Page) {
  await page.locator('[role="gridcell"][data-col="title"]').nth(3).dblclick();
  await expect(player(page).getByRole("button", { name: "Play", exact: true })).toBeEnabled();
}

test("View › Color offers the three palettes and the two hot cue colours, and keeps them", async ({ page }) => {
  await open(page);
  const dialog = await prefs(page);
  await dialog.getByRole("tab", { name: "Color" }).click();

  // Appearance is drawn, and greyed: the light theme is not built.
  await expect(dialog.getByRole("radio", { name: "Dark" })).toBeChecked();
  await expect(dialog.getByRole("radio", { name: "Light" })).toBeDisabled();

  const palette = dialog.getByRole("radiogroup", { name: "Waveform color" });
  await expect(palette.getByRole("radio", { name: "3Band" })).toBeChecked();
  await palette.getByRole("radio", { name: "RGB" }).click();
  await dialog.getByRole("combobox", { name: "HOT CUE color" }).selectOption("cdj");
  await page.keyboard.press("Escape");

  await page.reload();
  await expect(page.getByTestId("browser-title")).toContainText("Tracks)");
  const again = await prefs(page);
  await again.getByRole("tab", { name: "Color" }).click();
  await expect(again.getByRole("radiogroup", { name: "Waveform color" }).getByRole("radio", { name: "RGB" })).toBeChecked();
  await expect(again.getByRole("combobox", { name: "HOT CUE color" })).toHaveValue("cdj");
});

test("HOT CUE color CDJ draws every pad and badge in the one green", async ({ page }) => {
  await open(page);
  await load(page);
  // The mock's fourth track has coloured hot cues: pad A carries its colour.
  const padA = player(page).locator('[aria-label="Hot cues"]').getByRole("button", { name: "Hot cue A", exact: true });
  await expect(padA).toHaveAttribute("style", /--cue-colour/);
  const badgeA = page.getByTestId("player-overview").locator('[title="Hot cue A"]');
  await expect(badgeA).toHaveAttribute("style", /--cue-colour/);

  const dialog = await prefs(page);
  await dialog.getByRole("tab", { name: "Color" }).click();
  await dialog.getByRole("combobox", { name: "HOT CUE color" }).selectOption("cdj");
  await page.keyboard.press("Escape");

  // The colour is gone from the style, so the stylesheet's green shows.
  await expect(padA).not.toHaveAttribute("style", /--cue-colour/);
  await expect(badgeA).not.toHaveAttribute("style", /--cue-colour/);
});

test("the waveform palette redraws the deck and the rows without a reload", async ({ page }) => {
  await open(page);
  await load(page);
  const overview = page.getByTestId("player-overview").locator("canvas").first();
  const pixels = () =>
    overview.evaluate((c: HTMLCanvasElement) => {
      const ctx = c.getContext("2d");
      if (!ctx) return "";
      // A row of pixels through the middle, as a string, to compare.
      return Array.from(ctx.getImageData(0, Math.floor(c.height / 2), c.width, 1).data).join(",");
    });
  await expect.poll(pixels).not.toBe("");
  const threeBand = await pixels();

  const dialog = await prefs(page);
  await dialog.getByRole("tab", { name: "Color" }).click();
  await dialog.getByRole("radiogroup", { name: "Waveform color" }).getByRole("radio", { name: "BLUE" }).click();
  await page.keyboard.press("Escape");
  await expect.poll(pixels).not.toBe(threeBand);
});

test("the Beat Count Display counts bars, or down to the next memory cue", async ({ page }) => {
  await open(page);
  await load(page);
  const bars = page.getByTestId("player-bars");
  await expect(bars).toHaveText(/^\d+\.\dBars$/);

  const dialog = await prefs(page);
  await dialog.getByRole("radio", { name: "Count to the next MEMORY CUE (Beats)" }).click();
  // The mock's memory cue is at 2 % of the track: ahead of a fresh load.
  await expect(bars).toHaveText(/^-\d+Beats$/);
  await dialog.getByRole("radio", { name: "Count to the next MEMORY CUE (Bars)" }).click();
  await expect(bars).toHaveText(/^-\d+\.\dBars$/);
  await dialog.getByRole("radio", { name: "Current Position (Bars)" }).click();
  await expect(bars).toHaveText(/^\d+\.\dBars$/);
});

test("a click on the enlarged waveform plays, and again pauses and sets the cue, unless off", async ({ page }) => {
  await open(page);
  await load(page);
  const detail = page.getByTestId("player-detail");
  const box = await detail.boundingBox();
  if (!box) throw new Error("no detail waveform");
  const play = player(page).getByRole("button", { name: "Play", exact: true });
  const pause = player(page).getByRole("button", { name: "Pause", exact: true });

  // A stopped deck: the click plays it from where the head is.
  await detail.click({ position: { x: box.width * 0.75, y: box.height / 2 } });
  await expect(pause).toBeVisible();
  // A second click, once the head has moved, pauses it and takes the head
  // as the cue point.
  await expect(page.getByTestId("player-overview")).not.toHaveAttribute("aria-valuenow", "0");
  await detail.click({ position: { x: box.width * 0.75, y: box.height / 2 } });
  await expect(play).toBeVisible();
  await expect(page.getByTestId("player-overview")).not.toHaveAttribute("aria-valuenow", "0");

  const dialog = await prefs(page);
  await dialog.getByRole("switch", { name: "Enable" }).click();
  await page.keyboard.press("Escape");
  const before = await page.getByTestId("player-overview").getAttribute("aria-valuenow");
  await detail.click({ position: { x: box.width * 0.25, y: box.height / 2 } });
  await page.waitForTimeout(300);
  await expect(play).toBeVisible();
  await expect(page.getByTestId("player-overview")).toHaveAttribute("aria-valuenow", before ?? "0");
});

test("Audio has its own sections for the rate, the buffer, the metronome and the limiter", async ({ page }) => {
  await open(page);
  const dialog = await prefs(page);
  await dialog.getByRole("tab", { name: "Audio" }).click();
  for (const name of ["Sample Rate", "Buffer size", "Metronome", "Master limiter"]) {
    await expect(dialog.getByRole("heading", { name })).toBeVisible();
  }
  await expect(dialog.getByRole("combobox", { name: "Sample Rate" })).toHaveValue("48000");
  await expect(dialog.getByTestId("buffer-size")).toHaveText("512 samples (10.7 ms)");
  await dialog.getByRole("combobox", { name: "Sample Rate" }).selectOption("96000");
  await expect(dialog.getByTestId("buffer-size")).toHaveText("512 samples (5.3 ms)");
  await expect(dialog.getByRole("radiogroup", { name: "Metronome" }).getByRole("radio", { name: "Click Sound 02" })).toBeChecked();
  await expect(dialog.getByRole("radiogroup", { name: "Metronome volume" }).getByRole("radio", { name: "Large" })).toBeChecked();
  await expect(dialog.getByRole("switch", { name: /^Limiter/ })).toBeVisible();
});

test("the deck's metronome button is live once a track is loaded", async ({ page }) => {
  await open(page);
  // The GRID EDIT row lives behind the pad row's GRID tab.
  await player(page).getByRole("tab", { name: "GRID" }).click();
  const metronome = player(page).getByRole("button", { name: "Metronome" });
  await expect(metronome).toBeDisabled();
  await load(page);
  await expect(metronome).toBeEnabled();
  await metronome.click();
  await expect(metronome).toHaveAttribute("aria-pressed", "true");
  await metronome.click();
  await expect(metronome).toHaveAttribute("aria-pressed", "false");
});

test("Keyboard lists rekordbox's ten groups, with the unbuilt rows greyed", async ({ page }) => {
  await open(page);
  const dialog = await prefs(page);
  await dialog.getByRole("tab", { name: "Keyboard" }).click();
  const groups = dialog.locator('button[aria-expanded]');
  await expect(groups).toHaveText([
    "Browse", "Player A", "Player B", "General", "File", "View", "Track", "Playlist", "Help", "Link Export",
  ]);
  // Every group opens closed; Player A opened: Play/Pause works here, Time
  // Mode is rekordbox's alone.
  await expect(dialog.locator('[class*="keyRow"]')).toHaveCount(0);
  await dialog.getByRole("button", { name: "Player A" }).click();
  const playPause = dialog.locator('[class*="keyRow"]', { hasText: "Play/Pause" }).first();
  await expect(playPause).toContainText("spacebar");
  await expect(playPause).not.toHaveAttribute("data-dim");
  const timeMode = dialog.locator('[class*="keyRow"]', { hasText: "Time Mode" }).first();
  await expect(timeMode).toContainText(/^Time ModeT$/);
  await expect(timeMode).toHaveAttribute("data-dim", "");
  // A built row's key is a button: click it, press a key, and that is the key.
  const loopIn = dialog.locator('[class*="keyRow"]', { hasText: "Loop In" }).first();
  await expect(loopIn).not.toHaveAttribute("data-dim");
  await loopIn.getByRole("button", { name: "Loop In key" }).click();
  await page.keyboard.press("Shift+L");
  await expect(loopIn.getByRole("button", { name: "Loop In key" })).toHaveText("shift + L");
  // Player B's shift + L was Memory Cue 9; the key moved, so that row lost it.
  await dialog.getByRole("button", { name: "Player B" }).click();
  const cue9 = dialog.locator('[class*="keyRow"]', { hasText: "Memory Cue 9" }).nth(1);
  await expect(cue9.getByRole("button", { name: "Memory Cue 9 key" })).toHaveText("+");
  await dialog.getByRole("button", { name: "Reset to the preset" }).click();
  await expect(loopIn.getByRole("button", { name: "Loop In key" })).toHaveText("I");
  // A row rekordbox lists unbound has no key.
  await expect(dialog.locator('[class*="keyRow"]', { hasText: "1/64 Beat Loop" }).first()).toHaveText("1/64 Beat Loop");

  await dialog.getByRole("button", { name: "Link Export" }).click();
  await expect(dialog.getByText("Nothing is bound here in the Export preset.")).toBeVisible();
});

test("About shows the version and who it is by, and leaves updates to Advanced", async ({ page }) => {
  await open(page);
  const dialog = await prefs(page);
  await dialog.getByRole("tab", { name: "About" }).click();
  await expect(dialog.getByRole("heading", { name: "About" })).toBeVisible();
  await expect(dialog.getByText("Made with ❤️ in California")).toBeVisible();
  // The ask sits at the foot of the pane, just above where it was made.
  const support = dialog.getByRole("button", { name: "Support this app for $20" });
  await expect(support).toBeVisible();
  const [supportBox, madeBox, paneBox] = await Promise.all([
    support.boundingBox(),
    dialog.getByText("Made with ❤️ in California").boundingBox(),
    dialog.getByRole("tabpanel").boundingBox(),
  ]);
  expect(supportBox!.y + supportBox!.height).toBeLessThanOrEqual(madeBox!.y);
  expect(madeBox!.y + madeBox!.height).toBeGreaterThan(paneBox!.y + paneBox!.height * 0.8);
  await expect(dialog.getByTestId("about-version")).toHaveText("0.4.0");
  await expect(dialog).toContainText("@TRIODEOfficial");
  // The update check is said once, under Advanced › Others, not in two places.
  await expect(dialog.getByRole("switch", { name: "Automatically check for updates" })).toHaveCount(0);

  await dialog.getByRole("tab", { name: "Advanced" }).click();
  await dialog.getByRole("tab", { name: "Others" }).click();
  const auto = dialog.getByRole("switch", { name: /Keep rbxport up to date/ });
  await expect(auto).toBeChecked();
  await auto.click();
  await expect(auto).not.toBeChecked();
});

test("the player's ≡ opens rekordbox's own menu, and its choices are the View preferences", async ({ page }) => {
  await open(page);
  await load(page);
  await player(page).getByRole("button", { name: "Player menu" }).click();
  const menu = page.getByRole("menu", { name: "Player menu" });
  await expect(menu).toBeVisible();
  await expect(menu.getByRole("menuitem")).toHaveText([
    "Change waveform color", "Analyze Track", "Beat Count Display", "Export Track", "Export Loop As WAV",
    "Active Loop Playback", "Click on the waveform for PLAY and CUE",
  ]);
  for (const greyed of ["Export Track", "Export Loop As WAV", "Active Loop Playback"]) {
    await expect(menu.getByRole("menuitem", { name: greyed })).toBeDisabled();
  }
  // Analysis writes to the library, which the mock holds read-only here.
  await expect(menu.getByRole("menuitem", { name: "Analyze Track" })).toBeDisabled();

  // The submenu ticks what is in force, and choosing changes the preference.
  await menu.getByRole("menuitem", { name: "Change waveform color" }).hover();
  const colours = page.getByRole("menu", { name: "Change waveform color" });
  await expect(colours.getByRole("menuitemradio", { name: "3Band" })).toHaveAttribute("aria-checked", "true");
  await colours.getByRole("menuitemradio", { name: "BLUE" }).click();
  await expect(menu).toBeHidden();
  const dialog = await prefs(page);
  await dialog.getByRole("tab", { name: "Color" }).click();
  await expect(dialog.getByRole("radiogroup", { name: "Waveform color" }).getByRole("radio", { name: "BLUE" })).toBeChecked();
  await page.keyboard.press("Escape");
});

test("a BPM can be typed, dragged on a CDJ's fader, and shifted a semitone", async ({ page }) => {
  await open(page);
  await load(page);
  const deck = player(page);
  const bpm = deck.getByTestId("player-bpm");
  const resting = Number(await bpm.innerText());
  expect(resting).toBeGreaterThan(0);

  // Typed: the deck plays at the BPM asked for.
  await bpm.dblclick();
  const box = deck.getByRole("textbox", { name: "BPM" });
  await box.fill(String(resting + 3));
  await box.press("Enter");
  await expect(bpm).toHaveText(`${(resting + 3).toFixed(2)}`);
  // Escape leaves it alone.
  await bpm.dblclick();
  await box.fill("999");
  await box.press("Escape");
  await expect(bpm).toHaveText(`${(resting + 3).toFixed(2)}`);
  await deck.getByRole("button", { name: "Reset tempo" }).click();
  await expect(bpm).toHaveText(resting.toFixed(2));

  // Dragged: the fader opens under the number and the same drag drives it —
  // down is faster, as a CDJ's is.
  const at = await bpm.boundingBox();
  if (!at) throw new Error("no BPM field");
  await page.mouse.move(at.x + at.width / 2, at.y + at.height / 2);
  await page.mouse.down();
  await page.mouse.move(at.x + at.width / 2, at.y + at.height / 2 + 30, { steps: 6 });
  const fader = deck.getByRole("dialog", { name: "Tempo" });
  await expect(fader).toBeVisible();
  await page.mouse.up();
  await expect(fader.getByRole("radio", { name: "±6" })).toHaveAttribute("aria-checked", "true");
  expect(Number(await bpm.innerText())).toBeGreaterThan(resting);
  await expect(fader.getByTestId("tempo-percent")).toHaveText(/^\+\d\.\d\d%$/);

  // WIDE reaches double speed; the range buttons keep the tempo where it is.
  await fader.getByRole("radio", { name: "WIDE" }).click();
  await expect(fader.getByRole("slider", { name: "Tempo" })).toHaveAttribute("aria-valuemax", "100");
  await fader.getByRole("slider", { name: "Tempo" }).focus();
  const shown = Number(await bpm.innerText());
  await page.keyboard.press("ArrowDown");
  expect(Number(await bpm.innerText())).toBeGreaterThan(shown);

  // A semitone either way, shown beside the fader.
  const shift = fader.getByTestId("key-shift");
  await expect(shift).toHaveText("KEY");
  await fader.getByRole("button", { name: "Key up a semitone" }).click();
  await expect(shift).toHaveText("+1 st");
  await fader.getByRole("button", { name: "Key down a semitone" }).click();
  await fader.getByRole("button", { name: "Key down a semitone" }).click();
  await expect(shift).toHaveText("−1 st");
  // The BPM is untouched by the key.
  expect(Number(await bpm.innerText())).toBeCloseTo(Number(await bpm.innerText()), 2);

  await page.keyboard.press("Escape");
  await expect(fader).toBeHidden();
});
