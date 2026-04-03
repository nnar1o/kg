# kg-assistant - LLM-guided graph improvement

Use this skill when working with a user to collaboratively improve a knowledge graph. The LLM analyzes gaps, asks the user for information, and updates the graph.

Preferred command pattern: `kg graph <graph> ...`.

## Workflow

### Phase 1: Discovery (do once at start)

```bash
# Get graph overview
kg graph <graph> stats --by-type --by-relation

# Find quality issues
kg graph <graph> quality missing-descriptions --limit 10
kg graph <graph> quality missing-facts --limit 10
kg graph <graph> quality edge-gaps --limit 10
kg graph <graph> quality duplicates
```

### Phase 2: Gap prioritization

Rank gaps by importance:
1. **Orphan nodes** - nodes with no edges
2. **Missing descriptions** - nodes users won't understand
3. **Missing facts** - nodes with no key facts
4. **Edge gaps** - structural holes
5. **Duplicates** - naming inconsistencies

### Phase 3: Collaborative filling

For each gap, follow this pattern:

```
## Gap: [describe the gap]

**What I need from you:**
- [specific question 1]
- [specific question 2]
- [specific question 3]

**Example response format:**
If asking about a concept, ask for:
- Name (in Polish/English as appropriate)
- Short description (1-2 sentences)
- Key facts (2-3 bullet points)
- Related existing nodes (optional)
```

## Asking good questions

### For missing node (new concept):

```
Znalazłem, że w grafie brakuje węzła dla "[topic]".

Żeby go dodać, potrzebuję:
1. **Nazwa** - jak to nazwać? (np. "XYZ")
2. **Opis** - co to jest w 1-2 zdaniach?
3. **Kluczowe fakty** - 2-3 rzeczy które warto wiedzieć?
4. **Powiązania** - z czym to się łączy w istniejącym grafie?

Możesz też wskazać istniejący węzeł który jest podobny, jeśli taki jest.
```

### For missing description:

```
Węzeł "[node_id]" ([node_name]) nie ma opisu.

Czy możesz wyjaśnić w 1-2 zdaniach co to jest?
Jeśli znasz też kluczowe fakty o tym, to dodatkowy plus.
```

### For missing facts:

```
Węzeł "[node_id]" ma opis, ale brakuje mu kluczowych faktów.

Jakie 2-3 rzeczy są najważniejsze do zapamiętania o "[node_name]"?
Np. zachowania, ograniczenia, zależności.
```

### For edge gaps:

```
Węzeł "[node_id]" ([node_name]) nie ma żadnych powiązań.

Co jest z tym związane?
- Co go "używa" / wywołuje?
- Co przechowuje / gdzie jest dane?
- Co to "ma" / zawiera?

Pomocna podpowiedź: [list existing related nodes if any]
```

## Adding nodes from user input

After getting user response, add the node:

```bash
# Add node
kg graph <graph> node add <type>:<id> \
  --type <Type> \
  --name "<name>" \
  --description "<description>" \
  --source user-input \
  --fact "<fact1>" \
  --fact "<fact2>"

# Add edge
kg graph <graph> edge add <source_id> <RELATION> <target_id> --detail "<optional detail>"
```

## Session tracking

Keep track of what you've done:

```
## Session progress

Done:
- [x] Added concept:xyz (2024-01-15)
- [x] Connected process:abc to datastore:xyz (2024-01-15)

In progress:
- [ ] User asked about "ABC" - waiting for response
- [ ] Missing description for concept:foo

Todo:
- [ ] Fix edge gaps for datastore nodes
- [ ] Review duplicates
```

## Template: Full gap analysis report

When presenting gaps to user:

```
## Analiza grafu "<graph>"

### Podsumowanie
- Węzłów: N | Krawędzi: M
- Typy: Concept(N), Process(N), DataStore(N)...

### Priorytety do uzupełnienia

**1. Brakujące opisy** (N węzłów)
Najważniejsze:
- concept:xyz - "Nazwa"
- process:abc - "Nazwa"

**2. Brakujące fakty** (N węzłów)
Najważniejsze:
- concept:xyz - "Nazwa" (M powiązań)

**3. Luki strukturalne** (N przypadków)
- DataStore bez STORED_IN: datastore:xyz
- Process bez wejść: process:abc

**4. Duplikaty do sprawdzenia** (N par)
- concept:abc <-> concept:xyz (podobieństwo: 0.85)

---

Chcesz, żebym zaczął od jakiegoś konkretnego tematu? 
Mogę też przejść przez wszystkie po kolei.
```

## Best practices

1. **Ask one gap at a time** - don't overwhelm the user
2. **Provide context** - show the user what's already in the graph
3. **Accept partial info** - even 1 fact is better than none
4. **Suggest based on patterns** - if user mentions X, check if similar Y exists
5. **Mark provenance** - use `--source user-input` for user-provided content
6. **Verify after add** - run `kg graph <graph> node get <id>` to confirm

## Workflow commands summary

```bash
# Discovery
kg graph <graph> stats --by-type
kg graph <graph> quality missing-descriptions --limit 5
kg graph <graph> quality missing-facts --limit 5

# Add from user input
kg graph <graph> node add <id> --type <Type> --name "<name>" --description "<desc>" --source user-input --fact "<fact>"

# Connect
kg graph <graph> edge add <source> <RELATION> <target> --detail "<detail>"

# Verify
kg graph <graph> check
kg graph <graph> node get <id> --full
```
