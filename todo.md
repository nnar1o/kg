# TODO

## MCP tool kg_node_add / kg_node_add_batch - brak szczegolow bledow

**Problem:** MCP tool `kg_node_add_batch` / `kg_node_add` zwracają generyczne "kg command failed" bez szczegółów. Dopiero gdy użyłem CLI (kg przez Execute) dostałem prawdziwe komunikaty:
- `importance must be in range 1..=6, got 8` -- używałem skali 1-10, a KG wymaga 1-6
- `at least one --source is required` -- node wymaga parametru --source, którego nie podawałem przez MCP tool

**Wniosek:** MCP wrapper kg_node_add połyka szczegóły błędu i zwraca tylko "kg command failed". CLI daje pełną diagnostykę.