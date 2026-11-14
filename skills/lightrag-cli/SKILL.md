---
name: lightrag-cli
description: >-
  Query and manage a LightRAG knowledge collection through the Rust LightRAG CLI.
  Use for source-backed questions, document status, graph inspection, authorized
  uploads and collection maintenance. Not for generic web search or server deployment.
license: MIT
compatibility: Requires lightrag-cli on PATH and LIGHTRAG_URL; authenticated servers require an API key, bearer token, or private credential file.
metadata:
  tags: lightrag, knowledge, retrieval, rust, cli
---
# LightRAG CLI

Use `lightrag-cli --help` to discover operations; each method/path has its own
named command. `operations --json` gives the complete field-level contract.

## Retrieve evidence first

```sh
lightrag-cli post-query-data --query 'Your question' --json
lightrag-cli post-query-stream --query 'Your question'
lightrag-cli post-documents-paginated --page 1 --page-size 20 --json
```

Use retrieval sources/chunks as evidence, not a generated answer alone. Returned
file paths are not necessarily public URLs. Preserve attribution and uncertainty.
Use `--all` on document pagination or source-conflict listing to traverse safely.
Check `complete:true` and exit status; bounded/failed runs return partial JSON and
exit 1. Single-page mode remains the default. Changing collections are not a
transactional snapshot, and conflict candidate samples remain samples. Do not treat a sampled conflict
candidate list as complete.

## Authenticate and diagnose

```sh
lightrag-cli get-auth-verify --json
lightrag-cli get-documents-status-counts --json
```

Health alone does not establish authentication. Credentials are read privately
from LIGHTRAG_API_KEY, LIGHTRAG_TOKEN, or the configured username:password file.
Never print the file, token, or include them in a command argument. Distinguish
network/sandbox denial from server failure. Do not retry mutations blindly.

## Authorized changes only

```sh
lightrag-cli post-documents-upload --file paper.pdf --dry-run
lightrag-cli post-documents-upload --file paper.pdf --yes
```

Offline `--dry-run` performs no networking and needs no credentials. `--yes`
permits mutation but does not grant user permission. Source-conflict repair's
separate `--server-dry-run` is an API field. Require explicit authorization before
running destructive graph/document/reset operations. Accepted async work is not
finished work: inspect its status separately.

## Output

`--json` emits JSON only on stdout; errors are on stderr. Streaming commands emit
NDJSON including references/progress/timing and exit nonzero on an error record.
Use `--body @file.json` for nested inputs, or `--body -` for stdin. Use `--output`
for binary assets. Consult [README](../../README.md) for compatibility limits.
