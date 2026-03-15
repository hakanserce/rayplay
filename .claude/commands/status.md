Use the project-manager agent to give me a status report.
Check GitHub Issues status with gh issue list, open PRs with gh pr list,
recent commits on main with git log --oneline -10, and test coverage
with cargo llvm-cov --workspace.
Group issues by status label (in-progress, blocked, todo) and cross-reference
with open PRs.
Format as a table with sections for: Issues by Status, PRs, Recent Commits,
Coverage, Blockers.
