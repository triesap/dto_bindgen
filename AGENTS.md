# dto_bindgen - agent instructions

## Product Intent

`dto_bindgen` is a Rust-source-driven DTO generator. Rust source plus supported Serde metadata are canonical. Generated TypeScript and Python files are derived artifacts.

This crate family is a UniFFI companion, not a UniFFI replacement. UniFFI owns native callable bindings. `dto_bindgen` owns passive DTO/type/code generation that UniFFI does not provide in the required standalone form.

## Always Read

- `docs/implementation/BUILDOUT_PLAN.md`
- `docs/handoff/dto_bindgen_handoff/summary.txt`
- `docs/handoff/dto_bindgen_handoff/specs/PRODUCT_SPEC.md`
- `docs/handoff/dto_bindgen_handoff/specs/ARCHITECTURE.md`
- `docs/handoff/dto_bindgen_handoff/specs/API_CONTRACTS.md`
- `docs/handoff/dto_bindgen_handoff/specs/SERDE_SUPPORT_MATRIX.md`
- `docs/handoff/dto_bindgen_handoff/specs/BACKEND_TYPESCRIPT.md`
- `docs/handoff/dto_bindgen_handoff/specs/BACKEND_PYTHON.md`
- `docs/handoff/dto_bindgen_handoff/specs/NUMERIC_POLICY.md`
- `docs/handoff/dto_bindgen_handoff/specs/OUTPUT_AND_FILESYSTEM.md`
- `docs/handoff/dto_bindgen_handoff/specs/DIAGNOSTICS.md`
- `docs/handoff/dto_bindgen_handoff/specs/ACCEPTANCE_CRITERIA.md`

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

## RCLD Execution

Follow `docs/implementation/BUILDOUT_PLAN.md` as the execution order.

The raw handoff sequence at `docs/handoff/dto_bindgen_handoff/implementation/COMMIT_SEQUENCE.md` is a useful reference, but it is not the final execution order. The buildout plan corrects known review findings, including target repo bootstrap, inventory ordering, the large-integer `serde(with)` conflict, and Python parser contract gaps.

## Default Verification

Use repo-local commands once they exist. Until a Rust workspace exists, use docs-only checks for docs-only slices:

```sh
git diff --check
```

After the Rust workspace is scaffolded, the baseline is:

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
- Document deviations from the handoff in the step report and, when durable, in `docs/implementation/BUILDOUT_PLAN.md`.
