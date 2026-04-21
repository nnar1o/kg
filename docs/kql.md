# KQL (kg query language) v1

## Format podstawowy

```
<kind> <filter> [filter ...] [limit=N] [sort=field]
```

Kinds:
- `node`, `edge`, `note`

## Filtry

- Equality: `key=value`
- Contains: `key~value` (substring match)
- Not equal: `key!=value`
- Prefix: `key^value`
- Greater or equal: `key>=value`
- Less or equal: `key<=value`
- Greater: `key>value`
- Less: `key<value`

## Filtry czasowe

Do filtrowania po zakresach czasowych używaj `created_at` z operatorami porównania:
- Wszystkie node'y od dnia X: `created_at>=2026-04-20`
- Wszystkie node'y do dnia X: `created_at<2026-04-21`
- Wszystkie node'y w danym dniu: `created_at^2026-04-20` (prefix match)
- Zakres: `created_at>=2026-04-20 created_at<2026-04-21`

## Przykłady podstawowe

```
node type=Concept name~lodowka
edge relation=DEPENDS_ON
note tag=backlog
node type=Concept limit=10 sort=name
node created_at>=2026-04-20 sort=-created_at  # nodes od 2026-04-20
node created_at>=2026-04-20 created_at<2026-04-21  # nodes z 2026-04-20
```

## Sortowanie i limit

```
node type=Concept sort=name          # sortowanie rosnące
node type=Concept sort=-name        # sortowanie malejące
node type=Concept limit=5            # tylko 5 wyników
```

### Pola sortowania dla node:
- `name`, `type`, `id`
- `created_at` (lub `created`) — data utworzenia
- `importance` — waga node (0.0 - 1.0)
- `updated_at` (lub `updated`) — data modyfikacji

### Pola sortowania dla edge:
- `source`, `relation`, `target`

### Pola sortowania dla note:
- `id`, `node`, `created`

## Traversale (n-hop)

### Sąsiedztwo

```
neighbors id=<node_id> [hops=N] [out|in|both] [limit=N]
```

Przykłady:
```
neighbors id=concept:refrigerator              # 1-hop, obie strony
neighbors id=concept:refrigerator hops=2       # 2-hop
neighbors id=concept:refrigerator out          # tylko wychodzące
neighbors id=concept:refrigerator in           # tylko przychodzące
neighbors id=concept:refrigerator limit=10     # max 10 wyników
```

### Ścieżka

```
path from=<id> to=<id> [hops=N]
```

Przykłady:
```
path from=concept:refrigerator to=concept:temperature
path from=concept:foo to=concept:bar hops=5
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
- `id`, `type`, `name`, `description`, `domain_area`, `provenance`, `alias`, `fact`, `source`, `confidence`, `importance`, `created_at`, `updated_at`

Edges:
- `source_id`, `target_id`, `relation`, `detail`

Notes:
- `id`, `node_id`, `body`, `tag`, `author`, `provenance`, `source`

**Uwaga:** Filtrowanie po `created_at`/`updated_at` działa z operatorami `>=`, `<=`, `>`, `<` oraz prefix `^`. Wartości są porównywane leksykograficznie — działa poprawnie dla ISO 8601 timestamps.

## Temporal Validity (valid_from / valid_to)

Każdy node ma opcjonalne pola `valid_from` i `valid_to` określające okres ważności faktu/informacji:

| Pole | Opis |
|------|------|
| `valid_from` | Od kiedy fact jest prawdziwy (puste = od zawsze) |
| `valid_to` | Do kiedy fact jest ważny (puste = wciąż aktualny) |

### Przykłady:

```
# Znajdź fakty które przestały być aktualne
node valid_to>=2026-04-01 valid_to<2026-04-20

# Znajdź fakty które są nadal aktualne (valid_to jest puste)
node valid_to=  # spacja po = oznacza pustą wartość

# Znajdź fakty które weszły w życie po dacie X
node valid_from>=2026-04-01

# Co było prawdziwe w dniu X (as-of query)
# valid_from <= X AND (valid_to = '' OR valid_to > X)
node valid_from<=2026-04-15 valid_to>=2026-04-15
```

### Ustawianie validity przy dodawaniu node:

```bash
kg graph mygraph node add bug:xxx \
  --name "Bug Fix" \
  --type Bug \
  --valid-from 2026-01-01 \
  --valid-to 2026-04-01
```

### Ustawianie validity przy modyfikacji:

```bash
kg graph mygraph node modify bug:xxx --valid-to 2026-04-20
```

## Przykłady użycia

### Lista wszystkich nodes posortowana wg daty utworzenia (najnowsze pierwsze)
```
kg graph <g> kql "node sort=-created_at"
```

### Lista nodes typu Bug z datami
```
kg graph <g> kql "node type=Bug sort=-created_at"
```

### Nodes z importancją (najważniejsze pierwsze)
```
kg graph <g> kql "node sort=-importance limit=10"
```

### Filtrowanie po dacie (nodes utworzone po 2026-04-20)
```
kg graph <g> kql "node created_at~2026-04"
```

## Uwagi

- Filtry są ANDed.
- Wartości są parsowane jako raw tokens (bez cudzysłowów w MVP).
- Contains (`~`) to substring match.
