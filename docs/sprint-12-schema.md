# Sprint 12: Schema/constraints + walidacja na zapisie

## Status: done

## Cel

Wymuszanie jakości danych przy zapisie (nie tylko przy `kg check`).

## Plik schematu

Schemat jest zdefiniowany w pliku `.kg.schema.toml` w katalogu projektu lub jednym z jego rodziców.

### Przykładowy schemat

```toml
# .kg.schema.toml

# Ograniczenie dozwolonych typów węzłów (opcjonalne)
[node_types]
allowed = ["Concept", "Process", "DataStore", "Interface", "Rule", "Feature", "Decision"]

# Ograniczenie dozwolonych relacji (opcjonalne)
[relations]
allowed = ["HAS", "STORED_IN", "TRIGGERS", "CREATED_BY", "AFFECTED_BY", "AVAILABLE_IN", "DOCUMENTED_IN", "DEPENDS_ON", "TRANSITIONS", "DECIDED_BY", "GOVERNED_BY", "USES", "READS_FROM"]

# Wymagane pola dla poszczególnych typów
[node_types.required_fields]
Concept = ["description", "provenance"]
Process = ["description"]
DataStore = ["description", "provenance"]

# Reguły typów dla krawędzi
[[edge_rules]]
relation = "HAS"
source_types = ["Concept", "Process", "Interface"]
target_types = ["Concept", "Feature", "DataStore", "Rule", "Interface"]

[[edge_rules]]
relation = "STORED_IN"
source_types = ["Concept", "Process", "Rule"]
target_types = ["DataStore"]

# Wiązanie prefixów ID do typów
[id_patterns]
enforce_prefix_match = true

[id_patterns.prefix_to_type]
"concept" = "Concept"
"process" = "Process"
"datastore" = "DataStore"
"interface" = "Interface"
"rule" = "Rule"
"feature" = "Feature"
"decision" = "Decision"
"convention" = "Convention"
"note" = "Note"
"bug" = "Bug"

# Ograniczenia unikalności
[[uniqueness]]
scope = "global"
fields = ["type", "name"]
```

## Walidacja na zapisie

Schemat jest automatycznie ładowany z `.kg.schema.toml` (jeśli istnieje) i stosowany przy:

- `kg ... node add` - walidacja przed dodaniem węzła
- `kg ... node modify` - walidacja po modyfikacji węzła
- `kg ... edge add` - walidacja przed dodaniem krawędzi
- `kg ... import-csv` - walidacja wszystkich zaimportowanych węzłów/krawędzi
- `kg ... import-md` - walidacja wszystkich zaimportowanych węzłów/krawędzi

## Raportowanie naruszeń

Naruszenia schematu powodują błąd z jasnym komunikatem:

```
schema violations:
  - node type 'CustomType' is not allowed by schema (allowed: ["Concept", "Process", ...])
  - node concept:fridge (type 'Concept') is missing required field 'description'
  - edge concept:temp DEPENDS_ON datastore:cache has invalid target type 'DataStore' (allowed: ["Feature", "DataStore", ...])
```

## Wyjście (DoD)

- Da się wymusić spójność i kompletność danych przy zapisie.
- Schemat jest opcjonalny - jeśli `.kg.schema.toml` nie istnieje, walidacja nie jest przeprowadzana.
- Istniejące operacje bez schematu działają bez zmian.
