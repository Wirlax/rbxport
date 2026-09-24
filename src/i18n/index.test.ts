import { describe, expect, it } from "vitest";

import uiStrings from "./ui.json";
import de from "../../public/locales/de.json";
import ja from "../../public/locales/ja.json";
import { LANGUAGE_CHOICES, translate } from ".";
import { LOCALES } from "@/lib/preferences";

describe("localization", () => {
  it("offers every rekordbox language in its native name", () => {
    expect(LANGUAGE_CHOICES.map(({ value }) => value)).toEqual(LOCALES);
    expect(LANGUAGE_CHOICES.at(-1)).toEqual({ value: "ja", label: "日本語" });
  });

  it("uses the canonical catalogs and falls back to English", () => {
    expect(translate("Collection", de)).toBe("Sammlung");
    expect(translate("Collection", ja)).not.toBe("Collection");
    expect(translate("rbxport-only wording", de)).toBe("rbxport-only wording");
  });

  it("has a translation entry for every detected UI string in every locale", async () => {
    for (const locale of LOCALES.filter((value) => value !== "en")) {
      const catalog = (await import(`../../public/locales/${locale}.json`)).default;
      expect(uiStrings.filter((text) => catalog[text] === undefined), locale).toEqual([]);
    }
  });
});
