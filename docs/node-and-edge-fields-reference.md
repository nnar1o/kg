# Node and Edge Fields Reference

This document describes all fields stored in graph nodes and edges.

## Node Fields

| Field | Type | Required | Description | Constraints / Notes | Example |
|---|---|---:|---|---|---|
| `id` | string | Yes | Unique node identifier. | Must follow `prefix:snake_case` format and match the node type ID prefix rules. | `concept:graph_model` |
| `name` | string | Yes | Human-readable node name. | Keep it concise and clear. | `Graph Model` |
| `node_type` | enum(string) | Yes | Semantic category of the node. | Allowed values: `Concept`, `Process`, `DataStore`, `Interface`, `Rule`, `Feature`, `Decision`, `Convention`, `Note`, `Bug`. | `Concept` |
| `description` | string \| null | No | Short explanation of what the node represents. | Optional but recommended. | `Represents the core graph entity model.` |
| `facts` | string[] | No (defaults to empty) | Atomic factual statements about the node. | Prefer specific facts over long prose. | `["Node stores entity metadata"]` |
| `aliases` | string[] | No (defaults to empty) | Alternative names or synonyms. | Improves discovery and matching. | `["vertex", "entity node"]` |
| `sources` | string[] | No (defaults to empty) | Supporting references for node data. | Can include docs, URLs, tickets, file paths. | `["docs/schema.md"]` |
| `provenance` | string \| null | No | Origin/context for the information. | Example values: imported, inferred, manual. | `Imported from architecture docs` |
| `domain_area` | string \| null | No | Domain or subsystem classification. | Helps grouping and filtering. | `Knowledge Graph` |
| `confidence` | number \| null | No | Confidence score for node correctness. | Numeric score; exact scale is policy-defined. | `0.92` |
| `importance` | integer \| null | No | Relative importance or priority. | Integer range `0..255`. | `180` |
| `created_at` | string \| null | No | Creation timestamp metadata. | Typically ISO-8601 format. | `2026-04-10T12:30:00Z` |
| `valid_from` | string \| null | No | Start of validity period for facts. | Empty = valid from beginning. | `2026-01-01` |
| `valid_to` | string \| null | No | End of validity period for facts. | Empty = still valid (current). | `2026-04-01` |

## Edge Fields

| Field | Type | Required | Description | Constraints / Notes | Example |
|---|---|---:|---|---|---|
| `source_id` | string | Yes | ID of the source node (edge start). | Must reference an existing node ID. | `process:sync_pipeline` |
| `target_id` | string | Yes | ID of the target node (edge end). | Must reference an existing node ID. | `datastore:graph_db` |
| `relation` | enum(string) | Yes | Relationship type between source and target. | Must be one of the allowed relation values. | `READS_FROM` |
| `detail` | string \| null | No | Additional context for the relationship. | Optional free-text clarification. | `Reads incremental updates every 5 minutes` |
| `valid_from` | string \| null | No | Start of validity period for the relationship. | Empty = valid from beginning. | `2026-01-01` |
| `valid_to` | string \| null | No | End of validity period for the relationship. | Empty = still valid (current). | `2026-04-01` |

## Allowed Edge Relation Values

| Relation | Meaning |
|---|---|
| `HAS` | Source contains or includes target. |
| `STORED_IN` | Source is stored in target. |
| `TRIGGERS` | Source initiates or activates target. |
| `CREATED_BY` | Source was created by target. |
| `AFFECTED_BY` | Source is influenced or modified by target. |
| `AVAILABLE_IN` | Source is available within target context. |
| `DOCUMENTED_IN` | Source is documented in target. |
| `DEPENDS_ON` | Source requires target to function. |
| `TRANSITIONS` | Source transitions to target state or stage. |
| `DECIDED_BY` | Source is decided by target decision. |
| `GOVERNED_BY` | Source is governed by target rule or policy. |
| `USES` | Source uses target. |
| `READS_FROM` | Source reads data from target. |
