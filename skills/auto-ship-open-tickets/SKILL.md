---
name: auto-ship-open-tickets
description: >-
  Autonomously ship every actionable open GitHub issue and repair open PRs
  with failed validation or merge conflicts in chrisle/rbxport. Use when Chris
  asks to ship the open tickets, clear or drain the RBX backlog, run the ticket
  queue, or when this skill's scheduler invokes it.
---

# Auto-ship open rbxport issues

This is an orchestrator. Gather unhealthy open PRs and actionable open issues,
create isolated worktrees, dispatch one worker per queue item, wait for fresh
CI, merge successful PRs into `dev`, and report. Do not edit product code in
the orchestrator checkout.

## Project facts

- Repository and issue tracker: `chrisle/rbxport` on GitHub.
- Base branch: `dev`.
- Worktree parent: `../rbxport-worktrees/`.

## 1. Gather unhealthy PRs

Inspect every open PR targeting `dev`, including draft PRs:

```bash
gh pr list --repo chrisle/rbxport --state open --base dev --limit 100 \
  --json number,title,url,isDraft,headRefName,headRepositoryOwner,isCrossRepository,mergeable,mergeStateStatus,statusCheckRollup
```

Queue a PR for repair when either condition is true:

- A completed validation check has a failing check-run `conclusion` such as
  `FAILURE`, `TIMED_OUT`, `CANCELLED`, `ACTION_REQUIRED`, `STARTUP_FAILURE`, or
  `STALE`, or a legacy status context has `state: FAILURE` or `state: ERROR`.
- GitHub reports `mergeable: CONFLICTING` or a dirty merge state. If mergeability
  is `UNKNOWN`, refresh the PR before classifying it; do not treat an unknown or
  merely pending state as a conflict.

Ignore expected non-validation jobs such as an intentionally skipped release
or deployment job. A PR with only queued or in-progress checks is not failed;
leave it out of the repair queue unless it later reaches a failing conclusion.
For each queued PR, record the failed check names and conflict state so the
worker has concrete failure evidence.

## 2. Gather actionable issues

```bash
gh issue list --repo chrisle/rbxport --state open --limit 100 \
  --json number,title,body,labels,assignees,createdAt,url
```

Exclude issues labeled `wontfix`, `invalid`, or `duplicate`. Stay idempotent:
skip an issue if a matching `issue-<NUMBER>` branch or open PR already exists.
When that open PR is unhealthy, its PR repair item already owns the work; do not
also create an issue worker. If neither unhealthy PRs nor actionable issues
exist, report that and stop.

## 3. Create worktrees

Create worktrees sequentially because concurrent `git worktree add` calls race:

```bash
git fetch origin
mkdir -p ../rbxport-worktrees
git worktree add ../rbxport-worktrees/issue-<NUMBER> \
  -b <type>/issue-<NUMBER>-<slug> origin/dev
```

Use `fix` for issues labeled `bug`; otherwise use `feat` unless another
Conventional Commit type is clearly better.

For a PR repair, reuse its existing dedicated worktree when present. Otherwise,
fetch its head branch and create `../rbxport-worktrees/pr-<PR_NUMBER>` checked
out on a local branch that tracks the PR head. Confirm the branch is pushable
before dispatching. If a fork or permissions prevent updates, report the exact
blocker instead of changing a different branch.

## 4. Dispatch workers

Fan out in batches of at most three, with at most one worker per PR or issue.

- For a PR repair, give the worker the PR metadata, failed-check evidence,
  conflict state, worktree, head branch, and `dev` as the target. Tell it to read
  and follow [references/pr-repair-workflow.md](references/pr-repair-workflow.md).
- For a new issue, give the worker the issue number, title, labels, full body,
  worktree, branch, and `dev` as the target. Tell it to read and follow
  [references/ticket-workflow.md](references/ticket-workflow.md).

Every worker must work only inside its assigned worktree and report the item,
PR, local validation, fresh CI, merge/close state, and a one-line result.

## 5. Clean up and report

After all workers finish, remove successfully merged worktrees sequentially.
Leave unsuccessful worktrees intact for inspection. Finish with a compact table:
kind, issue (if any), PR, local validation, CI, merged, issue closed (if any),
and note.

## Safety

- Merge only after the repaired head commit passes required checks and GitHub
  reports no merge conflict. Leave still-failing or timed-out PRs open with a
  comment explaining the latest result.
- Never touch `main`; changes target `dev`.
- Stop and report genuinely ambiguous issues rather than guessing.
