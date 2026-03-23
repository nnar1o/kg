# Backend storage decision (Sprint 2)

Decision:
- Primary local DB backend: Redb (pure Rust, embedded, B-tree).
- SQLite rejected for this project (user preference).
- Vector search is treated as a sidecar index, not storage of record.

Context:
- `kg` must remain deterministic and local-first.
- Core storage must be easy to embed, portable, and reliable without a server.
- Semantic search (embeddings) is optional and should not be required for core operations.

Options considered:
- SQLite: strong local DB, but rejected due to user preference.
- Redb: pure Rust, embedded, minimal dependencies, deterministic, good fit for local storage.
- LMDB/RocksDB: strong KV, but brings native dependencies and heavier distribution.
- Vector DB (Qdrant/others): good for ANN search, but not a source of truth for nodes/edges/notes.

Implications:
- JSON remains supported as a reference backend.
- Redb stores the full graph as the authoritative local DB state.
- Vector indexing (if added) will be a deterministic sidecar index keyed by `node_id`.
- `kg` will not compute embeddings; it will only consume provided vectors.
