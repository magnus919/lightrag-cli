# LightRAG CLI

A Rust command-line client for the complete LightRAG REST API, for people and AI
agents. MIT licensed. Unofficial; not affiliated with the LightRAG project.

## Install

```sh
cargo install --git https://github.com/magnus919/lightrag-cli --locked
```

For a local checkout: `cargo install --path . --locked`.

## Quick start

```sh
export LIGHTRAG_URL=https://lightrag.example.com
# Set LIGHTRAG_API_KEY or LIGHTRAG_TOKEN in your environment, or create a private
# credential file containing username:password (split on the first colon).
chmod 600 ~/.config/lightrag
lightrag-cli get-auth-verify --json
lightrag-cli post-query-data --query 'What does this collection say?' --json
lightrag-cli post-query-stream --query 'Summarize the evidence'
lightrag-cli post-documents-paginated --page 1 --page-size 20 --json
lightrag-cli post-documents-upload --file ./paper.pdf --dry-run
lightrag-cli post-documents-upload --file ./paper.pdf --yes
```

No account, cloud service, telemetry or hosted middleman. Requests go directly to
your configured server. Credentials never go in the repository. Prefer HTTPS;
HTTP sends credentials and data unencrypted and is appropriate only on trusted
networks. Login happens per invocation; tokens are held in memory, not cached.

## Discover the entire API

`lightrag-cli --help` lists **45 named operations**. Command names mirror the HTTP
method and path: `POST /graph/entity/create` becomes
`post-graph-entity-create`. Path placeholders become named flags. Every documented
query/body field has a flag, replacing underscores with hyphens. Help shows types,
required/optional status, enums, server defaults and available bounds. Nested objects
and arrays take JSON. Each command has `--help` and examples.

```sh
lightrag-cli operations --json
lightrag-cli post-query --help
lightrag-cli post-query --body @query.json --json
cat query.json | lightrag-cli post-query --body - --json
lightrag-cli get-ui-customization-assets-asset-hash-asset-id \
  --asset-hash HASH --asset-id ID --output asset.bin
```

`--body` accepts literal JSON, `@file`, or stdin `-`; explicit field flags override
body fields. Omitted flags preserve server defaults. Nullable booleans accept
`null`, `true`, and `false`. Arbitrary nested JSON is preserved, not flattened.
Use `--server-dry-run` for source-conflict repair's *server* preview flag; global
`--dry-run` is an entirely offline request preview.

## Output and safety

- `--json` emits machine-readable compact JSON for JSON responses. Otherwise JSON
  is pretty-printed. No status banners on stdout. Errors go to stderr.
- Streams validate NDJSON objects and flush each record. Malformed records or
  embedded server errors exit nonzero while preserving received bytes. Progress, references,
  token output and response timing are retained. Embedded stream errors produce
  a nonzero exit after preserving partial output.
- `--output PATH` saves exact response bytes, useful for binary assets/streams.
- `--dry-run` never contacts the server or reads credentials. Required inputs are
  still validated. Login previews redact the credential payload.
- Mutations require `--yes`. Preview first. This is an explicit automation guard,
  not a substitute for your own permission checks.
- Requests are never automatically retried. Repeating writes can duplicate or
  destroy data; the server, not the CLI, controls idempotency and async jobs.
- HTTP errors report status/method/path without leaking potentially private error
  bodies. The CLI does not follow redirects (including the root UI redirect).
- Exit 0 means HTTP transport succeeded, not that an asynchronous job completed.
  Inspect business statuses such as busy, fail, not_allowed or partial_success.
  Usage errors exit 2; request/configuration/server-stream errors exit 1.

Auth precedence: `LIGHTRAG_API_KEY`, then `LIGHTRAG_TOKEN`, then
`--credentials-file` / `LIGHTRAG_CREDENTIALS_FILE` / `~/.config/lightrag`.
The file uses form login and Bearer JWT. Help, schema, operations and dry runs
work without credentials. Explicit `post-login` suppresses tokens on stdout;
normal commands log in automatically.

## Coverage and compatibility

The bundled schema was inspected against API **0344**. See
[API contract audit](docs/API-CONTRACT-AUDIT.md) for every operation and caveat.
The deployed Ollama handlers omit their request bodies from OpenAPI; supplemental
schemas extracted from the actual Pydantic models restore flags for both. Options
and image fields are accepted by those models but not used by those handlers.

`operations --json` is the executable coverage manifest: commands, methods, paths,
parameters, request schema and write guards. `schema` prints the original OpenAPI.
Single-page commands remain the default. Add `--all` to document pagination or
source-conflict listing for automatic traversal. Filters, sort and page size are
preserved; a missing document page size becomes 200. Source conflicts follow the
opaque cursor even after short/empty pages. Authentication happens once per run.

```sh
lightrag-cli post-documents-paginated --all --json
lightrag-cli get-documents-source-conflicts --all --max-pages 20 --max-items 1000 --json
```

Automatic results use `{items, pages_fetched, complete, duplicates_skipped, error,
resume, snapshot_consistent}`. Identical IDs are deduplicated. Default limits are
100 pages and 10,000 retained items; limits are explicit and bounded. A repeated
cursor/page, malformed page, HTTP failure or exhausted safety limit produces
partial JSON with `complete:false` **and exit 1**, never an exhaustive success.
`--output` preserves that same envelope. Dry-run shows the plan without logging in.

`resume` gives a page/cursor and within-page item offset when truncated; it is a
hint for further inspection, not an automatically replayable snapshot token.
Collections can change during traversal, so `snapshot_consistent` is always false:
deduplication prevents repeats but cannot guarantee no omissions in a changing
collection. Conflict candidate samples remain samples even when every conflict
page has been fetched.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

Tests use local mock HTTP servers, not a real collection. Never run mutation tests
against production. Live verification is restricted to protected read-only
endpoints. See [CONTRIBUTING](CONTRIBUTING.md), [contract](docs/CONTRACT.md) and
[agent skill](skills/lightrag-cli/SKILL.md).

Validation boundary: flags validate scalar types, enum values and scalar/array
length bounds, plus required top-level fields. Full JSON bodies, nested model
constraints and business rules are validated by LightRAG itself. A dry run is a
request preview, not proof the server will accept the request. Null-valued string
flags use `null` as null; use `--body` to send the literal string `"null"`.
