# Existing PR repair workflow

Own exactly one open PR targeting `dev` and work only in the assigned dedicated
worktree. Preserve the PR number and head branch; repair the existing PR rather
than opening a replacement.

## Inputs

The orchestrator provides the PR metadata, the observed failed check names and
conclusions, GitHub's mergeability state, the head branch, and the worktree.
Treat those observations as the starting evidence, then refresh them before
editing because checks and mergeability can change.

## Workflow

1. Read the PR body, linked issue, review comments, failed job logs, and relevant
   code and tests. Determine whether the failure is caused by the PR, a merge
   conflict, or external infrastructure. Do not change product code merely to
   hide an unrelated flaky or infrastructure failure.
2. Fetch current `origin/dev` and the PR head. If the PR conflicts with `dev`,
   rebase the PR branch onto `origin/dev`, resolve conflicts according to the
   PR's intent and current code, and rerun affected tests. Never merge `dev`
   into the PR branch.
3. Fix PR-caused validation failures. Run the smallest reproducing check first,
   then all checks required by `AGENTS.md` for the resulting change. Keep
   rekordbox write tests isolated with `RB_LITE_TEST=1`.
4. Commit any repair as logical Conventional Commits. If only a rebase was
   needed, do not add a content-free commit.
5. Push the repaired head branch. Because rebasing rewrites a published branch,
   use `--force-with-lease` only after confirming the remote head still matches
   the commit that was fetched. Never force-push `dev` or `main`.
6. Verify that a fresh CI run belongs to the pushed head SHA, then wait for all
   required checks. Do not reuse green results from the superseded commit. If a
   required check fails again, inspect the new logs and continue repairing while
   the cause is actionable and in scope.
7. Before merging, refresh mergeability and required checks once more. Merge
   into `dev` without a merge commit only when the current head is conflict-free
   and required checks pass. Do not merge a draft PR; mark it ready only when
   its scope and prior discussion show it is complete, otherwise leave it open
   and report that decision as required.
8. Add a concise final PR comment with the original failure/conflict, the repair,
   local validation, fresh CI result and head SHA, and resulting `dev` commit.
9. Verify any linked issue closed. If the PR was intended to close it but GitHub
   did not, link the merged PR in an issue comment and close it as completed.
10. Report the PR URL, linked issue (if any), local validation, fresh CI result,
    merge state, issue close state, and any remaining human decision.

## Stop conditions

- If the PR head is not pushable, leave it unchanged and report the permissions
  or fork-owner blocker.
- If the failure is external infrastructure, still flaky after a justified
  retry, or requires secrets/hardware unavailable to the worker, leave the PR
  open and comment with the evidence.
- If resolving a conflict would require guessing between materially different
  product behaviors, leave the worktree intact and report the decision needed.
