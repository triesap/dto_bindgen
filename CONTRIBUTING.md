# Contributing

Thanks for helping improve `dto_bindgen`.

## Development

Run the Rust validation lane before sending changes:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Generated TypeScript fixtures use the package scripts:

```sh
npm ci
npm run check:generated-ts
```

Generated Python fixture tests use Python 3.11. Set `DTO_BINDGEN_PYTHON` when
the desired interpreter is not first on `PATH`.

## Release

Crates.io publishing is operated from the private parent `triesap/dev`
workspace through local Act workflows and `triesap-publish`. Do not publish with
manual `cargo publish` commands from this repository.
