# kg-builder - graph generation skill

Use this skill when asked to build or extend a knowledge graph from source material: documentation, code, specs, tickets, etc.

## Your job

Read the provided source material and extract entities and relations into a knowledge graph using the `kg` CLI.

## Step 1: plan before execution

Before adding nodes, briefly answer:
- What are the main concepts/entities in this material?
- What processes or data flows exist?
- What rules, interfaces, or data stores appear?
- What are the key relationships between them?

Then produce a short plan that includes:
- Proposed nodes with types and IDs
- Proposed edges (source -> relation -> target)
- Any ambiguities or missing info that block accurate extraction

Do not skip this step. Bad node decomposition is the main source of low-quality graphs.

## Step 2: pick or create the graph

```bash
# Create a new graph if needed
kg create <graph_name>

# Or use an existing one
kg <graph> node find <topic>   # check what already exists
```

## Step 3: add nodes

Use the right type for each entity:

| Type      | When to use                                         |
|-----------|-----------------------------------------------------|
| Concept   | Domain concept, entity, data structure, object      |
| Process   | Algorithm, workflow, job, function, operation       |
| DataStore | Database, file, cache, queue, storage               |
| Interface | API, service, protocol, external system             |
| Rule      | Business rule, constraint, policy, validation       |
| Feature   | Product feature (hide by default in output)         |
| Decision  | Architectural or design decision (ADR-like)         |
| Convention| Naming or structural convention, standard           |
| Note      | Temporary note, observation, unclassified           |
| Bug       | Known defect or issue                               |

Node ID convention: `prefix:snake_case`

```bash
kg <graph> node add concept:benefit_pool \
  --type Concept \
  --name "Benefit Pool" \
  --description "A pool of benefits allocated to employees" \
  --fact "Each pool has a budget and expiry date" \
  --fact "Pools can be shared across benefit groups" \
  --alias "Pool" \
  --source benefits_spec.md \
  --domain-area benefits \
  --provenance doc \
  --confidence 0.9
```

Tips:
- `--fact` should be short, dense, and factual (1-2 sentences max)
- aim for 3-7 facts per node; fewer is fine for simple concepts
- `--alias` for common abbreviations or synonyms
- `--source` should reference the file or doc you read it from
- `--confidence` between 0.0 and 1.0 (omit if unsure, default is absent)
- `--domain-area` groups nodes by subsystem or module

## Step 4: add edges

Use relations that describe real structural or semantic links:

| Relation        | Meaning                                        |
|-----------------|------------------------------------------------|
| `HAS`           | Parent owns or contains child                  |
| `USES`          | Component uses another                         |
| `DEPENDS_ON`    | Hard dependency                                |
| `IMPLEMENTS`    | Realizes an interface or rule                  |
| `STORES`        | Process writes to a DataStore                  |
| `STORED_IN`     | Concept/data is stored in a DataStore          |
| `READS_FROM`    | Process reads from a DataStore                 |
| `TRIGGERS`      | Event or process triggers another              |
| `VALIDATES`     | Rule or process validates a concept            |
| `DOCUMENTED_IN` | Node documented in a Feature or spec           |
| `GOVERNED_BY`   | Concept governed by a Rule                     |
| `EXPOSES`       | Interface exposes a Concept or operation       |
| `CALLS`         | Process or Interface calls another Interface   |

```bash
kg <graph> edge add concept:benefit_pool GOVERNED_BY rule:pool_budget_rule
kg <graph> edge add process:pool_allocation READS_FROM datastore:benefit_db
kg <graph> edge add process:pool_allocation STORES datastore:allocation_log --detail "writes final allocation records after approval"
```

Tips:
- add `--detail` when the relation has nuance worth preserving
- prefer specific relations (`READS_FROM`, `STORES`) over generic ones (`USES`) where applicable
- one edge per actual relationship — do not add redundant edges

## Step 5: verify and validate

```bash
kg <graph> check
kg <graph> stats --by-type --by-relation
```

Fix any errors before finishing. Common issues:
- missing `source_files`
- referential integrity errors (edge pointing to non-existent node)
- invalid node type or ID format

## Step 6: spot-check output

```bash
kg <graph> node find <key_term>
kg <graph> node get <id>
```

Confirm the output looks clean and facts are legible to an AI reading them cold.

## Quality bar

A good node:
- has a clear, non-redundant name
- has 2+ key facts that are not obvious from the name alone
- has at least one outgoing or incoming edge
- has a source file reference

A good graph:
- has no orphan nodes (every node has at least one edge)
- has no DataStore without an incoming `STORED_IN` or `STORES` edge
- has no Process without at least one incoming or outgoing edge
- passes `kg <graph> check` with zero errors

## Example workflow

```bash
# 1. create graph
kg create myapp

# 2. add nodes from code/docs
kg myapp node add concept:user_session --type Concept --name "User Session" ...
kg myapp node add datastore:session_db --type DataStore --name "Session DB" ...
kg myapp node add process:session_cleanup --type Process --name "Session Cleanup" ...

# 3. connect them
kg myapp edge add process:session_cleanup READS_FROM datastore:session_db
kg myapp edge add process:session_cleanup STORES datastore:audit_log

# 4. validate
kg myapp check
kg myapp stats --by-type
```
