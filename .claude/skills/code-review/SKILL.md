---
name: code-review
description: "Review branch changes against UC acceptance criteria"
context: fork
agent: senior-developer
argument-hint: "<UC-ID e.g. UC-027>"
---

## UC Document

```
!`UC_FILE=$(find docs/uc/ -maxdepth 1 -iname "*${ARGUMENTS}*" -name "*.md" | head -1); [ -n "$UC_FILE" ] && cat "$UC_FILE" || echo "No UC document matching '$ARGUMENTS' found. Available: $(ls docs/uc/)"`
```

## Commit History

```
!`git log --oneline main..HEAD`
```

## Changed Files

```
!`git diff --stat main...HEAD`
```

## Full Diff

```
!`git diff main...HEAD`
```

## Instructions

This is a **read-only review** — do NOT modify any files. Read the changed files directly rather than reviewing the diff line by line. Check if there's an associated PR with `gh pr list --search "$ARGUMENTS"` and read its description. Review against the project's standards in CLAUDE.md. Use the review output format from your agent instructions.
