# Agent usability eval

This eval measures whether a coding agent can use the public SDK from the
crate documentation alone, without reading SDK source files.

Give the agent the ten prompts in `tasks.json` and ask it to write one Rust
module per task, named `<task-id>.rs`. Each module holds public functions that
take a `&Client` or an API key. The agent may read the crate's `rustdoc`
output and the README, but not `src`, `openapi`, tests, generator code, or the
reference answers.

Score an answer directory with:

```bash
cargo xtask generate
cargo xtask score-agent-eval path/to/answers
```

The scorer reports two independent results per task:

- `compile`: the answer compiles against the crate with `cargo check`.
- `semantic`: lightweight required and forbidden markers show that it used the
  intended resource, terminal operation, and behavior.

The semantic checks are conservative heuristics, not a substitute for human
review. Review incorrect answers for invented methods, raw HTTP paths,
misunderstood references, eager pagination, and branching on error messages.

`evals/reference` is a checked-in 10/10 baseline that proves the tasks and
scorer still match the current crate. It is not an agent score.
