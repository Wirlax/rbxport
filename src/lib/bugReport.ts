// A dedicated Worker; rbxport.com itself is static Cloudflare Pages content.
const REPORT_URL = "https://report.rbxport.com/";

export interface BugReportSubmission {
  email: string;
  description: string;
  attachment: string;
  turnstileToken: string;
}

export interface BugReportReceipt {
  key: string;
  attachmentAdded: boolean;
}

function isReceipt(value: unknown): value is BugReportReceipt {
  return Boolean(value) && typeof value === "object" &&
    typeof (value as { key?: unknown }).key === "string" &&
    typeof (value as { attachmentAdded?: unknown }).attachmentAdded === "boolean";
}

export async function submitBugReport(submission: BugReportSubmission): Promise<BugReportReceipt> {
  const response = await fetch(REPORT_URL, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(submission),
  });
  const body: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const message = body && typeof body === "object" && "error" in body && typeof body.error === "string"
      ? body.error : "The report could not be submitted. Please try again later.";
    throw new Error(message);
  }
  if (!isReceipt(body)) {
    throw new Error("The report service returned an invalid response.");
  }
  return body;
}
