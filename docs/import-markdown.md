# Markdown import format (Sprint 7)

Files with YAML frontmatter are supported. Frontmatter is delimited by `---`.

Node example:

```
---
id: concept:refrigerator
type: Concept
name: Refrigerator
description: Cooling appliance
domain_area: appliance
provenance: docs
alias: [lodowka, fridge]
key_facts: ["Energy class A++", "Capacity 300L"]
source_files: [manual.md]
---
Optional body (ignored by default).
```

Note example:

```
---
note: true
note_id: note:energy
node_id: concept:refrigerator
tags: [backlog, research]
author: mario
created_at: 2026-03-20T00:00:00Z
provenance: field
---
This is the note body.
```

Rules:
- `note: true` or `note_id`/`node_id` triggers note import.
- If `note_id` is missing, it is derived from the filename.
- `import-md --notes-as-nodes` stores markdown bodies as node facts.
