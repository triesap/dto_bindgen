# dto_bindgen
Rust-source-driven DTO bindgen for TypeScript and Python dataclass surfaces.

## Status

This repository is in MVP buildout. The implementation authority is:

- `AGENTS.md`
- `docs/implementation/BUILDOUT_PLAN.md`
- `docs/handoff/dto_bindgen_handoff/summary.txt`
- `docs/handoff/dto_bindgen_handoff/specs/`

## Product Boundary

`dto_bindgen` derives a neutral DTO IR from Rust structs/enums and supported Serde metadata, then renders passive TypeScript and Python DTO surfaces.

It is not a UniFFI replacement, not a JSON-schema-first generator, and not a thin `ts-rs` fork.
