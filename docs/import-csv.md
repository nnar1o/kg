# CSV import format (Sprint 7)

Delimiter: comma, RFC CSV.

Nodes CSV columns:
- `id`, `type`, `name` (required)
- optional: `description`, `domain_area`, `provenance`, `confidence`, `created_at`,
  `key_facts`, `alias`, `source_files`

Edges CSV columns:
- `source_id`, `relation`, `target_id` (required)
- optional: `detail`

Notes CSV columns:
- `id`, `node_id` (required)
- optional: `body`, `tags`, `author`, `created_at`, `provenance`, `source_files`

List fields use `|` as a separator (`alias`, `key_facts`, `tags`, `source_files`).
