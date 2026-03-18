Implement $ARGUMENTS from docs/uc/$ARGUMENTS.md. Follow all non-functional requirements
in CLAUDE.md. First, find the GitHub Issue for $ARGUMENTS with gh issue list and update
its status to in-progress. Read the acceptance criteria and implement in the appropriate
crate. Write unit tests. Run `cargo make ci` (fmt + clippy + tests + coverage gate) to
verify all quality gates pass before committing. When creating the PR,
include "Closes #<issue-number>" in the PR body.
