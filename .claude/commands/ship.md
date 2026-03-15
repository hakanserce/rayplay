Commit all changes with a conventional commit message referencing $ARGUMENTS.
Push the branch and create a PR targeting main.
Find the GitHub Issue number for $ARGUMENTS with gh issue list.
Include in the PR description:
- UC ID and title (from docs/uc/$ARGUMENTS.md)
- "Closes #<issue-number>" to auto-close the issue on merge
- Summary of changes
- How to test
- Quality gate checklist
