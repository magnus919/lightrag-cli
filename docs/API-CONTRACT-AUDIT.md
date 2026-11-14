# API contract audit

Source: live deployment OpenAPI API version 0344, retrieved 2026-11-14. 45 paths / 45 operations. Supplemental Ollama request schemas were extracted from the deployed router models because OpenAPI omits those request bodies. No production data mutations were performed.

| Method | Path | Path/query parameters | Body schema / fields |
|---|---|---|---|
| POST | `/documents/scan` | — | — |
| GET | `/documents/scan/status/{track_id}` | path:track_id (required) | — |
| GET | `/documents/source_conflicts` | query:limit, query:cursor | — |
| POST | `/documents/source_conflicts/repair` | — | application/json: SourceConflictRepairRequest (canonical_source_key*, primary_doc_id*, expected_candidate_count, expected_candidate_fingerprint, dry_run) |
| POST | `/documents/upload` | — | multipart/form-data: Body_upload_to_input_dir_documents_upload_post (file*) |
| POST | `/documents/text` | — | application/json: InsertTextRequest (text*, file_source, chunking) |
| POST | `/documents/texts` | — | application/json: InsertTextsRequest (texts*, file_sources, chunking) |
| DELETE | `/documents` | — | — |
| GET | `/documents/pipeline_status` | — | — |
| DELETE | `/documents/delete_document` | — | application/json: DeleteDocRequest (doc_ids*, delete_file, delete_llm_cache) |
| POST | `/documents/clear_cache` | — | application/json: ClearCacheRequest () |
| GET | `/documents/track_status/{track_id}` | path:track_id (required) | — |
| POST | `/documents/paginated` | — | application/json: DocumentsRequest (status_filter, status_filters, page, page_size, sort_field, sort_direction) |
| GET | `/documents/status_counts` | — | — |
| GET | `/documents/supported_file_types` | — | — |
| POST | `/documents/reprocess_failed` | — | — |
| POST | `/documents/recovery/force_reset` | — | application/json: ForceResetRecoveryRequest (confirm) |
| POST | `/documents/cancel_pipeline` | — | — |
| POST | `/query` | — | application/json: QueryRequest (query*, mode, only_need_context, only_need_prompt, response_type, top_k, chunk_top_k, max_entity_tokens, max_relation_tokens, max_total_tokens, hl_keywords, ll_keywords, conversation_history, user_prompt, disable_user_prompt_prefix, enable_rerank, include_references, include_chunk_content, include_progress, stream) |
| POST | `/query/stream` | — | application/json: QueryRequest (query*, mode, only_need_context, only_need_prompt, response_type, top_k, chunk_top_k, max_entity_tokens, max_relation_tokens, max_total_tokens, hl_keywords, ll_keywords, conversation_history, user_prompt, disable_user_prompt_prefix, enable_rerank, include_references, include_chunk_content, include_progress, stream) |
| POST | `/query/data` | — | application/json: QueryRequest (query*, mode, only_need_context, only_need_prompt, response_type, top_k, chunk_top_k, max_entity_tokens, max_relation_tokens, max_total_tokens, hl_keywords, ll_keywords, conversation_history, user_prompt, disable_user_prompt_prefix, enable_rerank, include_references, include_chunk_content, include_progress, stream) |
| GET | `/graph/label/list` | — | — |
| GET | `/graph/label/popular` | query:limit | — |
| GET | `/graph/label/search` | query:q (required), query:limit | — |
| GET | `/graphs` | query:label (required), query:max_depth, query:max_nodes | — |
| GET | `/graph/entity/exists` | query:name (required) | — |
| POST | `/graph/entity/edit` | — | application/json: EntityUpdateRequest (entity_name*, updated_data*, allow_rename, allow_merge) |
| POST | `/graph/relation/edit` | — | application/json: RelationUpdateRequest (source_id*, target_id*, updated_data*) |
| POST | `/graph/entity/create` | — | application/json: EntityCreateRequest (entity_name*, entity_data*) |
| POST | `/graph/relation/create` | — | application/json: RelationCreateRequest (source_entity*, target_entity*, relation_data*) |
| POST | `/graph/entities/merge` | — | application/json: EntityMergeRequest (entities_to_change*, entity_to_change_into*) |
| DELETE | `/graph/entity/delete` | — | application/json: DeleteEntityRequest (entity_name*) |
| DELETE | `/graph/relation/delete` | — | application/json: DeleteRelationRequest (source_entity*, target_entity*) |
| GET | `/ui/customization` | query:locale | — |
| GET | `/ui/customization/assets/{asset_hash}/{asset_id}` | path:asset_hash (required), path:asset_id (required) | — |
| GET | `/api/version` | — | — |
| GET | `/api/tags` | — | — |
| GET | `/api/ps` | — | — |
| POST | `/api/generate` | — | Supplemental OllamaGenerateRequest (model*, prompt*, system, stream, options) |
| POST | `/api/chat` | — | Supplemental OllamaChatRequest (model*, messages*, stream, options, system) |
| GET | `/` | — | — |
| GET | `/auth/verify` | — | — |
| GET | `/auth-status` | — | — |
| POST | `/login` | — | application/x-www-form-urlencoded: Body_login_login_post (grant_type, username*, password*, scope, client_id, client_secret) |
| GET | `/health` | — | — |

`*` marks a required body field. The checked-in schema is authoritative for types, bounds, defaults, enums, nullable values, and nested object contracts.

## Critical semantic requirements

- `/query/stream` is NDJSON, not SSE. Preserve reference/progress/timing records. An HTTP 200 stream can contain an error object after partial content; exit nonzero and preserve partial evidence.
- `/api/chat` defaults stream true; `/api/generate` defaults false. Both accept JSON or UTF-8 JSON with octet-stream Content-Type, not arbitrary binary. Chat requires a nonempty messages array with final role user. options and images exist in the model but the deployed handler does not forward them; do not promise they affect generation.
- Login is form-encoded, not JSON. Authentication alternatives are bearer JWT and X-API-Key. `/auth/verify` verifies actual credentials; health can succeed unauthenticated.
- Documents pagination is POST JSON (page 1-based, page_size 10–200); source_conflicts is opaque cursor pagination. Never interpret bounded conflict samples as full candidate sets.
- Source conflict repair defaults server dry_run=true and requires candidate count/fingerprint CAS fields to commit. Distinguish this server-side preview from CLI offline --dry-run.
- Force-reset recovery requires explicit confirm=true and may reopen inconsistent storage. Destructive document/graph commands must never run during smoke tests.
- Upload is multipart field file; preserve filename. Graph asset GET may return binary and root may redirect to UI, not JSON.
- Nullable booleans require omitted/null/true/false fidelity; arrays and arbitrary nested object fields require JSON input. Entity update and relation update field names differ from create.
- Clear-cache body is required but empty {}. It always clears all cache; older mode arguments do not scope it.
- Async accepted statuses are not completed work. Scan can skip while pipeline busy; reprocess returns an empty track_id and retains original IDs.
- Query user_prompt changes generation only, not retrieval. Empty prompt does not suppress configured prefix; disable_user_prompt_prefix does. conversation_history does not affect retrieval.
- Source references are paths and chunk text, not guaranteed public URLs. Preserve llm_generated=false for context/prompt/canned responses.
- CLI --help and offline --dry-run must work without auth or server access. Do not silently retry mutations or emit secrets in previews/errors.
