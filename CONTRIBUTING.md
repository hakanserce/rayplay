# Contributing to RayPlay

Thank you for your interest in contributing! This document explains how to get involved.

## License

By contributing, you agree that your contributions are subject to the
[PolyForm Noncommercial License 1.0.0](LICENSE). Commercial use of this
software or any derivatives requires separate written permission from the author.

## Ways to Contribute

- **Bug reports** — open a GitHub Issue with reproduction steps
- **Bug fixes** — open a PR referencing the issue
- **Feature discussions** — open a GitHub Issue before writing code
- **Documentation improvements** — PRs welcome

## Development Setup

```sh
git clone https://github.com/hakanserce/rayplay.git
cd rayplay
git config core.hooksPath .githooks   # activates pre-commit formatting check
cargo make build
```

### Prerequisites

- Rust stable toolchain: https://rustup.rs
- `cargo install cargo-make`
- `cargo install cargo-nextest`
- `cargo install cargo-llvm-cov`

## Quality Gates

Every PR must pass the full CI pipeline before it can be merged:

```sh
cargo make ci   # fmt + clippy --pedantic + tests + coverage ≥99%
```

Run this locally before pushing. Do not skip it.

## Pull Request Guidelines

- One logical change per PR — keep diffs focused
- Reference a GitHub Issue in the PR body (`Closes #NNN` or `Relates to #NNN`)
- Write or update tests for every change — coverage must stay ≥99%
- Follow existing code style (idiomatic Rust, no `.unwrap()` in production code)
- PR titles should use conventional commit format: `feat(crate): description`

## Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(network): add connection retry with backoff
fix(video): prevent frame drop on resolution change
docs(readme): clarify pairing instructions
```

## Code Standards

- No `.unwrap()` in production code — use `?` or handle explicitly
- Use `thiserror` for library errors, `anyhow` for application errors
- No `mod.rs` — use `module_name.rs`
- clippy --pedantic must pass with zero warnings
- Comments only where the logic is non-obvious; prefer clear naming

## Reporting Security Vulnerabilities

See [SECURITY.md](SECURITY.md).

## Code of Conduct

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
