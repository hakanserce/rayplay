---
name: qa
description: "Validate a UC against its acceptance criteria"
context: fork
agent: qa-tester
argument-hint: "<UC-ID e.g. UC-027>"
---

## UC Document

```
!`UC_FILE=$(find docs/uc/ -maxdepth 1 -iname "*${ARGUMENTS}*" -name "*.md" | head -1); [ -n "$UC_FILE" ] && cat "$UC_FILE" || echo "No UC document matching '$ARGUMENTS' found. Available: $(ls docs/uc/)"`
```

## Instructions

This is a **read-only validation** — do NOT modify source code or tests, only report findings. First, identify which modules implement this UC by searching for related types, functions, and test files using Grep and Glob. Check if there's an open PR with `gh pr list --search "$ARGUMENTS"`. Run the project's quality gates as described in CLAUDE.md. For each acceptance criterion, trace it to specific code and tests. Report using the issue format from your agent instructions.
