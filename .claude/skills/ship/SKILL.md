---
name: ship
description: "Commit, push, and create a PR for the current branch"
disable-model-invocation: true
allowed-tools:
  - Bash
  - Read
argument-hint: "<UC-ID or feature-name>"
---

## Current State

Branch: `!`git branch --show-current``

```
!`git status --short`
```

```
!`git diff --stat`
```

## UC Document

```
!`UC_FILE=$(find docs/uc/ -maxdepth 1 -iname "*${ARGUMENTS}*" -name "*.md" | head -1); [ -n "$UC_FILE" ] && cat "$UC_FILE" || echo "No UC document matching '$ARGUMENTS' found."`
```

## GitHub Issue

```
!`gh issue list --search "$ARGUMENTS" --json number,title --jq '.[0]'`
```

## Instructions

Commit all changes with a conventional commit message referencing $ARGUMENTS. Push the branch and create a PR targeting the main branch. Use the PR template from `${CLAUDE_SKILL_DIR}/pr-template.md`. Include "Closes #<issue-number>" in the PR body. If quality gates haven't been run yet, warn the user but don't run them — that's a separate step.
