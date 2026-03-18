---
name: eod
description: "End-of-day summary of today's progress"
context: fork
agent: project-manager
disable-model-invocation: true
---

## Today's Commits

```
!`git log --oneline --since="midnight" main`
```

## Recently Closed Issues

```
!`gh issue list --state closed --json number,title,closedAt --limit 20`
```

## In-Progress Issues

```
!`gh issue list --label "status:in-progress" --json number,title,labels`
```

## Blocked Issues

```
!`gh issue list --label "blocked" --json number,title,labels`
```

## Instructions

Summarize today's progress. Report: issues closed today, issues still in-progress, issues blocked, and new issues created. Identify PRs merged today with `gh pr list --state merged --json number,title,mergedAt`. Check latest CI status via `gh run list --limit 1`. Compare issues closed vs. daily target.
