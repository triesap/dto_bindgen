# dto_bindgen

`dto_bindgen` derives a neutral DTO description from Rust source and renders passive TypeScript and Python DTO surfaces from explicit export roots or generated source-manifest roots.

The MVP targets:

- TypeScript `.ts` or `.d.ts` DTO files with type-only imports/exports.
- Python 3.11 stdlib dataclasses with `from_dict`, `to_dict`, `StrEnum`, tagged-enum parser helpers, `DtoParseError`, and `py.typed`.
- Deterministic safe output with a generated manifest, check mode, and manifest-based clean mode.

## Quickstart

Add `#[derive(dto_bindgen::Dto)]` to DTO types, mark package roots with `#[dto(export)]`, and keep Serde metadata as the wire-format source of truth.

```rust,no_run
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
#[serde(tag = "type", content = "payload", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum SdkEvent {
    UserCreated { user: UserProfile, event_id: String },
}
```

Generate the root module from an explicit source manifest:

```toml
[root_discovery]
mode = "source_manifest"
source_files = ["src/lib.rs"]
root_module_file = "src/generated/dto_roots.rs"
```

```sh
dto_bindgen roots --config dto_bindgen.toml
dto_bindgen roots-check --config dto_bindgen.toml
```

Multi-package configs can keep top-level discovery explicit and declare
package-scoped root modules instead:

```toml
[[package]]
key = "core"
rust_package = "radroots-core"
rust_crate = "radroots_core"
npm = "@radroots/core-bindings"
out_dir = "packages/core-bindings/src/generated"

[package.root_discovery]
mode = "source_manifest"
source_files = ["crates/core/src/lib.rs"]
root_module_file = "crates/core/src/generated/dto_roots.rs"
```

Then include the generated module from a test, xtask, or export binary and call the normal compiled export path:

```rust,ignore
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

Compiled exports remain explicit because Rust trait descriptors must be type checked by Rust. The `roots` CLI scans only configured source files and produces/checks the generated root module; backend exports still run through a compiled root harness.

## Configuration

```toml
schema_version = 1

[export]
out_dir = "generated"
emit_docs = false
wire_format = "json"

[numeric]
large_int_policy = "require_explicit"

[typescript]
enabled = true
out_dir = "generated/ts"
wire_contract = "json_exchange"
layout = "bundle"
bundle_file = "types.ts"
emit = "ts"
module_resolution = "bundler"
import_extension = "none"
type_only_imports = true
strict_null_checks_required = true
style = "dto_bindgen"

[python]
enabled = true
out_dir = "generated/python/my_sdk_dto"
package = "my_sdk_dto"
mode = "dataclass"
python_version = "3.11"
frozen = true
slots = true
kw_only = true
emit_from_dict = true
emit_to_dict = true
emit_py_typed = true
unknown_fields = "ignore"

[root_discovery]
mode = "source_manifest"
source_files = ["src/lib.rs"]
root_module_file = "src/generated/dto_roots.rs"
```

Backend generated paths must stay under `[export].out_dir`. The manifest is written inside that output root.

## CLI

The CLI is a helper for config, source-manifest root generation/checking, inventory reports, diagnostics plumbing, and manifest cleanup.

```sh
cargo run -p dto_bindgen_cli -- --help
cargo run -p dto_bindgen_cli -- config --config dto_bindgen.toml
cargo run -p dto_bindgen_cli -- roots --config dto_bindgen.toml
cargo run -p dto_bindgen_cli -- roots-check --config dto_bindgen.toml
cargo run -p dto_bindgen_cli -- diagnostics --json --config dto_bindgen.toml
cargo run -p dto_bindgen_cli -- clean --config dto_bindgen.toml
cargo run -p dto_bindgen_cli -- inventory --manifest dto_bindgen.inventory.toml \
  --json-out docs/implementation/reports/sdk_inventory_pilot.json \
  --markdown-out docs/implementation/reports/sdk_inventory_pilot.md
```

Use a test, xtask, or small export binary that calls `dto_bindgen::export::export_with_roots` with the generated root module for backend rendering. Inventory and root discovery use explicit source inputs from manifests; the CLI does not scan every Rust root automatically, and `export`/`check` are reserved for compiled-root backend workflows.
`diagnostics --json` reports each generated root module, configured source
files, generated type paths, and whether the root module is missing, current, or
stale.

## Supported MVP Shape

Rust/Serde support:

- named structs
- one-field `#[serde(transparent)]` structs
- fieldless external enums
- fieldless string enums with explicit `#[dto(rename = "...")]` variant names
- internally tagged enum struct variants
- adjacently tagged enum unit, struct, and one-field newtype variants
- `rename`, `rename_all`, `rename_all_fields`, and `deny_unknown_fields`
- field `rename`, `skip`, `skip_serializing_if = "Option::is_none"` for `Option<T>`, and built-in `default` for `Option<T>`, `String`, `bool`, numeric types, `Vec<T>`, and string-keyed maps
- primitives, `Option<T>`, `Vec<T>`, arrays, string-keyed maps, and named DTO references

DTO-specific support:

- `#[dto(export)]`
- `#[dto(skip)]`
- `#[dto(as = "string")]`
- `#[dto(as = "string_enum")]`
- `#[dto(rename = "...")]` on string-enum variants
- `#[dto(int = "json_string" | "json_number")]`
- `#[dto(bytes = "base64")]` for `Vec<u8>` fields

Unsupported behavior fails closed with diagnostics. MVP non-goals include implicit whole-crate discovery, `flatten`, `untagged`, container `default`, custom `serde(default = "...")` functions, custom `skip_serializing_if` predicates, arbitrary custom serializers, Pydantic, JSON Schema/OpenAPI, Swift/Kotlin backends, and UniFFI integration.

## Numeric Policy

Large Rust integers do not silently map to JSON-facing target types. With the default policy, fields such as `u64`, `i128`, and `u128` require an explicit `#[dto(int = "...")]` override.

```rust
#[derive(dto_bindgen::Dto)]
struct LedgerEntry {
    #[dto(int = "json_string")]
    amount_minor_units: u128,
}
```

The Python backend parses `json_string` integers through `int(...)` and serializes them with `str(...)`. The TypeScript backend renders them as `string`.

## Verification

Local and CI verification:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Generated Python fixture tests use Python 3.11. Set `DTO_BINDGEN_PYTHON=/path/to/python3.11` to override the interpreter used by tests.

## Examples

See `crates/dto_bindgen/examples/basic_export.rs` for a compile-checked export harness covering structs, fieldless enums, tagged enums, and numeric policy.
