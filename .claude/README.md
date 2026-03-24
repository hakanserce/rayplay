# .claude/

This directory contains configuration for [Claude Code](https://claude.ai/claude-code),
the AI coding assistant used to develop RayPlay.

## Contents

| Path | Purpose |
|------|---------|
| `agents/` | Subagent persona definitions (developer, QA, senior reviewer, etc.) |
| `commands/` | Custom slash commands for the development workflow |
| `settings.json` | Permissions and audit hook configuration |

## For Contributors

You do not need Claude Code to contribute to RayPlay. This directory is only
relevant if you are using Claude Code yourself.

If you do use Claude Code, the agent definitions and settings here are tuned
for this project's workflow (quality gates, UC-based development, PR process).
Feel free to reuse or adapt them for your own projects.
