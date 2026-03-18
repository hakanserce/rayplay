---
name: gates
description: "Run quality gates and report results"
disable-model-invocation: true
allowed-tools:
  - Bash
  - Read
---

Current working tree:
```
!`git status --short`
```

Run the project's CI/quality gate command as described in CLAUDE.md. Report each gate's result (pass/fail) and the relevant output for any failures.
