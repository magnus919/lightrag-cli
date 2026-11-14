# Contributing

Use Rust stable, `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test --locked`. Add mock HTTP regression tests for request semantics and
security changes. Keep Cargo.lock checked in. Do not put credentials, private
server addresses, documents or query outputs in commits or fixtures.

When refreshing the OpenAPI snapshot, audit all new operations and parameters,
update supplemental source-derived schemas, and run coverage checks. Do not
claim compatibility with versions not inspected. Keep mutation tests hermetic.
Pull requests should explain behavior, verification and remaining limitations.
