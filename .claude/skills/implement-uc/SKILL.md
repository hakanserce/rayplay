---
name: implement-uc
description: "Implement a Use Case end-to-end"
context: fork
agent: developer
argument-hint: "<UC-ID e.g. UC-027>"
---

## UC Document

```
!`UC_FILE=$(find docs/uc/ -maxdepth 1 -iname "*${ARGUMENTS}*" -name "*.md" | head -1); [ -n "$UC_FILE" ] && cat "$UC_FILE" || echo "No UC document matching '$ARGUMENTS' found. Available: $(ls docs/uc/)"`
```

## GitHub Issue

```
!`gh issue list --search "$ARGUMENTS" --json number,title,labels --jq '.[0]'`
```

## Issue Details

```
!`ISSUE_NUM=$(gh issue list --search "$ARGUMENTS" --json number --jq '.[0].number'); [ -n "$ISSUE_NUM" ] && gh issue view "$ISSUE_NUM" --json number,body,comments 2>/dev/null || echo "No matching issue found."`
```

## Instructions

Update the matching GitHub issue to in-progress. Read the UC's Technical Approach section and search the codebase with Grep and Glob for related existing types and modules. Implement in the appropriate package. Write tests. Run the project's quality gates as described in CLAUDE.md. Create a PR with "Closes #<issue-number>" in the body.
