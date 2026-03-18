---
name: coverage
description: "Run coverage, find gaps, and add tests to meet the threshold"
allowed-tools:
  - Bash
  - Read
  - Write
  - Edit
  - Glob
  - Grep
---

Run the project's coverage command as described in CLAUDE.md. If coverage is below the threshold, use Grep and Glob to find the uncovered source files. Read the coverage report to identify specific uncovered lines, then add tests to fill the gaps. Re-run coverage to confirm the threshold is met.
