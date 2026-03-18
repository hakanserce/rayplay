---
name: uc-create
description: "Create a new Use Case document and matching GitHub issue"
context: fork
agent: product-manager
argument-hint: "<feature description>"
---

## Existing UC Files

```
!`ls docs/uc/ | sort -V | tail -5`
```

## Requirements Documents

```
!`ls docs/requirements/ 2>/dev/null || echo "No requirements directory found."`
```

## Instructions

Read relevant existing UCs and requirements docs to understand project context and avoid overlapping scope. Read the template from `${CLAUDE_SKILL_DIR}/uc-template.md`. Write the UC document to docs/uc/ with the next available ID. Create a matching GitHub Issue with the appropriate priority and status:todo labels, using checkboxes for each acceptance criterion.
