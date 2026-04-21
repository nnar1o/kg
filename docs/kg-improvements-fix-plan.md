# Plan Naprawy kg-improvements

**Status:** W trakcie realizacji  
**Graph:** kg-improvements  
**Node:** `process:kg_improvements_fix_plan_v1`  
**Data:** 2026-04-21

---

## Cel

Naprawa bugów i wdrożenie kluczowych feature z największym impact na Developer Experience (DX). Priorytety na podstawie: importance score, frequency of trigger, effort to fix.

---

## Faza 1 — Szybkie Zwycięstwa

**Zasada:** Wysoki impact, niski koszt implementacyjny. Można zrobić w 1-2h.

### 1.1 auto_skip_high_confidence_feedback

| Pole | Wartość |
|------|--------|
| **Typ** | Feature |
| **Bug powiązany** | `bug:feedback_prompt_blocks_lookup` |
| **Importance** | 1.0 |
| **Priority** | TOP 1 |

**Problem:** `node find` wymusza feedback loop (YES/NO/PICK N) nawet dla prostych lookupów. ~80% wywołań to lookup, nie trening rankingu.

**Rozwiązanie:**
- Auto-skip gdy: `top_score > 800` LUB `limit=1`
- Nowa flaga `--with-feedback` dla sesji treningowych (odwrócona logika domyślnej)
- `--skip-feedback` zachować jako alias (backward compatibility)
- Config: `feedback.auto_skip_threshold` (default: 800)

**Implementacja:**
```
kg graph <g> node find "query"  # score 900 → auto skip feedback
kg graph <g> node find "query" --with-feedback  # wymuś feedback
```

---

### 1.2 schema_per_graph_alias

| Pole | Wartość |
|------|--------|
| **Typ** | Feature |
| **Bug powiązany** | `bug:schema_parse_error`, `bug:inconsistent_command_scope` |
| **Importance** | 0.7 |
| **Priority** | TOP 2 |

**Problem:** `kg graph <name> schema` → parse_error. `schema` jest tylko globalne. Najczęstsze źródło confuzji w sesji.

**Rozwiązanie:**
- Alias: `kg graph <name> schema` → deleguje do `kg schema`
- Opcjonalnie: zwróć schema + statystyki typów nodes dla tego grafu
- Zero kosztu implementacyjnego — alias w command router

**Implementacja:**
```bash
kg graph kg-improvements schema
# → wywołuje kg schema, zwraca global schema
# → opcjonalnie: dodaje "nodes: 18, edges: 47" na końcu
```

---

### 1.3 kg_node_find CLI flags parity

| Pole | Wartość |
|------|--------|
| **Typ** | Bug |
| **Node** | `bug:kg_node_find_cli_flags_mismatch_mcp` |
| **Importance** | 0.85 |
| **Priority** | TOP 3 |

**Problem:** MCP tool `kg_node_find` ma parametr `skip_feedback`, ale CLI `kg graph jira node find --skip-feedback true` → `unknown option: --skip-feedback`. Dwa różne interfejsy.

**Rozwiązanie:**
- CLI powinien akceptować flagi zgodne z MCP tool schema
- Lub: tool mapuje `--skip-feedback` → `skip_feedback=true`
- Command router parity check

**Implementacja:**
```bash
kg graph jira node find "CLMGLCEG" --skip-feedback true
# → działa (zamiast ERROR)
```

---

## Faza 2 — Core Fixes

**Zasada:** Większy effort, ale rozwiązuje core UX problems.

### 2.1 KQL fix + documentation + range queries ✅ DONE

| Pole | Wartość |
|------|--------|
| **Typ** | Bug / Feature |
| **Node** | `bug:kg_kql_broken_or_undocumented` |
| **Status** | ✅ Zaimplementowane |

**Zmiany:**
- Parser obsługuje sortowanie po `created_at`, `importance`, `updated_at` (+ aliasy)
- Output zawiera `@ created_at` timestamp
- **Nowe operatory:** `>=`, `<=`, `>`, `<` dla filtrów numerycznych/czasowych
- Filtrowanie po zakresach czasowych: `created_at>=2026-04-20 created_at<2026-04-21`
- Prefix matching: `created_at^2026-04-20` (wszystkie z danego dnia)
- Jednostkowe testy w `kql.rs` (~200 LOC)
- Integracyjne testy w `graph_query.rs`
- Zaktualizowana dokumentacja `docs/kql.md`

---

### 2.2 semantic_error_messages ✅ DONE

| Pole | Wartość |
|------|--------|
| **Typ** | Feature |
| **Bug powiązany** | `bug:generic_error_hints` |
| **Status** | ✅ Zaimplementowane |

**Zmiany:**
- `COMMAND_REGISTRY` — registry wszystkich komend z scope/examples
- `suggest_command()` — fuzzy matching z Levenshtein distance
- `semantic_error_hint()` — zwraca "Did you mean X?" hints
- Specjalny hint dla `schema` → "Did you mean 'kg schema' (global command)?"
- Jednostkowe testy (~30 testów)

**Przykłady:**
```
# ERROR: unrecognized subcommand 'schema'
# → Hint: Did you mean 'kg schema' (global command)? Example: kg schema

# ERROR: unrecognized subcommand 'kqlx'
# → Hint: Did you mean 'kql'? Example: kg graph mygraph kql "nodes"
```

---

### 2.3 list_all_nodes via KQL (feature derived from 2.1)

| Pole | Wartość |
|------|--------|
| **Typ** | Feature |
| **Node** | `feature:list_all_nodes_with_timestamps` |
| **Importance** | 0.95 |
| **Depends on** | 2.1 KQL ✅ DONE |
| **Status** | ⏳ Do zrobienia |

**Problem:** Brak sposobu na wylistowanie wszystkich nodes z metadanymi (id, created_at).

**Rozwiązanie:**
- Feature `list_all_nodes` = convenience wrapper na KQL
- Składnia: `kg graph <g> list [--type Bug|Feature] [--since DATE] [--limit N]`
- Opcjonalnie: eksport do CSV

**Implementacja:**
```bash
# Pod spodem: KQL
kg graph jira list --type Bug --limit 50
kg graph jira list --since 2026-04-20 --fields id,aka,created_at
```

---

## Faza 3 — Stretch Goals

**Zasada:** Niższy priority, ale poprawiają polish.

### 3.1 feedback_ttl_autodiscard ✅ DONE

| Pole | Wartość |
|------|--------|
| **Typ** | Feature |
| **Bug powiązany** | `bug:feedback_prompt_blocks_lookup` |
| **Status** | ✅ Zaimplementowane |

**Zmiany:**
- TTL na feedback requests: auto-discard po 60s (konfigurowalne)
- Env var: `KG_FEEDBACK_TTL_SECONDS=60`
- Funkcja `get_feedback_ttl_ms()` zwraca TTL z env lub domyślnie 10 min

---

### 3.2 default_graph_config ✅ DONE

| Pole | Wartość |
|------|--------|
| **Typ** | Feature |
| **Bug powiązany** | `bug:inconsistent_command_scope` |
| **Status** | ✅ Zaimplementowane |

**Zmiany:**
- Dodane pole `default_graph` do `KgConfig`
- Env var: `KG_DEFAULT_GRAPH=jira`
- Config: `default_graph = "jira"` w `.kg.toml`
- Funkcja `resolve_default_graph(cwd)` do pobierania domyślnego grafu

---

### 3.3 rich_update_response ✅ DONE

| Pole | Wartość |
|------|--------|
| **Typ** | Feature |
| **Importance** | 0.5 |
| **Status** | ✅ Zaimplementowane |

**Zmiany:**
- `node modify` teraz pokazuje diff zmian:
  - `- name: old_value` / `+ name: new_value`
  - `- description: ...` / `+ description: ...`
  - `- importance: X` / `+ importance: Y`
  - `- confidence: ...` / `+ confidence: ...`

---

## Podsumowanie Timeline

| Faza | Items | Estymacja |
|------|-------|----------|
| **Faza 1** | auto_skip, schema_alias, CLI flags parity | 2-4h |
| **Faza 2** | list_all, semantic_errors, KQL fix | 4-8h |
| **Faza 3** | TTL, default_graph, rich_update | 2-3h |

**Total:** ~8-15h (3 sprints)

---

## Akcje Wymagane

- [ ] Zatwierdzenie planu
- [ ] Przydzielenie do fazy 1 (sprint 1)
- [ ] Review implementacji fazy 1 przed fazą 2