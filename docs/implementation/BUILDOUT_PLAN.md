# dto_bindgen Buildout Plan

This public repo plan is the durable execution reference for `dto_bindgen`.
Historical handoff docs may exist in the private parent workspace, but this file
is the repo-local authority for ongoing work.

## Current Priorities

1. Preserve the Rust-source-driven DTO contract and fail-closed Serde subset.
2. Keep generated TypeScript and Python deterministic and path-contained.
3. Keep `dto_bindgen_core` independent from backend and proc-macro crates.
4. Publish only through the parent workspace local Act pipeline.

## Release Readiness

- `dto_bindgen_test` remains private with `publish = false`.
- Release documentation lives in `CHANGELOG.md`, `CONTRIBUTING.md`, and
  `SECURITY.md`.
- The initial public release group is versioned together.
- Direct ad hoc `cargo publish` is not an accepted release path.
