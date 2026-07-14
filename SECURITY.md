# Security Policy

`dto_bindgen` generates passive DTO source files from Rust source metadata. It
does not execute generated TypeScript or Python.

Report security issues privately to the maintainer listed in `Cargo.toml`.
Please include affected versions, reproduction steps, and any generated output
that demonstrates the issue.

Unsupported Serde shapes and path-unsafe generated output must fail closed.
