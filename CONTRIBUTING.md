# Contributing

Openhandle SDK behavior is defined by the pinned OpenAPI contract and the
language-neutral SDK contract in the Openhandle API repository.

## Development

Install Rust 1.85 or newer with `rustfmt` and `clippy`, then run:

```bash
cargo xtask generate
cargo test
```

Run `cargo xtask generate` after you change the pinned OpenAPI document or the
generator in `xtask`. Commit the generated files with their source changes.

Quality checks:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo xtask generate --check
cargo xtask score-agent-eval evals/reference
cargo package --locked
```

## Commits

Use Conventional Commits for commit subjects, such as `feat: add an endpoint`
or `fix(runtime): honor request cancellation`.

Every release uses the version of the API contract. API contract changes
arrive as automated pull requests. Runtime, documentation, and example fixes
are welcome directly, and ship with the next contract version.
