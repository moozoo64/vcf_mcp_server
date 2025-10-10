# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

## Development Workflow

### Before Committing

Always run the following commands before committing code:

```bash
cargo fmt
cargo clippy --all-targets --all-features
```

These checks are enforced in CI, so running them locally will catch issues early.

### Formatting

Code must be formatted with `rustfmt`:
```bash
cargo fmt
```

### Linting

Code must pass `clippy` without warnings:
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Fix any clippy warnings before committing.
