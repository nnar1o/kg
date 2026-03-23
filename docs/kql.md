# KQL (kg query language) v1

## Format podstawowy

```
<kind> <filter> <filter> ... [; sort=<field>] [; limit=<n>]
```

Kinds:
- `node`, `edge`, `note`

## Filtry

- Equality: `key=value`
- Contains: `key~value` (substring match)
- Not equal: `key!=value`
- Prefix: `key^value`

## Przykłady podstawowe

```
node type=Concept name~lodowka
edge relation=DEPENDS_ON
note tag=backlog
node type=Concept ; sort=name ; limit=10
```

## Sortowanie i limit

```
node type=Concept ; sort=name          # sortowanie rosnące
node type=Concept ; sort=-name        # sortowanie malejące
node type=Concept ; limit=5            # tylko 5 wyników
```

Pola sortowania dla node: `name`, `type`, `id`
Pola sortowania dla edge: `source`, `relation`, `target`
Pola sortowania dla note: `id`, `node`, `created`

## Traversale (n-hop)

### Sąsiedztwo

```
neighbors <node_id> [hops=<n>] [out|in|both] [limit=<n>]
```

Przykłady:
```
neighbors concept:refrigerator              # 1-hop, obie strony
neighbors concept:refrigerator hops=2       # 2-hop
neighbors concept:refrigerator out          # tylko wychodzące
neighbors concept:refrigerator in           # tylko przychodzące
neighbors concept:refrigerator limit=10     # max 10 wyników
```

### Ścieżka

```
path <from_id> <to_id> [hops=<n>]
```

Przykłady:
```
path concept:refrigerator concept:temperature
path concept:foo concept:bar hops=5
```

## Agregacje

```
count [node|edge|note] by=[type|domain|source|...]
```

Przykłady:
```
count node by=type              # liczba node'ów per typ
count edge by=relation          # liczba krawędzi per relacja
count note by=author            # liczba notatek per autora
count node by=domain           # liczba node'ów per domenę
```

## Obsługiwane klucze

Nodes:
- `id`, `type`, `name`, `description`, `domain_area`, `provenance`, `alias`, `fact`, `source`, `confidence`

Edges:
- `source_id`, `target_id`, `relation`, `detail`

Notes:
- `id`, `node_id`, `body`, `tag`, `author`, `provenance`, `source`

## Uwagi

- Filtry są ANDed.
- Wartości są parsowane jako raw tokens (bez cudzysłowów w MVP).
- Contains (`~`) to substring match.
