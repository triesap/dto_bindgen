# dto_bindgen - agent instructions

## Product Intent

`dto_bindgen` is a Rust-source-driven DTO generator. Rust source plus supported Serde metadata are canonical. Generated TypeScript and Python files are derived artifacts.

This crate family is a UniFFI companion, not a UniFFI replacement. UniFFI owns native callable bindings. `dto_bindgen` owns passive DTO/type/code generation that UniFFI does not provide in the required standalone form.

## Always Read

- `README.md`
- `CHANGELOG.md`
- `CONTRIBUTING.md`
- `SECURITY.md`

## Public Repo Boundary

- Write README, docs, examples, tests, comments, and package metadata as standalone public repository content.
- Keep private operator context and external repository layout out of public repo content.

## Mandatory Boundaries

- `dto_bindgen_core` must not depend on backend crates.
- `dto_bindgen_core` must not depend on `dto_bindgen_macros`.
- `dto_bindgen_core` must not depend on UniFFI.
- Proc macros must not write files.
- Backends must consume neutral IR.
- Unsupported Serde features must fail closed.
- Large integer TypeScript JSON behavior must be explicit.
- Generated file writing must be deterministic, path-contained, manifest-based, and all-or-nothing.
- Prefer `#![forbid(unsafe_code)]` unless a crate has a documented exception.

## Default Verification

For docs-only changes, use:

```sh
git diff --check
```

For Rust changes, use:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
```

Add clippy, all-features checks, TypeScript strict checks, and Python import/runtime checks as relevant to the active slice.

## Commit Hygiene

- Keep commits small and green.
- Do not mix generated-output churn with unrelated implementation work.
- Do not claim a slice complete when required checks are red.
- Keep durable project guidance in tracked repository files that make sense for public contributors.
