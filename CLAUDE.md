# RayPlay

Rust-based low-latency game streaming application.
- **Server (RayHost):** Runs on Windows with Nvidia GPU, captures screen and encodes via NVENC
- **Client (RayView):** Runs on macOS (Android planned later), decodes and renders stream
- Inspired by Sunshine/Moonlight, built from scratch in Rust

## Project Vision

Extreme low latency on both graphics and input. The client should feel native —
exclusive input mode where the mouse is fully trapped in the client window and all
input goes directly to the host. Target latency similar to USB passthrough (VirtualHERE).

## Project Structure

    rayplay/
    ├── Cargo.toml              # Workspace root
    ├── CLAUDE.md               # This file — project context for all agents
    ├── Makefile.toml           # cargo-make build tasks
    ├── crates/
    │   ├── rayplay-core/       # Core streaming logic, shared types
    │   ├── rayplay-network/    # Networking (UDP relay, WebRTC, discovery)
    │   ├── rayplay-video/      # Video capture, encoding (NVENC), decoding
    │   ├── rayplay-input/      # Input capture, relay, exclusive mode
    │   └── rayplay-cli/        # CLI interface (clap) for server and client
    ├── docs/
    │   ├── requirements/       # Functional requirements
    │   ├── uc/                 # Use Case documents (one per feature)
    │   └── adr/                # Architecture Decision Records
    ├── .claude/
    │   ├── agents/             # Subagent definitions (personas)
    │   ├── commands/           # Custom slash commands
    │   ├── settings.json       # Permissions, sandbox, hooks
    │   └── worktrees/          # Auto-created worktree directories (gitignored)
    └── .github/
        └── workflows/          # CI/CD pipelines
            ├── ci.yml          # Quality gates (fmt, clippy, test, coverage)
            └── claude-review.yml  # Automated Claude PR reviews

## Build & Test Commands

    cargo make build              # Build all workspace crates
    cargo make test               # Run all tests
    cargo make lint               # Run clippy --pedantic
    cargo make fmt                # Check formatting
    cargo make coverage           # Generate coverage report (llvm-cov)
    cargo make lint-test-coverage # All three in sequence
    cargo make benchmark          # Run Criterion benchmarks
    cargo make ci                 # Full CI pipeline

## Code Standards

### Clean Code Principles
- DRY — eliminate redundancy through abstraction
- Strong encapsulation with clear component boundaries
- Modularity with single responsibility principle
- KISS — favor simplicity over cleverness
- Self-documenting code — prioritize clarity and readability
- Comments only when absolutely necessary; prefer refactoring for clarity first

### Rust Standards
- Follow official Rust naming conventions strictly
- Write idiomatic Rust — leverage the type system and ownership model
- Apply established Rust design patterns
- Use module_name.rs NOT module_name/mod.rs
- High cohesion, low coupling for module design
- Use cargo workspaces for multi-crate organization
- Create separate crates for reusable components worthy of crates.io publication
- Clear, descriptive module names that reflect purpose

### Essential Crates
- anyhow — flexible error handling for applications
- thiserror — custom error types for libraries
- axum — web framework (discovery, API)
- serde + serde_json — serialization/deserialization
- tokio — async runtime
- tracing — structured logging and diagnostics
- criterion — benchmarking
- clap — CLI argument parsing

### Error Handling
- Use thiserror for library/crate-level errors (crates/rayplay-core, crates/rayplay-network, etc.)
- Use anyhow for application-level errors (crates/rayplay-cli)
- Every error variant should have a descriptive message
- Never use .unwrap() in production code — use ? or handle explicitly
- Use .expect("reason") only in tests or truly unreachable cases

## Quality Gates (MUST pass before every commit)

1. cargo fmt --all — code must be formatted
2. cargo clippy --workspace -- -W clippy::pedantic — zero warnings
3. cargo test --workspace — all tests must pass
4. cargo llvm-cov --workspace --fail-under-lines 99 — coverage ≥99%
5. Benchmark tests for performance-critical code (criterion)

Do NOT skip these. If coverage drops below 99%, add unit tests to fill the gap
before committing.

## Testing Standards
- Unit tests for every public function and struct
- Integration tests for critical workflows
- Benchmark tests for performance-critical paths (video encoding, network relay, input latency)
- Tests follow the same clean code principles as production code
- Use descriptive test names: test_udp_relay_drops_invalid_packets
- Test edge cases: empty inputs, maximum sizes, concurrent access, error paths
- Each test should test ONE thing

## Git & PR Process

### Branching
- main is protected — all changes go through PRs
- Use worktrees for parallel agent work: claude -w feat-<feature-name>
- One UC = one branch = one PR = one GitHub Issue

### Commit Messages
Use conventional commit format:

    feat(crate): short description (UC-XXX)
    fix(network): handle connection timeout gracefully (UC-015)
    refactor(core): extract stream config into separate module
    test(video): add benchmark for NVENC encoding pipeline

### PR Description Template
Every PR must include:
- UC ID and title
- GitHub Issue reference: `Closes #<issue-number>`
- Summary of changes
- How to test
- Quality gate checklist: fmt ✓, clippy ✓, tests ✓, coverage ✓

### PR Review Flow
1. Developer Agent updates GitHub Issue to `status:in-progress`
2. Developer Agent creates PR via gh pr create with `Closes #<issue>` in body
3. GitHub Actions CI runs quality gates automatically
4. Claude Code Action posts automated review
5. Human approves and merges — GitHub auto-closes the linked issue

## Use Case (UC) Documents

All features are tracked as Use Cases in docs/uc/. Each UC follows this template:
- **UC ID:** UC-XXX
- **Title:** Short descriptive title
- **Description:** User story format (As a [user], I want [goal], so that [benefit])
- **Acceptance Criteria:** 3-7 concrete, testable items
- **Technical Approach:** High-level implementation guidance
- **Dependencies:** Other UCs or components required
- **Estimated Complexity:** S / M / L
- **Priority:** P0 (critical path) / P1 (important) / P2 (nice-to-have)

## GitHub Issues — Task Tracking

GitHub Issues is the source of truth for UC and task status. Every UC gets
a corresponding GitHub Issue. Agents update issues as part of their workflow.

### Issue Structure
- Each UC has a GitHub Issue titled: "UC-XXX: <title>"
- Labels for priority: `P0`, `P1`, `P2`
- Labels for status: `status:todo`, `status:in-progress`, `status:in-review`, `status:done`
- Labels for agent: `agent:developer`, `agent:senior-dev`, `agent:qa`
- Assign the issue to the agent working on it (use comments for agent handoffs)

### Agent Workflow with Issues
- **Before starting a UC:** Update the issue label to `status:in-progress`
      gh issue edit <number> --remove-label "status:todo" --add-label "status:in-progress"
- **When creating a PR:** Reference the issue in the PR body with `Closes #<number>`
      gh pr create --title "UC-XXX: title" --body "Closes #<number>\n\n..."
- **When PR is merged:** GitHub auto-closes the issue via the `Closes #` keyword
- **When blocked:** Add a comment to the issue explaining the blocker and add label `blocked`
      gh issue comment <number> --body "Blocked: reason"
      gh issue edit <number> --add-label "blocked"

### Issue Commands Reference
    gh issue list                              # List all open issues
    gh issue list --label "P0"                 # Filter by priority
    gh issue list --label "status:in-progress" # Filter by status
    gh issue create --title "UC-XXX: title" --label "P1,status:todo" --body "..."  # Create
    gh issue edit <number> --add-label "status:in-progress"  # Update status
    gh issue comment <number> --body "Status update: ..."    # Add progress note
    gh issue close <number>                    # Close when done

### Creating Issues for New UCs
When defining a new UC (via Product Manager Agent), always:
1. Write the UC document to docs/uc/UC-XXX.md
2. Create a matching GitHub Issue:
      gh issue create \
        --title "UC-XXX: <title>" \
        --label "<priority>,status:todo" \
        --body "UC document: docs/uc/UC-XXX.md\n\n<acceptance criteria summary>"

## Architecture Decision Records (ADRs)

Significant architecture decisions are documented in docs/adr/. Format:
- **ADR-XXX:** Title
- **Status:** Proposed / Accepted / Deprecated / Superseded
- **Context:** Why this decision is needed
- **Decision:** What was decided
- **Consequences:** Trade-offs and implications

## Agent Collaboration Notes

- Subagent definitions are in .claude/agents/ — do not modify them during sessions
- Each agent works in its own worktree to avoid file conflicts
- Agents communicate through structured documents (UCs, code reviews, test reports)
- Human reviews and approves all architectural decisions
- When a UC is ambiguous, document assumptions and flag for human review
- No code merges without Senior Developer review

## Performance Targets (Guidelines)

- Video encoding latency: target <5ms per frame (NVENC)
- Network round-trip overhead: target <1ms
- Input relay latency: target <2ms end-to-end
- Total glass-to-glass latency goal: <16ms (sub-frame at 60fps)
- All performance-critical paths must have Criterion benchmarks

## What NOT to Do

- Do NOT use .unwrap() in production code
- Do NOT skip quality gates for any reason
- Do NOT modify .claude/agents/ definitions during sessions
- Do NOT commit directly to main — always use a PR
- Do NOT write comments explaining obvious code — refactor for clarity instead
- Do NOT create God objects or mega-functions — keep things modular
- Do NOT use mod.rs — use module_name.rs
- Do NOT add dependencies without justification — keep the dependency tree lean
