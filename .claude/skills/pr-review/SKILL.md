---
name: pr-review
description: "Review a pull request against project standards"
context: fork
agent: senior-developer
argument-hint: "<PR-number>"
---

## PR Metadata

```
!`gh pr view $ARGUMENTS --json title,body,headRefName,baseRefName,additions,deletions,files`
```

## PR Diff

```
!`gh pr diff $ARGUMENTS`
```

## Existing Reviews

```
!`gh api repos/$(gh repo view --json nameWithOwner --jq '.nameWithOwner')/pulls/$ARGUMENTS/reviews --jq '.[] | {user: .user.login, state: .state, body: .body}' 2>/dev/null || echo "No existing reviews."`
```

## Instructions

This is a **read-only review** — do NOT modify any files. Review the diff against the project's standards in CLAUDE.md. If the PR body references a UC or feature document, read it to verify acceptance criteria. Check: code quality, test coverage, linter compliance, clean code principles. Use the review output format from your agent instructions.
