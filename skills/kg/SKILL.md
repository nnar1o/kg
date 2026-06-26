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

Use these as reference patterns for high-quality, traceable graph data. All operations use the `kg` script runner. Call `kg_help <domain>` for full syntax.

### Create Node (Complete Metadata)

```json
{
  "tool": "kg",
  "arguments": {
    "script": "fridge node add concept:fridge_energy_profile --type Concept --name \"Fridge Energy Profile\" --description \"Model of daily and seasonal refrigerator energy behavior.\" --domain-area kitchen_iot --provenance D --confidence 0.93 --importance 0.88 --created-at 2026-02-14T09:10:00Z --fact \"Average daily consumption is tracked per cooling mode.\" --fact \"Door-open frequency strongly impacts compressor cycles.\" --alias \"energy profile\" --alias \"fridge power model\" --source \"CONFLUENCE https://confluence.abc/display/FRIDGE/energy-model v3\" --source \"LOG telemetry/fridge-eu-17.log 2026-02-01..2026-02-07\" --source \"CONVERSATION 2026-02-12 ai chat with maintenance lead\""
  }
}
```

### Create Edge (Meaningful Relation + Detail)

```json
{
  "tool": "kg",
  "arguments": {
    "script": "fridge edge add process:compressor_control_loop TRIGGERS process:auto_defrost_scheduler --detail \"Defrost scheduler is triggered after compressor runtime threshold is exceeded.\""
  }
}
```

### Search (Discovery + Verification)

```json
{
  "tool": "kg",
  "arguments": {
    "script": "fridge node find \"energy profile compressor defrost\" \"kitchen_iot\" --full --output-size 1200 --skip-feedback"
  }
}
```

```json
{
  "tool": "kg",
  "arguments": {
    "script": "fridge node get concept:fridge_energy_profile --full --output-size 1200"
  }
}
```

### Why These Are Good Patterns

- Include full metadata (`description`, `domain_area`, `provenance`, `confidence`, `importance`, `created_at`).
- Add multiple atomic `facts[]` instead of one long paragraph.
- Add `aliases[]` with realistic synonyms used by different teams.
- Attach multiple `sources[]` with explicit, auditable context.
- Use edge `detail` to explain causal or operational meaning, not just restate relation.

# 3. Tools (3)

All kg operations are exposed through 3 MCP tools. Call `kg_help <domain>` for detailed syntax and examples before unfamiliar operations.

| Tool | Purpose | Parameters |
| --- | --- | --- |
| `kg` | Execute one or more kg commands: find/get nodes, CRUD nodes/edges, graph create/stats, audit, feedback. | `script` (string), `mode?` (`best_effort`\|`strict`), `debug?` (bool) |
| `kg_help` | Return detailed manual with examples for a domain. | `domain` (string): `node`, `edge`, `graph`, `schema`, `kql`, `feedback`, `batch`, `script`, `all` |
| `kg_schema` | Return valid node types, relations, ID prefixes, and edge rules. | (none) |

### Recommended Defaults

- Start with `kg_schema()` to discover valid types/relations.
- Call `kg_help(domain="node")` or `kg_help(domain="edge")` before unfamiliar operations.
- Use `kg` for all execution. Multi-step flows (find → feedback → get) go in one script call.
- Inspect `structured_content.requires_feedback` after `node find`/`node get`; submit feedback via `uid=...` lines in the next `kg` call.
