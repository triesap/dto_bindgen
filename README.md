# dto_bindgen

Rust-source-driven DTO generation for TypeScript and Python.

`dto_bindgen` reads explicit Rust DTO roots, applies supported Serde metadata,
builds a neutral DTO description, and writes deterministic TypeScript and Python
dataclass surfaces. Rust remains the source of truth. Unsupported shapes fail
closed with diagnostics.

## Status

`dto_bindgen` is pre-1.0. The supported contract is intentionally small,
deterministic, and tested.

## Crates

- `dto_bindgen`: facade crate for derive macros, config, diagnostics, and export helpers
- `dto_bindgen_core`: neutral IR, config, validation, writer, and export orchestration
- `dto_bindgen_macros`: `#[derive(Dto)]`
- `dto_bindgen_backend_ts`: TypeScript renderer
- `dto_bindgen_backend_python`: Python dataclass renderer
- `dto_bindgen_cli`: `dto_bindgen` command-line helper
- `dto_bindgen_test`: internal test helper, not published

## Install

```toml
[dependencies]
dto_bindgen = "0.1.0"
```

```sh
cargo install dto_bindgen_cli
```

## Quickstart

Mark exported Rust DTO roots with `#[dto(export)]`. Serde metadata defines the
wire shape.

```rust
use dto_bindgen::Dto;

#[derive(Dto)]
#[dto(export)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UserProfile {
    user_id: String,
    active: bool,
}

#[derive(Dto)]
#[dto(export)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum SdkEvent {
    UserCreated { user: UserProfile, event_id: String },
}
```

Create `dto_bindgen.toml`. A complete example is available at
`examples/dto_bindgen.toml`.

```toml
[export]
out_dir = "generated"

[typescript]
enabled = true
out_dir = "generated/ts"

[python]
enabled = true
out_dir = "generated/python/my_sdk_dto"
package = "my_sdk_dto"

[root_discovery]
mode = "source_manifest"
source_files = ["src/lib.rs"]
root_module_file = "src/generated/dto_roots.rs"
```

Generate and check the Rust root module.

```sh
dto_bindgen roots --config dto_bindgen.toml
dto_bindgen roots-check --config dto_bindgen.toml
```

Run backend rendering from a compiled test, xtask, or small export binary.

```rust
mod generated {
    include!("generated/dto_roots.rs");
}

fn main() -> Result<(), dto_bindgen::export::ExportError> {
    dto_bindgen::export::export_with_roots(
        dto_bindgen::export::ExportOptions::new("dto_bindgen.toml"),
        generated::dto_bindgen_roots(),
    )?;
    Ok(())
}
```

Compiled exports are deliberate. Rust type descriptors must be type checked by
Rust before generated TypeScript or Python is written.

## CLI

```sh
dto_bindgen config --config dto_bindgen.toml
dto_bindgen roots --config dto_bindgen.toml
dto_bindgen roots-check --config dto_bindgen.toml
dto_bindgen diagnostics --json --config dto_bindgen.toml
dto_bindgen clean --config dto_bindgen.toml
dto_bindgen inventory --manifest dto_bindgen.inventory.toml
```

`export` and `check` are reserved for compiled-root workflows. Source-manifest
commands only discover configured files; they do not scan whole crates.

## Supported Shape

Rust and Serde support:

- named structs
- one-field `#[serde(transparent)]` structs
- fieldless external enums
- fieldless string enums with explicit `#[dto(rename = "...")]` variant names
- internally tagged enum struct variants
- adjacently tagged enum unit, struct, and one-field newtype variants
- `rename`, `rename_all`, `rename_all_fields`, and `deny_unknown_fields`
- field `rename`, `skip`, `skip_serializing_if = "Option::is_none"`, and built-in defaults
- primitives, `Option<T>`, `Vec<T>`, arrays, string-keyed maps, and named DTO references

DTO-specific support:

- `#[dto(export)]`
- `#[dto(skip)]`
- `#[dto(as = "string")]`
- `#[dto(as = "string_enum")]`
- `#[dto(rename = "...")]`
- `#[dto(int = "json_string" | "json_number")]`
- `#[dto(bytes = "base64")]`

Large JSON-facing integers require an explicit policy.

```rust
#[derive(dto_bindgen::Dto)]
struct LedgerEntry {
    #[dto(int = "json_string")]
    amount_minor_units: u128,
}
```

Unsupported behavior fails closed. Current non-goals include implicit whole-crate
discovery, `flatten`, `untagged`, custom Serde functions, Pydantic, JSON Schema,
OpenAPI, Swift, Kotlin, and UniFFI bindings.

## Verification

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
npm ci
npm run check:generated-ts
```

Generated Python fixture tests use Python 3.11. Set `DTO_BINDGEN_PYTHON` to
select a specific interpreter.

## Release

Maintainers publish the public crates as one coordinated release set:
`dto_bindgen_core`, `dto_bindgen_macros`, `dto_bindgen_backend_python`,
`dto_bindgen_backend_ts`, `dto_bindgen`, and `dto_bindgen_cli`.

`dto_bindgen_test` is private to the test suite and is not published.

## License

Licensed under either the MIT license or the Apache License, Version 2.0.
