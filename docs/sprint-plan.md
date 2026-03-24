# kg roadmap: plan sprintow (draft)

Zasady:
- `kg` pozostaje deterministycznym silnikiem grafu (bez wbudowanego dostepu do providerow LLM).
- Integracje/ekstrakcja z dokumentacji odbywa sie przez skille/prompty/klientow (MCP, OpenCode, Claude Desktop, Cursor), nie przez `kg`.
- Kazdy sprint konczy sie zestawem testow + `kg check`/`kg audit` na przykladowych grafach.

## Sprint 0: UX bootstrap (init/prompt/skills)
Status: done
Cel: ulatwic start i standaryzowac prace z `kg` i `kg-mcp`.

Zakres:
- Komenda typu `kg init` / `kg prompt` drukujaca na stdout:
  - instrukcje dla pracy przez CLI
  - instrukcje dla pracy przez MCP (w tym format wielo-komendowy, jesli jest)
  - wariant "doc -> graph" (skill/prompt do skopiowania)
- Aktualizacja skill w repo (bazujac na `skills/kg-builder/SKILL.md`) o twardy protokol: plan -> wykonanie -> walidacja.

Wyjscie (DoD):
- Uzytkownik potrafi w 5 min uruchomic workflow "docs -> graph" bez modyfikacji kodu klienckiego.

## Sprint 1: Warstwa storage (abstrakcja + przygotowanie pod multi-backend)
Status: done
Cel: odseparowac logike grafu od formatu przechowywania.

Zakres:
- Wydzielenie interfejsu backendu (trait) dla operacji: read graph, write graph, list graphs.
- Backend JSON jako referencyjny (kompatybilny wstecz).
- Podwaliny pod migracje (wersjonowanie formatu, pole `schema_version`).

Wyjscie (DoD):
- Testy integracyjne: ten sam zestaw operacji dziala na backendzie JSON.
- Brak zmian w deterministycznym output (poza ewentualnymi naglowkami wersji).

## Sprint 2: Backend DB (wybor + multi-backend v1)
Status: done (redb backend + import/export)
Cel: skalowanie, szybsze zapytania, stabilniejsze zapisy; bez przywiazania core do jednego formatu.

Wybor backendu: redb (lokalnie, bez zewnetrznych serwisow). Decyzja opisana w docs/decision-backend.md.

Kandydaci (local-first):
- SQLite: relacyjny storage + transakcje + latwe migracje + jedna biblioteka + wbudowane FTS5 (BM25).
- LMDB/RocksDB/sled: szybki KV, dobra wydajnosc na duzych danych; zwykle brak natywnego full-text (trzeba osobnego indeksu).
- DuckDB: analityczne (kolumnowe) query, spoko pod raporty/diffy; mniej naturalne do czestych malych aktualizacji.

Rekomendacja na start:
- SQLite jako pierwszy backend DB, bo daje "bateries included" (transakcje, migracje, FTS/BM25) i jest najlatwiejszy do dystrybucji.
- Ale: robimy to po Sprint 1 (trait) i z wyraznym punktem decyzyjnym (gdyby benchmarki/UX wyszly slabo, zamiana backendu ma byc realna).

Zakres:
- Krotki "decision doc" (MD) z kryteriami i wynikiem wyboru backendu (testy + benchmark).
- Nowy backend DB (docelowo SQLite): `~/.kg/graphs/<name>.db` lub `.kg/graphs/<name>.db`.
- Import/eksport: JSON <-> DB.
- Migracje schematu DB.

Wyjscie (DoD):
- Ten sam graf mozna otworzyc z JSON i zapisac do DB bez utraty informacji.
- Benchmark "node find"/"node get" dla srednich grafow (np. 10k wezlow) z wyraznym zyskiem.

## Sprint 3: Notatki/observations jako 1st-class
Status: done
Cel: rozdzielic "fakty o wezle" od "notatek w czasie".

Zakres:
- Model "note/observation" z metadanymi (autor, timestamp, tags, zrodlo/provenance).
- Komendy do dodawania/przegladania notatek oraz eksportu do HTML.

Wyjscie (DoD):
- Da sie dopisac notatke do dowolnego node bez modyfikacji jego podstawowej definicji.
- `kg quality` uwzglednia nowe dane (np. missing-notes opcjonalnie).

## Sprint 4: Full-text + BM25 (bez LLM) + podstawy semantyki "hardcoded"
Status: done
Cel: wyszukiwanie po tresci i ranking wynikow.

BM25 (co to jest):
- Standardowa funkcja rankingowa w wyszukiwaniu pelnotekstowym (IR).
- Dziala na czestosci slow (TF) + rzadkosci w korpusie (IDF) + normalizacji dlugosci dokumentu.
- Daje lepsze wyniki niz "contains" i zwykly TF-IDF w typowych tekstach.

Zakres:
- Indeks full-text:
  - Jesli backend SQLite: FTS5.
  - Jesli backend KV/inny: osobny indeks (np. Tantivy) + deterministyczna synchronizacja.
- `node find` rozszerzone o tryby: exact, prefix, fuzzy, bm25.
- "Semantyka bez LLM" (hardcoded):
  - normalizacja (lowercase, stemming/lemmatyzacja jesli latwo)
  - slownik synonimow/aliasow (konfig w repo lub per-graph)
  - ekspansja zapytania po relacjach (np. szukaj w aliasach + facts + notatkach)

Wyjscie (DoD):
- Powtarzalny ranking wynikow dla tego samego korpusu.
- Dokumentacja jak stroic slownik synonimow bez modeli.

## Sprint 5: TUI do wyszukiwania i nawigacji po grafie (async)
Status: done
Cel: szybki, manualny "graph browser" dla uzytkownika.

Zakres:
- TUI (np. ratatui) z:
  - paskiem wyszukiwania (BM25)
  - lista wynikow + podglad node (description/facts/notes)
  - nawigacja po sasiedztwie (in/out edges)
  - kopiowanie ID do schowka (opcjonalnie)
  - architektura asynchroniczna:
    - debounce zapytan (np. 150-250ms)
    - anulowanie poprzedniego wyszukiwania po zmianie query
    - inkrementalne wyniki (pierwsze N szybko, reszta w tle)
    - render loop niezalezny od I/O (brak blokowania UI)

Wyjscie (DoD):
- Uzytkownik potrafi znalezc node, obejrzec kontekst relacji i skopiowac ID bez odpalania edytora.
- UI nie "przycina" podczas wyszukiwania (I/O w tle, brak dlugich operacji na watku render).

## Sprint 6: Temporal (wersjonowanie w czasie)
Status: done
Cel: odpowiedziec na pytania "co bylo prawda kiedy".

Zakres (wariant MVP):
- Event log zmian (append-only) + materializacja aktualnego stanu.
- Pola temporalne dla nodes/edges/notes (valid_from, valid_to) albo "tx_time" (kiedy zapisano).
- Komendy: `as-of <time>`, diff w czasie (np. miedzy timestampami).

Wyjscie (DoD):
- Da sie odtworzyc stan grafu "na dzien X".
- Da sie pokazac zmiany od X do Y.

## Sprint 7: Import/Merge
Status: done (CSV + Markdown import, deterministic merge + conflict report)
Cel: zasilanie grafu z istniejacych zrodel.

Zakres:
- Import z Markdown/YAML frontmatter (notes-as-nodes) i/lub CSV.
- Merge dwoch grafow (strategie: prefer-new, prefer-old, manual map).
- Raport konfliktow + propozycje deduplikacji.

Wyjscie (DoD):
- Da sie zrobic merge bez utraty danych, a konflikty sa jawne i deterministyczne.

## Sprint 8: Rename/Migrate IDs + diff grafow
Status: done
Cel: refactoring grafu bez rozbijania referencji.

Zakres:
- Komenda rename node id + aktualizacja wszystkich krawedzi/odnosnikow.
- "migrate ids" (reguly masowej zmiany, np. prefixy).
- `kg diff` (nodes/edges/notes) z czytelnym outputem.

Wyjscie (DoD):
- Operacje sa atomowe (w szczegolnosci na SQLite) i odwracalne (przez event log / temporal).

## Sprint 9: KQL (kg query language) - design + MVP
Status: done (MVP + docs/kql.md)
Cel: zapytania wzorcowe i filtry bez wchodzenia w pelny Cypher.

Plan pracy:
- Najpierw "design sprint": gramatyka, semantyka, typy wynikow, bledy, deterministyczny output.
- MVP: selekcja wezlow + filtry + relacje 1-hop/2-hop + ograniczenia typu.

Wyjscie (DoD):
- Spec gramatyki (MD) + parser + testy.
- KQL dziala identycznie na JSON i SQLite.

## Sprint 10: Wizualizacja
Status: done (export HTML + DOT/Mermaid, filtry/legendy/subgraph)
Cel: szybkie "zobacz graf" dla ludzi i do PR/artefaktow.

Zakres:
- Rozszerzenie HTML export o widoki: subgraph around node, filtry po typach, legenda.
- Dodatkowe exporty: DOT/Graphviz lub Mermaid.

Wyjscie (DoD):
- Jednym poleceniem da sie wygenerowac artefakt do podzielenia sie (HTML + opcjonalnie Mermaid/DOT).

## Uwagi o "semantic search" bez LLM
Da sie, ale to nie bedzie embedding-based "rozumienie"; to bedzie lepsze IR + heurystyki:
- BM25/FTS + fuzzy + stemming + slownik synonimow/aliasow + ekspansja po relacjach
- (opcjonalnie pozniej) lokalne embeddingi jako dodatkowy indeks, ale nadal poza `kg` core (sidecar)

## Sprint 11: Stabilizacja interfejsu (CLI/MCP) + output maszynowy
Status: done
Cel: stabilny, skryptowalny interfejs bez parsowania tekstu.

Zakres:
- `--json` dla kluczowych komend (find/get/list/diff/quality/kql).
- Stabilne schemy outputu + kody wyjscia.
- Spójne bledy (deterministyczne, bez "pretty randomness").

Wyjscie (DoD):
- Skrypty i integracje nie wymagaja parsowania tekstu.

## Sprint 12: Schema/constraints + walidacja na zapisie
Status: done
Cel: wymuszanie jakosci danych jak w DB.

Zakres:
- Plik reguł (np. `.kg.schema.toml`) dla: typow, relacji, wymaganych pol, unikalnosci.
- Walidacja przy mutacjach (node add/modify, edge add, importy).
- Raport naruszen z jasnymi wskazowkami.

Wyjscie (DoD):
- Da sie wymusic spojnosc i kompletność danych przy zapisie.

## Sprint 13: KQL v1 (traversale + agregacje)
Status: done (sort + limit + neighbors + path + count by)
Cel: realny "Cypher-lite" dla lokalnych grafow.

Zakres:
- OR/AND, nawiasy, quoting, sort/limit.
- N-hop: `from=<id> hops=2` / `neighbors` / `path`.
- Agregacje: count by type/relation/tag.

Wyjscie (DoD):
- Uzyteczne zapytania wzorcowe bez wychodzenia do DB.

## Sprint 14: Indeksy i wydajnosc (redb + incremental)
Status: done (infrastruktura indeksow + zapis przy save + ladowanie/uzycie w zapytaniach)
Cel: stabilna wydajnosc na srednich i duzych grafach.

Zakres:
- Trwale indeksy w redb (np. inverted index dla BM25).
- Incremental update indeksow po mutacjach/importach.
- Integracja: indeks BM25 ladowany przy wyszukiwaniu i uzywany zamiast obliczen in-memory.
- Benchmarki: 10k/100k nodes (find/get/kql).

Wyjscie (DoD):
- Przewidywalne czasy odpowiedzi przy rosnacym grafie.

## Sprint 15: Vector sidecar (lokalnie, deterministycznie)
Status: done (vectors import + node find --mode vector + brute-force cosine)
Cel: semantyczne wyszukiwanie bez providerow w `kg`.

Zakres:
- Import wektorow: `kg <graph> vectors import --input vectors.jsonl`.
- `node find --mode vector` (MVP: brute-force cosine).
- Filtry po type/tag + deterministyczne ustawienia.

Wyjscie (DoD):
- Lokalny, deterministyczny semantic search.

## Sprint 16: Interop/publishing
Status: done (GraphML + MD folder export)
Cel: latwe wejscie/wyjscie z ekosystemu.

Zakres:
- Eksport: GraphML + JSON-LD/RDF (minimalny mapping).
- Eksport do "PKM": folder MD (nodes/notes) z linkami/backlinkami.

Wyjscie (DoD):
- Jednym poleceniem da sie wygenerowac kompatybilne artefakty.

## Sprint 17: Git-friendly storage + conflict resolution
Status: done (split graph to separate files + MANIFEST)
Cel: sensowna praca zespolowa przez git.

Zakres:
- Opcja "split graph" (nodes/edges/notes jako osobne pliki lub deterministyczne exporty).
- Narzedzia do merge/resolve z raportem konfliktow.

Wyjscie (DoD):
- Praca zespolowa bez bolu konfliktow.

## Sprint 18: Release + dystrybucja (multi-OS)
Status: done
Cel: latwa instalacja binarek bez cargo, spojne release artefakty.

Zakres:
- Rozszerzenie GitHub Actions release o build na Linux/MacOS/Windows (artefakty per target).
- Checksums dla wszystkich artefaktow + jasne nazewnictwo plikow.
- Aktualizacja `install.sh`:
  - wybor poprawnego artefaktu per OS/arch
  - fallback na `cargo install` (jesli brak binarki)
- (Opcjonalnie) podpisywanie artefaktow.

Wyjscie (DoD):
- Instalacja dziala na 3 OS bez budowania ze zrodel.
- Release ma komplet artefaktow + `checksums.txt`.

## Sprint 19: FUSE mount (filesystem jako interfejs KG)
Status: done
Cel: dostep do grafu przez standardowe narzedzia Unix (ls, grep, cat).

Co to FUSE:
- Filesystem in Userspace - montowanie grafu jako folderu w systemie plikow.
- Pozwala uzywac `grep`, `find`, `ls` na danych semantycznych.
- Integracja z edytorami (Obsidian, VS Code) bez posrednictwa CLI.

Przykladowe uzycie:
```sh
kg fridge mount /tmp/kg-fuse
ls /tmp/kg-fuse/concepts/
grep -r "refrigerator" /tmp/kg-fuse/
cat /tmp/kg-fuse/nodes/concept:refrigerator.json
```

Zakres:
- Komenda `kg mount <graph> <mountpoint>`.
- Virtual filesystem: nodes jako pliki, relacje jako linki.
- Implementacja: biblioteka FUSE (np. `fuser` crate) + handler dla operacji readdir/lookup/read.
- Unmount: `kg unmount <mountpoint>` lub `fusermount -u`.

Wyjscie (DoD):
- Graf dostepny jako folder, wszystkie operacje FS przekladaja sie na zapytania KG.
- Dziala na Linuksie; MacOS FUSE jako opcjonalny target.

## Sprint 20: Hybrid search (BM25 + semantic)
Status: done
Cel: pelny hybrid search laczacy precyzje BM25 z rozumieniem semantycznym.

Co to hybrid search:
- BM25: dokładne dopasowanie slow ("lodowka" = "lodowka")
- Semantic: dopasowanie znaczenia ("lodowka" ≈ "chlodziarka", "fridge")
- Hybrid: laczy oba podejscia, daje lepsze wyniki niz kazde z osobna

Stan obecny (Sprint 15):
- Vector sidecar istnieje, ale wymaga recznego importu wektorow.
- Brak mechanizmu automatycznej generacji embeddings.

Zakres:
- Opcjonalna integracja z lokalnym embedding providerem (np. Ollama via HTTP).
- Komenda `kg <graph> vectors embed --text "..."` (generuje wektory).
- Tryb `node find --mode hybrid` (RRF fusion: laczenie wynikow BM25 + semantic).
- Konfiguracja: `embedding_provider = "ollama"` w `.kg.toml`.

Wyjscie (DoD):
- Jeden tryb `hybrid` daje lepsze wyniki niz `bm25` lub `vector` osobno.
- Provider plikowany (ollama, openai, local), kg-core bez zaleznosci.

## Sprint 21: SDK bindings (TypeScript + Python)
Status: done
Cel: latwa integracja kg z aplikacjami poza CLI.

Co to SDK:
- Biblioteki dla innych jezykow niz Rust.
- Parity z nanograph (TypeScript SDK) i innymi KG toolami.

Przykladowe uzycie TypeScript:
```typescript
import { KgGraph } from '@kg-db/core';

const graph = new KgGraph('fridge');
const nodes = await graph.find({ query: 'lodowka', mode: 'hybrid' });
const node = await graph.get('concept:refrigerator');
```

Przykladowe uzycie Python:
```python
from kg import Graph

graph = Graph('fridge')
nodes = graph.find(query='lodowka', mode='hybrid')
```

Zakres:
- TypeScript SDK: package `@kg-db/core` lub podobna nazwa na npm.
- Python SDK: package `kg-db` na PyPI.
- Wrapper na CLI lub binding do Rust library (np. przez PyO3).
- Dokumentacja: README + API reference + migration guide.

Wyjscie (DoD):
- Przykladowa aplikacja uzywajaca kazdego SDK.
- SDK publikowane na npm/PyPI z wersjami.

## Sprint 22: Stabilizacja i hardening (DX + CI + perf)
Status: done
Cel: podniesc jakosc, przewidywalnosc i bezpieczenstwo zmian.

Zakres:
- CI: macierz OS (linux/macos/windows) + `cargo fmt --check` + `cargo clippy`.
- Regression guardrails: bazowe wyniki benchmarkow (np. zapisywane jako artefakt) + progi ostrzegawcze.
- Testy integracyjne dla krytycznych komend (create/import/find/diff/check/audit) na JSON i redb.
- Stabilizacja outputu `--json` (schemy + wersjonowanie) i kodow wyjscia.

Wyjscie (DoD):
- PR nie przechodzi bez fmt/clippy/test na 3 OS.
- Performance regresje sa widoczne (benchmark artefakty) i latwe do porownania.

## Sprint UX-1: Opcjonalny Feedback (0.5 dnia)
Status: done
Cel: Usunac friction przy prostych lookupach.

Zakres:
- Dodac parametr `skip_feedback?: bool` do `kg_node_find`.
- Gdy `skip_feedback=true`: nie zwracac `requires_feedback` w structured_content.
- Przyklad: agent robi `kg_node_find` -> sprawdza czy UID pasuje -> kontynuuje bez oddzielnego `kg_feedback`.

Wyjscie (DoD):
- Lookup bez feedbacku dziala, feedback nadal dziala normalnie.

## Sprint UX-2: kg_edge_add_batch (0.5 dnia)
Status: done
Cel: Symetria z `kg_node_add_batch`.

Zakres:
- Nowy tool `kg_edge_add_batch` z:
  - `graph: String`
  - `edges: Vec<{ source_id, relation, target_id, detail? }>`
  - `mode?: "atomic" | "best_effort"`
- Walidacja istnienia wezlow przed zapisem.
- Atomic mode: wszystko albo nic.

Wyjscie (DoD):
- 6 krawedzi = 1 tool call zamiast 6.

## Sprint UX-3: kg_schema Tool (1 dzien)
Status: done
Cel: Schema widoczna w tool description / osobny tool.

Zakres:
- Opisac valid types i relations w `kg_node_add` / `kg_edge_add` descriptions.
- Dodac `kg_schema` tool zwracajacy: { valid_types, valid_relations, type_to_prefix, edge_rules }.
- Tool przydatny dla programistycznego dostepu do schematu.

Wyjscie (DoD):
- Agent zna konwencje bez `kg_stats`.

## Sprint UX-4: Walidacja na Zapisie (1 dzien)
Status: done
Cel: Bledy wychodza przy `add`/`modify`, nie przy `check`.

Zakres:
- Rozszerzyc `kg_node_add` / `kg_edge_add` o pre-write validation:
  - Sprawdzenie `node_type` vs `VALID_TYPES`
  - Sprawdzenie `id` vs `TYPE_TO_PREFIX`
  - Sprawdzenie `relation` vs `VALID_RELATIONS`
  - Sprawdzenie source/target type constraints (EDGE_TYPE_RULES)
- Blad zwraca jasna wiadomosc, np.:
  ```
  Invalid node_type 'analysis'. Valid types: Concept, Process, DataStore, Interface, Rule, Feature, Decision, Convention, Note, Bug
  ```

Wyjscie (DoD):
- `kg_node_add` z blednym typem = niezwloczny blad z jasna wiadomoscia.

## Sprint UX-5: Redukcja Tool Noise (0.5 dnia)
Status: done
Cel: Mniej tooli w liscie, latwiejszy wybor.

Zakres:
- Oznaczyc jako deprecated (z "Prefer kg script") dla:
  - `kg_check`, `kg_audit`, `kg_quality`, `kg_access_log`, `kg_access_stats`, `kg_export_html`
- Core tools (bez deprecation): `kg`, `kg_node_find`, `kg_node_get`, `kg_node_add`, `kg_node_add_batch`, `kg_node_modify`, `kg_edge_add`, `kg_edge_add_batch`, `kg_stats`, `kg_feedback`, `kg_feedback_batch`, `kg_schema`, `kg_create_graph`

Wyjscie (DoD):
- 13 core tools vs 20+ obecnie.
