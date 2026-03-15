# Daily Prompts & Slash Commands Reference

Common prompts for daily agentic development with Claude Code on the RayPlay project. Each prompt can also be saved as a slash command in `.claude/commands/`.

---

## 🌅 Morning — Kick Off Agents

### Get a status report

<aside>
📊

Slash command: `.claude/commands/status.md`

</aside>

```
Use the project-manager agent to give me a status report.
Check GitHub Issues status with gh issue list, open PRs with gh pr list,
recent commits on main with git log --oneline -10, and test coverage.
Group issues by status label (in-progress, blocked, todo) and cross-reference
with open PRs.
```

### Spin up a developer agent on a UC

<aside>
🤖

Slash command: `.claude/commands/implement.md` — invoke with `/implement UC-012`

</aside>

```
Implement $ARGUMENTS from docs/uc/$ARGUMENTS.md. Follow all non-functional requirements
in CLAUDE.md. First, find the GitHub Issue for $ARGUMENTS with gh issue list and update
its status to in-progress. Run all quality gates before committing. When creating the PR,
include "Closes #<issue-number>" in the PR body.
```

### Review overnight PRs

```
# List open PRs, then review each one via CLI
gh pr list
gh pr diff <number> | claude "Review this diff against our CLAUDE.md standards.
Check: idiomatic Rust, test coverage, clippy compliance, clean code."
```

---

## 🔨 During the Day — Development

### Implement a feature

<aside>
💻

Slash command: `.claude/commands/implement.md` — invoke with `/implement UC-025`

</aside>

```
Implement $ARGUMENTS from docs/uc/$ARGUMENTS.md. Read the acceptance criteria
and implement in the appropriate crate. Write unit tests, run clippy --pedantic,
and make sure coverage stays above 99%.
Find the GitHub Issue for $ARGUMENTS and update its label to status:in-progress.
When done, create a PR with "Closes #<issue-number>" in the body.
```

### Fix a failing test or bug

```
cargo test is failing in crates/rayplay-video. Diagnose the issue and fix it.
```

### Explore the codebase

```
Explain the architecture of crates/rayplay-network. What are the main modules,
public APIs, and how does data flow through them?
```

### Add a new crate to the workspace

```
Create a new crate called "rayplay-input" in crates/rayplay-input. Set up the Cargo.toml
with our standard dependencies (anyhow, thiserror, tokio, tracing, serde).
Add it to the workspace. Create a basic lib.rs with module scaffolding.
```

---

## 🔍 Code Review & QA

### Ask for a code review

<aside>
🔍

Slash command: `.claude/commands/review.md` — invoke with `/review UC-012`

</aside>

```
Use the senior-developer agent to review the changes on this branch
against $ARGUMENTS's acceptance criteria in docs/uc/$ARGUMENTS.md.
```

### Run QA validation

<aside>
🧪

Slash command: `.claude/commands/qa.md` — invoke with `/qa UC-012`

</aside>

```
Use the qa-tester agent to validate $ARGUMENTS. Read the acceptance criteria
from docs/uc/$ARGUMENTS.md. Run all tests, check coverage, and verify each
acceptance criterion.
```

### Run all quality gates

```
Run the full quality gate check: cargo fmt, clippy --pedantic, all tests,
and coverage. Report any failures.
```

---

## 📝 Architecture & Planning

### Define a new Use Case

<aside>
📋

Slash command: `.claude/commands/uc-create.md` — invoke with `/uc-create UC-030 UDP hole-punching for NAT traversal`

</aside>

```
Use the product-manager agent to write a UC for $ARGUMENTS.
Save the UC document to docs/uc/ and then create a matching GitHub Issue
with the appropriate priority, status:todo, and size labels.
Include the acceptance criteria as checkboxes in the issue body.
```

### Make an architecture decision

```
Use the senior-developer agent to evaluate whether we should use WebRTC
or raw UDP for the video stream transport. Write an ADR to docs/adr/.
```

### Research a technical topic

```
Research the best approach for low-latency screen capture on Windows
using the Desktop Duplication API. Summarize trade-offs and recommend
an approach for our NVENC pipeline.
```

---

## 🔀 Git & PRs

### Commit and create a PR

<aside>
🚀

Slash command: `.claude/commands/ship.md` — invoke with `/ship UC-012`

</aside>

```
Commit all changes with a conventional commit message referencing $ARGUMENTS.
Push the branch and create a PR targeting main.
Find the GitHub Issue number for $ARGUMENTS with gh issue list.
Include in the PR description:
- UC ID and title (from docs/uc/$ARGUMENTS.md)
- "Closes #<issue-number>" to auto-close the issue on merge
- Summary of changes
- How to test
- Quality gate checklist
```

### Review a specific PR (via CLI)

<aside>
🔍

Slash command: `.claude/commands/pr-review.md` — invoke with `/pr-review 14`

</aside>

```bash
# Full review against project standards
gh pr diff $ARGUMENTS | claude "Review this diff against our CLAUDE.md standards.
Check: idiomatic Rust, test coverage, clippy compliance,
clean code principles, and UC acceptance criteria."

# Quick summary
gh pr view $ARGUMENTS --json body,title,files | claude "Summarize this PR and flag concerns."
```

### Resolve merge conflicts

```
There are merge conflicts with main. Pull the latest main, resolve
the conflicts preserving both sets of changes, and run all tests.
```

---

## 🧹 Refactoring & Quality

### Refactor a module

```
The stream_handler in crates/rayplay-network/src/relay.rs is getting too large.
Refactor it into smaller, well-named functions following clean code principles.
```

### Improve test coverage

<aside>
📈

Slash command: `.claude/commands/coverage.md` — invoke with `/coverage crates/rayplay-network`

</aside>

```
Run cargo llvm-cov on $ARGUMENTS. Find the uncovered paths and add unit tests
to bring coverage back above 99%.
```

### Run benchmarks

```
Run the full benchmark suite with cargo bench. Compare results to
the previous run and flag any regressions above 5%.
```

---

## 🌙 Evening — Wrap Up

### End-of-day summary

<aside>
📊

Slash command: `.claude/commands/eod.md`

</aside>

```
Use the project-manager agent to summarize today's progress.
Use gh issue list to report: issues closed today, issues still in-progress,
issues blocked, and new issues created. Also list PRs merged, coverage trends,
and blockers. Compare issues closed vs. daily target (3-5).
```

### Queue overnight work

```
Run the full benchmark suite and generate a report comparing
this week's benchmarks to last week's.
```

### Clean up worktrees

```
List all git worktrees. Remove any that have been merged to main.
```

---

## ⚙️ Saving as Slash Commands

To turn any prompt into a reusable slash command, save it as a markdown file in `.claude/commands/`:

```bash
mkdir -p .claude/commands
```

### Making commands parametric with `$ARGUMENTS`

Use the `$ARGUMENTS` placeholder anywhere in your command file. Everything the user types after the command name gets substituted in.

Example — `/implement UC-012` replaces every `$ARGUMENTS` with `UC-012`.

<aside>
💡

`$ARGUMENTS` is the **only** supported placeholder. For multi-value inputs, pass them space-separated and write the prompt so Claude can parse them naturally.

</aside>

Example — `.claude/commands/status.md` (no arguments needed):

```
Use the project-manager agent to give me a status report.
Check GitHub Issues with gh issue list, open PRs with gh pr list,
recent commits on main with git log --oneline -10, and test coverage
with cargo llvm-cov --workspace.
Format as a table with sections for: Issues by Status, PRs, Recent Commits,
Coverage, Blockers.
```

Example — `.claude/commands/implement.md` (parametric):

```
Implement $ARGUMENTS from docs/uc/$ARGUMENTS.md. Follow all non-functional requirements
in CLAUDE.md. Run all quality gates before committing.
```

Then invoke them in any session with:

```
/status
/implement UC-025
```

### Suggested Slash Commands to Create

| Command | File | Example | Uses `$ARGUMENTS`? |
| --- | --- | --- | --- |
| `/status` | `status.md` | `/status` | No |
| `/implement` | `implement.md` | `/implement UC-025` | ✅ UC ID |
| `/review` | `review.md` | `/review UC-012` | ✅ UC ID |
| `/qa` | `qa.md` | `/qa UC-012` | ✅ UC ID |
| `/ship` | `ship.md` | `/ship UC-012` | ✅ UC ID |
| `/uc-create` | `uc-create.md` | `/uc-create UC-030 UDP hole-punching` | ✅ UC ID + description |
| `/pr-review` | `pr-review.md` | `/pr-review 14` | ✅ PR number |
| `/coverage` | `coverage.md` | `/coverage crates/rayplay-network` | ✅ Crate path |
| `/eod` | `eod.md` | `/eod` | No |
| `/gates` | `gates.md` | `/gates` | No |