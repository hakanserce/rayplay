---
name: implement-issue
description: "Implement a GitHub issue end-to-end"
context: fork
agent: developer
argument-hint: "<issue-number>"
---

## Issue Details

```
!`gh issue view $ARGUMENTS --json number,title,body,labels,comments`
```

## Referenced UC Document (if any)

```
!`UC_ID=$(gh issue view $ARGUMENTS --json body --jq '.body' | grep -oE 'UC-[0-9]+' | head -1); [ -n "$UC_ID" ] && cat docs/uc/${UC_ID}.md 2>/dev/null || echo "No UC document referenced in issue body."`
```

## Instructions

Update the issue status to in-progress. Identify which modules and packages are relevant by searching the codebase for related types and functions using Grep and Glob. Implement the feature following CLAUDE.md standards. Write tests. Run the project's quality gates as described in CLAUDE.md. Create a PR with "Closes #$ARGUMENTS" in the body.
