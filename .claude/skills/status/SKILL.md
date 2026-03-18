---
name: status
description: "Project status report with issues, PRs, and CI health"
context: fork
agent: project-manager
---

## Open Issues

```
!`gh issue list --limit 50 --json number,title,labels,state`
```

## Open PRs

```
!`gh pr list --json number,title,headRefName,state,statusCheckRollup`
```

## Recent Commits

```
!`git log --oneline -10 main`
```

## Instructions

Group issues by status label (in-progress, blocked, todo) and cross-reference with open PRs. Check the latest CI run results via `gh run list --limit 3`. Format as a table with sections: Issues by Status, PRs, Recent Commits, CI Status, Blockers.
