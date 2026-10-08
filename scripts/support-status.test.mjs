import assert from "node:assert/strict";
import { test } from "node:test";
import {
  aggregateStatuses, canonicalDuplicateNumber, extractFixPrNumbers,
  extractThreadIds, mergeForumTags, resolveStatus, RBX_FORUM_TAGS,
} from "./support-status.mjs";

test("Discord source links and explicit markers yield distinct thread IDs", () => {
  assert.deepEqual(extractThreadIds(
    "## Discord source\nhttps://discord.com/channels/723/111\n<!-- rbx-support-thread: 222 -->\n[Discord source](https://discord.com/channels/723/999/333)\nRelated: https://discord.com/channels/723/444",
    "723", ["999"],
  ), ["111", "333", "222"]);
  assert.deepEqual(extractThreadIds(
    "[Discord source](https://discord.com/channels/723/444/555)",
    "723", ["999"],
  ), ["444"]);
});

test("only reviewed fix PRs in the queue comment establish code state", () => {
  assert.deepEqual(extractFixPrNumbers([
    { body: "Related #157", user: { login: "reporter" } },
    { body: "<!-- rbx-queue-status -->\n- **PR:** https://github.com/chrisle/rbxport/pull/157\n- **Fix PR:** https://github.com/chrisle/rbxport/pull/157", user: { login: "chrisle" } },
  ]), [157]);
  assert.deepEqual(extractFixPrNumbers([{ body: "<!-- rbx-fix-pr: 157 -->", user: { login: "reporter" } }]), []);
  assert.deepEqual(extractFixPrNumbers([{ body: "<!-- rbx-fix-pr: 157 -->", user: { login: "chrisle" } }]), [157]);
});

test("assignment plus active work, dev merge, and main ancestry set the three stages", () => {
  const issue = { labels: [{ name: "status:in-progress" }], assignees: [{ login: "chrisle" }] };
  assert.equal(resolveStatus(issue, []), "in-progress");
  assert.equal(resolveStatus(issue, [{ onDev: true, onMain: false, onPublicRelease: false }]), "under-review");
  assert.equal(resolveStatus(issue, [{ onDev: true, onMain: true, onPublicRelease: false }]), "done");
  assert.equal(resolveStatus({ ...issue, labels: [{ name: "duplicate" }] }, [{ onDev: true, onMain: true }]), null);
  assert.equal(resolveStatus({ ...issue, assignees: [] }, []), null);
  assert.equal(resolveStatus({ ...issue, labels: [{ name: "status:under-review" }] }, []), "under-review");
});

test("a duplicate follows its canonical ticket without promoting the intake issue", () => {
  assert.equal(canonicalDuplicateNumber(
    { labels: [{ name: "duplicate" }] },
    [{ body: "Duplicate of #42", user: { login: "chrisle" } }],
  ), 42);
  assert.equal(aggregateStatuses(["done", "under-review"]), "under-review");
  assert.equal(aggregateStatuses(["done", null]), null);
});

test("tag updates preserve non-workflow forum tags", () => {
  assert.deepEqual(mergeForumTags(["unrelated", RBX_FORUM_TAGS["in-progress"]], "done"), [
    "unrelated", RBX_FORUM_TAGS.done,
  ]);
});
