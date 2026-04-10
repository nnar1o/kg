# 1. What Is kg-mcp

kg-mcp is an MCP interface for the kg knowledge graph, enabling structured read/write operations on nodes and edges.

# 2. Node and Edge Structure

### Node Fields

| Field | Status | Description | Example |
| --- | --- | --- | --- |
| `id` | Strictly Required | Unique node ID in format `<type_code>:snake_case`. | `concept:smart_fridge` |
| `name` | Strictly Required | Human-readable node name. | `Smart Fridge` |
| `node_type` | Strictly Required | Node category (see dictionaries below). | `Concept` |
| `description` | Strictly Required | Short explanation of the node. | `Connected refrigerator with inventory and cooling automation.` |
| `facts[]` | Strongly Recommended | Concrete statements about the node. | `{"Tracks item quantity and expiry dates."}` |
| `aliases[]` | Strongly Recommended | Alternative names for lookup. | `{"fridge", "refrigerator"}` |
| `sources[]` | Strictly Required | Source references using standard per-type formats (see source format cases below). | `{"SVN https://svnurl.abc/manuals/refrigerator-manual.doc 2.1 introduction page 5", "WIKI https://wikipage.abc", "CONVERSATION 2026-01-10 ai chat with user"}` |
| `domain_area` | Strictly Required | Functional area tag. | `kitchen_iot` |
| `provenance` | Strictly Required | Origin metadata using provenance dictionary values (see dictionaries below). | `D` |
| `confidence` | Strictly Required | Confidence score in range `0..1` (`0` = information is certainly false, `1` = information is certain). | `0.95` |
| `importance` | Strictly Required | Importance score in range `0..1` (`0` = negligible priority, `1` = critical priority). | `0.9` |
| `created_at` | Strictly Required | Creation timestamp. | `2026-04-10T12:30:00Z` |

### Edge Fields

| Field | Status | Description | Example |
| --- | --- | --- | --- |
| `source_id` | Strictly Required | ID of the source node in an edge. | `process:monitor_fridge_temperature` |
| `target_id` | Strictly Required | ID of the target node in an edge. | `datastore:fridge_telemetry_store` |
| `relation` | Strictly Required | Edge relation type (see dictionaries below). | `STORED_IN` |
| `detail` | Optional (Nice to Have) | Optional edge-level context. | `Temperature samples are stored every 5 minutes.` |

### Dictionaries and Source Formats

Node type dictionary: `Concept`, `Process`, `DataStore`, `Interface`, `Rule`, `Feature`, `Decision`, `Convention`, `Note`, `Bug`.

Edge relation dictionary: `HAS`, `STORED_IN`, `TRIGGERS`, `CREATED_BY`, `AFFECTED_BY`, `AVAILABLE_IN`, `DOCUMENTED_IN`, `DEPENDS_ON`, `TRANSITIONS`, `DECIDED_BY`, `GOVERNED_BY`, `USES`, `READS_FROM`.

Provenance dictionary: `U` = User input, `D` = Documentation scan, `A` = AI deduction.

Source type dictionary: `URL`, `SVN`, `SOURCECODE`, `WIKI`, `CONFLUENCE`, `CONVERSATION`, `GIT_COMMIT`, `PULL_REQUEST`, `ISSUE`, `DOC`, `LOG`, `OTHER`.

Source format cases:
- `URL <URL> <OPTIONAL_DETAILS>`
- `SVN <URL> <OPTIONAL_DETAILS>`
- `SOURCECODE <URL_OR_PATH> <OPTIONAL_DETAILS>`
- `WIKI <URL> <OPTIONAL_DETAILS>`
- `CONFLUENCE <URL> <OPTIONAL_DETAILS>`
- `CONVERSATION <DATE> <OPTIONAL_WITH_WHOM>`
- `GIT_COMMIT <REPO_URL_OR_NAME> <COMMIT_SHA> <OPTIONAL_DETAILS>`
- `PULL_REQUEST <URL> <OPTIONAL_DETAILS>`
- `ISSUE <URL_OR_ID> <OPTIONAL_DETAILS>`
- `DOC <PATH_OR_URL> <OPTIONAL_DETAILS>`
- `LOG <SYSTEM_OR_PATH> <TIME_RANGE_OR_DETAILS>`
- `OTHER <REFERENCE> <OPTIONAL_DETAILS>`

Examples:
- `SVN https://svnurl.abc/manuals/refrigerator-manual.doc 2.1 introduction page 5`
- `WIKI https://wikipage.abc`
- `CONVERSATION 2026-01-10 ai chat with user`
- `GIT_COMMIT https://git.abc/fridge-platform.git a1b2c3d adjusted cooling thresholds`
- `PULL_REQUEST https://git.abc/fridge-platform/pull/245 review approved by ops`
- `ISSUE FRIDGE-431 compressor cycle too frequent`
- `DOC /docs/fridge/safety-guidelines.pdf chapter 3`
- `LOG telemetry/fridge-12.log 2026-01-10T10:00:00Z..2026-01-10T12:00:00Z`
- `OTHER vendor_email_2026-01-09 warranty clarification`

## 2.1 Examples (Best Practices)

Use these as reference patterns for high-quality, traceable graph data.

### Create Node (Complete Metadata)

```json
{
  "tool": "kg-mcp_kg_node_add",
  "arguments": {
    "graph": "fridge",
    "id": "concept:fridge_energy_profile",
    "name": "Fridge Energy Profile",
    "node_type": "Concept",
    "description": "Model of daily and seasonal refrigerator energy behavior.",
    "domain_area": "kitchen_iot",
    "provenance": "D",
    "confidence": 0.93,
    "importance": 0.88,
    "created_at": "2026-02-14T09:10:00Z",
    "facts": [
      "Average daily consumption is tracked per cooling mode.",
      "Door-open frequency strongly impacts compressor cycles."
    ],
    "aliases": ["energy profile", "fridge power model"],
    "sources": [
      "CONFLUENCE https://confluence.abc/display/FRIDGE/energy-model v3",
      "LOG telemetry/fridge-eu-17.log 2026-02-01..2026-02-07",
      "CONVERSATION 2026-02-12 ai chat with maintenance lead"
    ]
  }
}
```

### Create Edge (Meaningful Relation + Detail)

```json
{
  "tool": "kg-mcp_kg_edge_add",
  "arguments": {
    "graph": "fridge",
    "source_id": "process:compressor_control_loop",
    "relation": "TRIGGERS",
    "target_id": "process:auto_defrost_scheduler",
    "detail": "Defrost scheduler is triggered after compressor runtime threshold is exceeded."
  }
}
```

### Search (Discovery + Verification)

```json
{
  "tool": "kg-mcp_kg_node_find",
  "arguments": {
    "graph": "fridge",
    "queries": ["energy profile compressor defrost", "kitchen_iot"],
    "full": true,
    "output_size": 1200,
    "skip_feedback": true
  }
}
```

```json
{
  "tool": "kg-mcp_kg_node_get",
  "arguments": {
    "graph": "fridge",
    "id": "concept:fridge_energy_profile",
    "full": true,
    "output_size": 1200
  }
}
```

### Why These Are Good Patterns

- Include full metadata (`description`, `domain_area`, `provenance`, `confidence`, `importance`, `created_at`).
- Add multiple atomic `facts[]` instead of one long paragraph.
- Add `aliases[]` with realistic synonyms used by different teams.
- Attach multiple `sources[]` with explicit, auditable context.
- Use edge `detail` to explain causal or operational meaning, not just restate relation.

# 3. Tools (Current Only)

Use these tools for active kg-mcp workflows (deprecated tools intentionally omitted).

### Core Execution

| Tool | Purpose | Typical Use |
| --- | --- | --- |
| `kg-mcp_kg` | Run one or more kg commands as a script. | Best default for multi-step flows: find/get/update/feedback in one call. |
| `kg-mcp_kg_command` | Run a single kg CLI command via args array. | Good for one-shot operations when scripting is not needed. |

### Graph Metadata and Quality

| Tool | Purpose | Typical Use |
| --- | --- | --- |
| `kg-mcp_kg_schema` | Return valid node types, relations, ID rules, edge rules. | Validate structure expectations before writes. |
| `kg-mcp_kg_stats` | Return graph usage and structure statistics. | Fast graph overview and trend checks. |
| `kg-mcp_kg_gap_summary` | Return top quality gaps (facts, descriptions, edge gaps, duplicates). | Prioritize cleanup and enrichment work. |
| `kg-mcp_kg_feedback_batch` | Submit relevance feedback (`YES`/`NO`/`PICK`). | Close the feedback loop after find/get calls. |

### Graph Lifecycle

| Tool | Purpose | Typical Use |
| --- | --- | --- |
| `kg-mcp_kg_create_graph` | Create a new graph. | Bootstrap a new domain graph (for example `fridge`). |

### Node Operations

| Tool | Purpose | Typical Use |
| --- | --- | --- |
| `kg-mcp_kg_node_find` | Search nodes using one or more queries. | Discovery, candidate collection, semantic lookup. |
| `kg-mcp_kg_node_get` | Fetch a node by ID (with optional full payload). | Detailed inspection before modify/edge updates. |
| `kg-mcp_kg_node_add` | Add one node. | High-quality single node creation. |
| `kg-mcp_kg_node_add_batch` | Add many nodes in one request. | Imports and large structured updates. |
| `kg-mcp_kg_node_modify` | Modify node fields. | Metadata refinement and corrections. |
| `kg-mcp_kg_node_remove` | Remove a node and incident edges. | Controlled cleanup of obsolete entities. |

### Edge Operations

| Tool | Purpose | Typical Use |
| --- | --- | --- |
| `kg-mcp_kg_edge_add` | Add one edge between nodes. | Express explicit dependency/causality relations. |
| `kg-mcp_kg_edge_add_batch` | Add many edges (with optional `dry_run`). | Bulk relation modeling and migration workflows. |
| `kg-mcp_kg_edge_remove` | Remove an edge between nodes. | Remove stale or invalid relationships. |

### Recommended Defaults

- Prefer `kg-mcp_kg` for end-to-end workflows.
- Use `kg-mcp_kg_node_find` + `kg-mcp_kg_node_get` before any destructive update.
- Use batch tools for volume changes; use single-item tools for precise edits.
