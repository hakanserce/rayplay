# Development Guide

Everything you need to develop, test, and contribute to RayPlay.

## Prerequisites

### Required Tools

| Tool              | Install                              | Purpose                    |
| ----------------- | ------------------------------------ | -------------------------- |
| Rust (stable)     | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` | Compiler toolchain |
| cargo-make        | `cargo install cargo-make`           | Build task orchestration   |
| cargo-nextest     | `cargo install cargo-nextest`        | Fast test runner           |
| cargo-llvm-cov    | `cargo install cargo-llvm-cov`       | Code coverage              |
| llvm-tools        | `rustup component add llvm-tools-preview` | Coverage instrumentation |
| clippy            | `rustup component add clippy`        | Linter                     |
| rustfmt           | `rustup component add rustfmt`       | Formatter                  |
| GitHub CLI        | `brew install gh` (macOS)            | PR management              |

### Optional Tools

| Tool              | Install                              | Purpose                    |
| ----------------- | ------------------------------------ | -------------------------- |
| cargo-instruments | `cargo install cargo-instruments`    | Profiling (macOS only)     |
| Docker            | https://docker.com                   | Sandboxed agent execution  |

## Build Commands

All commands use cargo-make. Run from the workspace root:

    cargo make build              # Debug build, all crates
    cargo make build-release      # Release build, all crates
    cargo make test               # Run all tests (nextest)
    cargo make test-cargo         # Standard cargo test (includes doc tests)
    cargo make lint               # clippy --pedantic, zero warnings
    cargo make fmt                # Check formatting
    cargo make fmt-fix            # Auto-fix formatting
    cargo make coverage           # Terminal coverage summary
    cargo make coverage-html      # HTML report, opens in browser
    cargo make coverage-ci        # Fails if coverage < 99%
    cargo make benchmark          # Criterion benchmarks
    cargo make instruments-time   # Time Profiler on rayplay-cli (macOS)
    cargo make instruments-alloc  # Allocations Profiler (macOS)
    cargo make lint-test-coverage # lint + test + coverage
    cargo make ci                 # Full CI: fmt + lint + test + coverage
    cargo make check              # Fast type-check
    cargo make doc                # Build and open rustdoc
    cargo make clean              # Delete build artifacts

## Code Standards

### Clean Code Principles

- DRY — eliminate redundancy through abstraction
- Strong encapsulation with clear component boundaries
- Single responsibility principle for modules and functions
- KISS — favor simplicity over cleverness
- Self-documenting code — clarity and readability first
- Comments only when absolutely necessary; prefer refactoring for clarity

### Rust Conventions

- Follow official Rust naming conventions strictly
- Write idiomatic Rust — leverage the type system and ownership model
- Use `module_name.rs` NOT `module_name/mod.rs`
- High cohesion, low coupling for module design
- No `.unwrap()` in production code — use `?` or handle explicitly
- `.expect("reason")` only in tests or truly unreachable cases

### Error Handling

- Library crates (rayplay-core, rayplay-network, rayplay-video, rayplay-input):
  Use `thiserror` for custom error types
- Application crate (rayplay-cli): Use `anyhow` for flexible error handling
- Every error variant must have a descriptive, actionable message

### Module Structure

- Use `module_name.rs` files, not `module_name/mod.rs` directories
- Keep modules focused — one clear responsibility per module
- Extract into a new crate when a module grows large and is independently reusable

## Quality Gates

**Every change must pass ALL of these before commit or PR:**

1. `cargo fmt --all -- --check` — code is formatted
2. `cargo clippy --workspace -- -W clippy::pedantic` — zero warnings
3. `cargo nextest run --workspace` — all tests pass
4. `cargo llvm-cov --workspace --fail-under-lines 99` — coverage >= 99%

Quick check: `cargo make ci` runs all four in sequence.

If coverage drops below 99%, add unit tests to fill the gap before proceeding.

## Testing Standards

- Unit tests for every public function and struct
- Integration tests for critical workflows
- Benchmark tests for performance-critical paths (Criterion)
- Descriptive test names: `test_udp_relay_drops_invalid_packets`
- Each test should test ONE thing
- Test edge cases: empty inputs, maximum sizes, concurrent access, error paths
- Tests follow the same clean code principles as production code

## Git Workflow

### Branching

- `main` is protected — all changes go through PRs
- Feature branches: `feat/uc-xxx-short-description`
- Bug fixes: `fix/uc-xxx-short-description`
- Refactoring: `refactor/short-description`
- One UC = one branch = one PR

### Commit Messages

Use conventional commit format:

    feat(rayplay-network): implement UDP stream relay (UC-012)
    fix(rayplay-video): correct NVENC frame timing (UC-015)
    refactor(rayplay-core): extract config into separate module
    test(rayplay-input): add edge case tests for exclusive mode

### PR Process

1. Run `cargo make ci` locally — all gates must pass
2. Push branch and create PR:

       git push origin feat/uc-012-udp-relay
       gh pr create --title "UC-012: Implement UDP stream relay" \
         --body "## Summary\n...\n\n## Quality Gates\n- [x] fmt\n- [x] clippy\n- [x] tests\n- [x] coverage" \
         --base main

3. GitHub Actions CI runs automatically
4. Claude Code Action posts automated review
5. Human reviews and approves
6. Merge to main

### PR Description Template

Every PR must include:

    ## UC Reference
    UC-XXX: Title

    ## Summary
    What changed and why.

    ## How to Test
    Steps to verify the changes.

    ## Quality Gates
    - [x] cargo fmt
    - [x] clippy --pedantic (zero warnings)
    - [x] all tests pass
    - [x] coverage >= 99%

## Use Case Documents

All features are defined as Use Cases in `docs/uc/`. Template:

    # UC-XXX: Title

    **Priority:** P0 | P1 | P2
    **Complexity:** S | M | L
    **Dependencies:** UC-YYY, UC-ZZZ

    ## Description
    As a [user], I want [goal], so that [benefit].

    ## Acceptance Criteria
    1. Given [context], when [action], then [result]
    2. ...

    ## Technical Approach
    High-level implementation guidance.

    ## Test Plan
    Key test scenarios.

## Architecture Decision Records

Significant decisions are documented in `docs/adr/`:

    # ADR-XXX: Title

    **Status:** Proposed | Accepted | Deprecated | Superseded
    **Date:** YYYY-MM-DD

    ## Context
    Why this decision is needed.

    ## Decision
    What was decided.

    ## Consequences
    Trade-offs and implications.

## Agentic Development

This project uses Claude Code with multiple specialized subagents:

| Agent            | Role                                                       |
| ---------------- | ---------------------------------------------------------- |
| Project Manager  | Track deliverables, velocity, blockers, costs              |
| Product Manager  | Define requirements, write UCs, manage acceptance criteria |
| Developer        | Implement UCs, write tests, follow non-functional reqs     |
| Senior Developer | Code review, architecture decisions, refactoring tasks     |
| QA Tester        | Validate acceptance criteria, coverage, quality gates      |
| DevOps           | CI/CD, build tasks, deployment, infrastructure             |

Agent definitions live in `.claude/agents/`. See CLAUDE.md for full project
context that all agents read at session start.

### Worktrees

Agents work in parallel using git worktrees:

    claude -w feat-uc-012      # Creates worktree, branch, and session
    claude -w fix-uc-015       # Another agent in parallel

Worktrees are auto-cleaned when the agent session closes.

## Performance Profiling

### Benchmarks (all platforms)

    cargo make benchmark       # Run all Criterion benchmarks
    cargo bench -- <filter>    # Run specific benchmarks

### Instruments (macOS only)

    cargo make instruments-time    # Time Profiler on rayplay-cli
    cargo make instruments-alloc   # Allocations Profiler on rayplay-cli

### Key performance targets

| Metric                 | Target         |
| ---------------------- | -------------- |
| NVENC encoding         | < 5ms/frame    |
| Network round-trip     | < 1ms          |
| Input relay e2e        | < 2ms          |
| Glass-to-glass total   | < 16ms (60fps) |

All performance-critical paths must have Criterion benchmarks.
