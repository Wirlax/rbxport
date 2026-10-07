import { describe, expect, it } from "vitest";

import { formatBugReportAttachment } from "./bugReport";

describe("formatBugReportAttachment", () => {
  it("includes the current email and every supplied preference before the diagnostic log", () => {
    const preferences = {
      analysis: { auto: false, mode: "rbxport" },
      advanced: { protectLibrary: true, relocateFolders: ["/Music"] },
    };

    const attachment = formatBugReportAttachment(
      "System information\nrbxport 1.0.0\n\nApplication log\nready\n",
      "  dj@example.com  ",
      preferences,
    );

    const details = attachment.match(/^Report details\n([\s\S]+?)\n\nSystem information/)?.[1];
    expect(details).toBeDefined();
    expect(JSON.parse(details ?? "{}")).toEqual({ email: "dj@example.com", preferences });
    expect(attachment).toMatch(/Report details[\s\S]+System information[\s\S]+Application log/);
  });

  it("records that an optional email was not supplied", () => {
    expect(formatBugReportAttachment("log", "   ", {})).toContain('"email": null');
  });
});
