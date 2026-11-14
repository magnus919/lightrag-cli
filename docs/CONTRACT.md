# Delivery contract

Build a rich Rust CLI for humans and agents covering every operation and parameter
in the deployed LightRAG OpenAPI document, with MIT-licensed public source.

Acceptance: inspect actual deployed schema; maintain a generated endpoint/parameter
coverage matrix; discoverable help, structured JSON, stdin/file payloads, upload and
stream support; credential-safe lazy authentication; no-network dry runs; bounded
errors and no automatic replay of mutations; tests for auth, request construction,
error/stream handling, and complete schema coverage. Verify actual installation
with safe read-only requests. Never publish local credentials, URLs, documents or
queries. Live destructive operations are not authorized by this implementation
request and must only be mocked/dry-run tested.

The owner explicitly chose Rust over the skill's usual Python default, and a
complete API client over a deliberately small wrapper. Public publication on
magnus919 GitHub and local Mac use are authorized. Server changes are out of scope.
