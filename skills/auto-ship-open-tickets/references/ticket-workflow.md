# Per-issue workflow

Own exactly one GitHub issue and work only in the assigned
`../rbxport-worktrees/issue-<NUMBER>` worktree.

## Conventions

- Use logical Conventional Commits with no generated-by or co-author trailers.
- Reference `#<NUMBER>` in the PR body.
- Preserve the repository's safety rules and linear `dev` history.

## Workflow

1. Add `status:in-progress`:
   `gh issue edit <NUMBER> -R chrisle/rbxport --add-label status:in-progress`.
2. Read the relevant code, tests, and architecture before editing. Implement the
   complete issue without writing to a real rekordbox library.
3. Run the smallest relevant checks first, then all checks required by
   `AGENTS.md` for the change.
4. Commit logical units and push the branch.
5. Open a PR against `dev`. Use a Conventional Commit title and explain the
   change, evidence, and validation. Include `Closes #<NUMBER>`.
6. Wait for required CI with `gh pr checks --watch --fail-fast`. On failure or
   timeout, comment with the evidence and leave the PR open.
7. Rebase onto current `dev` if needed and merge without creating a merge commit.
8. Add a concise final report to the PR with the issue, change, checks, and
   resulting `dev` commit.
9. Verify the issue closed. If GitHub did not close it from the PR, link the PR
   in a comment and run
   `gh issue close <NUMBER> -R chrisle/rbxport --reason completed`.
10. Report the issue number, PR URL, CI result, merge state, close state, and any
    remaining human decision to the orchestrator.
