# Plan uzupełnienia słowników dla `eyg`

## Cel
Ustalić jednoznaczny i walidowalny format tekstowy dla node/edge, aby:
- mieć unikalne kody 1-literowe dla typów systemowych,
- utrzymać stałą kolejność pól,
- narzucić sortowanie rekordów i pól wielowartościowych,
- ograniczyć długość opisów i format custom atrybutów.

## Założenia i reguły ogólne
- Kody 1-literowe są zarezerwowane wyłącznie dla aplikacji (hardcoded).
- Custom `property`, custom `node type`, custom `relation`:
  - długość `2..10`,
  - bez whitespace,
  - brak kolizji z kodami 1-literowymi.
- Opisy (`D`, `d`) mają maksymalnie 200 znaków.
- Wszystkie timestampy muszą być w formacie ISO-8601 UTC: `YYYY-MM-DDTHH:MM:SSZ`.
- Pola oznaczone `*` są wymagane.
- Lista node'ów jest sortowana rosnąco alfabetycznie po `node id`.
- Pola wielowartościowe (`A`, `F`, `S`, linki `>`) są sortowane alfabetycznie.

## Limity długości i normalizacja

### Limity pól
- `N` (node name): `1..120` znaków
- `D` (node description): `1..200` znaków
- `A` (alias): `1..80` znaków
- `F` (key fact): `1..200` znaków
- `S` (source): `1..200` znaków
- `d` (relation description): `1..200` znaków
- custom value (`- <key> <value>`): `1..200` znaków

### Normalizacja tekstu
- Trim na początku i końcu wartości (wszystkie pola tekstowe).
- Redukcja wielokrotnych spacji wewnętrznych do pojedynczej.
- Deduplikacja `A/F/S` case-insensitive (zachowujemy pierwszą oryginalną formę).
- Sortowanie alfabetyczne case-insensitive; przy remisie sortowanie po surowym stringu.
- `node_id`, `custom_key`, custom `type/relation`: tylko `[a-z0-9_:-]`.

## Zasady semantyczne relacji (decision table)

| Relacja | Kiedy używać | Kiedy nie używać |
|---|---|---|
| `O DEPENDS_ON` | Gdy źródło nie działa/poprawnie nie istnieje bez celu | Nie do luźnych powiązań tematycznych |
| `H HAS` | Gdy źródło zawiera element strukturalny celu | Nie do relacji czasowych ani przyczynowych |
| `D DOCUMENTS` | Gdy źródło dokumentuje/opisuje cel | Nie jako dowód logiczny (`SUPPORTS`) |
| `S SUPPORTS` | Gdy źródło wzmacnia tezę/cel argumentacyjnie | Nie dla czystego opisu (wtedy `DOCUMENTS`) |
| `V CONTRADICTS` | Gdy źródło stoi w sprzeczności merytorycznej z celem | Nie dla różnicy zakresu bez konfliktu logicznego |
| `L RELATED_TO` | Gdy istnieje związek ogólny, ale brak silniejszej relacji | Nie jako domyślny zamiennik dla precyzyjnych relacji |
| `U SUMMARIZES` | Gdy źródło jest syntetycznym skrótem celu | Nie gdy źródło jedynie cytuje część celu |
| `T TRIGGERS` | Gdy źródło inicjuje zdarzenie/proces celu | Nie dla zależności statycznych (`DEPENDS_ON`) |
| `A AFFECTS` | Gdy źródło wpływa na stan/zachowanie celu | Nie gdy wymagane jest ścisłe następstwo czasowe |
| `R READS` | Gdy proces/interfejs odczytuje dane z celu | Nie gdy relacja dotyczy zapisu lub własności |
| `C CREATES` | Gdy źródło tworzy nowy byt/artefakt celu | Nie dla aktualizacji istniejącego obiektu |
| `I AVAILABLE_IN` | Gdy coś jest dostępne w kontekście/platformie celu | Nie do relacji części-całość |
| `G GOVERNS` | Gdy cel definiuje zasady ograniczające źródło | Nie dla luźnych rekomendacji |

## Format rekordu node

### Kolejność pól
1. `@ <TYPE>:<node_id>` *
2. `N <node_name>` *
3. `D <description>` *
4. `A <alias>` (0..n)
5. `F <key_fact>` (0..n)
6. `E <created_at_timestamp>` *
7. `I <valid_from_timestamp>` (0..1)
8. `X <valid_to_timestamp>` (0..1)
9. `C <confidence_1_10>` (0..1)
10. `V <importance_1_6>` *
11. `P <provenance>` *
12. `S <source>` (0..n)
13. `- <custom_key> <value>` (0..n)

### Słownik `V` (importance)
- `1` Ciekawostka
- `2` Trivial
- `3` Minor fact
- `4` Normal (wartość domyślna)
- `5` Important - key information
- `6` Critical

`V` jest polem obowiązkowym i będzie używane do sortowania wyników.

### Słownik `P` (provenance)
- `U` User input
- `D` Documentation scan
- `A` AI deduction

### Relacje (sekcja edge wewnątrz node)
- `> <REL> <target_node_id>`
- `d <relation_description>`
- `i <relation_valid_from_timestamp>`
- `x <relation_valid_to_timestamp>`
- `- <custom_key> <value>`

`d/i/x/-` dotyczą ostatniej zadeklarowanej relacji `>`.

## Słownik typów node (unikalne kody)

### Typy systemowe
- `F` Feature
- `K` Concept
- `I` Interface
- `P` Process
- `D` Datastore
- `A` Attribute
- `Y` Entity/Object
- `N` Note
- `R` Rule
- `C` Convention
- `B` Bug/Issue/Problem
- `Z` Decision
- `O` OpenQuestion (zamiast `?`)
- `Q` Claim
- `W` Insight
- `M` Reference
- `T` Term
- `S` Status
- `L` Doubt (zamiast konfliktowego `D`)

## Słownik relacji edge (unikalne kody)

### Typy systemowe
- `D` DOCUMENTS
- `H` HAS
- `T` TRIGGERS
- `A` AFFECTS
- `R` READS
- `G` GOVERNS
- `O` DEPENDS_ON
- `I` AVAILABLE_IN
- `S` SUPPORTS
- `U` SUMMARIZES (zamiast duplikatu `S`)
- `L` RELATED_TO (zamiast duplikatu `R`)
- `V` CONTRADICTS (zamiast duplikatu `C`; bez kolizji z `X` jako valid_to)
- `C` CREATES

## Plan wdrożenia
1. Zatwierdzić finalne słowniki kodów node i edge.
2. Spisać formalną specyfikację parsera (gramatyka linii i kontekst relacji).
3. Dodać walidator formatu i słowników.
4. Dodać normalizer wymuszający kolejność pól i sortowanie.
5. Przygotować migrator starych kodów do nowych (`? -> O`, `Doubt:D -> L`, `CONTRADICTS:X -> V`, duplikaty edge).
6. Uruchomić dry-run migracji i raport różnic.
7. Wykonać pełną migrację i blokadę zapisów niespełniających specyfikacji.

## Migracja silnika i formatów grafu (`*.json` -> `*.kg`)

### Nowy model danych
- Nowy silnik działa natywnie na plikach grafu `*.kg`.
- `*.kg` jest formatem docelowym dla odczytu i zapisu.

### Legacy engine
- Stary model/silnik JSON przenieść do osobnego modułu/pakietu w kodzie i oznaczyć jako `legacy`.
- Flaga `--legacy` wymusza odczyt grafu `*.json` przez stary silnik (bez automigracji).

### Automigracja przy odczycie grafu
- Przy próbie otwarcia grafu `<name>`:
  1. jeśli istnieje `<name>.kg` -> użyj nowego silnika,
  2. jeśli `<name>.kg` nie istnieje, ale istnieje `<name>.json` -> wykonaj migrację do `<name>.kg`,
  3. po migracji zachowaj oryginalny `<name>.json` (bez kasowania),
  4. dalsza praca odbywa się na `<name>.kg`.
- Migracja musi być idempotentna i raportować wynik (liczba node/edge, ostrzeżenia).

### Inteligentna migracja typów i relacji
- Migracja mapuje historyczne nazwy typów node na słownik docelowy (`F/K/I/P/D/A/Y/N/R/C/B/Z/O/Q/W/M/T/S/L`) także dla wariantów:
  - różna wielkość liter (`feature`, `Feature`, `FEATURE`),
  - separatorów (`data_store`, `data-store`, `datastore`),
  - liczby pojedynczej/mnogiej i skrótów (`bugs`, `iface`, `ref`, `question`).
- Nierozpoznane typy, których nie da się dopasować automatycznie, są zapisywane jako custom node type zgodnie z zasadami custom (`2..10`, bez whitespace), a oryginalna wartość jest zachowywana w raporcie migracji.
- Analogicznie dla atrybutów: migrator najpierw próbuje mapowania do pól standardowych; jeśli brak dopasowania, zapisuje dane jako custom attribute (`- <custom_key> <value>`).
- W nowym modelu przechowujemy wyłącznie relacje wychodzące (`->`).
- Wszystkie relacje przychodzące (`<-`) ze starego modelu muszą zostać przeniesione do odpowiednich node'ów jako relacje wychodzące (`->`) po stronie źródłowej.
- Dla relacji niesymetrycznych migrator stosuje mapowanie kierunku zgodne ze słownikiem relacji i raportuje każdy przypadek transformacji.

## Access log i feedback log (`*.kglog`)

### Założenie
- W starym modelu feedback był zapisywany w głównym pliku JSON.
- W nowym modelu zdarzenia dostępu i feedback są zapisywane w osobnym pliku logu `*.kglog`.

### Format linii logu
- Hit (zwrócony node):
  - `<timestamp> <user_short_uid> H <node_id>`
- Feedback:
  - `<timestamp> <user_short_uid> F <node_id> <feedback>`

### Reguły logowania
- `timestamp` w ISO-8601 UTC (`YYYY-MM-DDTHH:MM:SSZ`).
- `user_short_uid` to krótki identyfikator użytkownika.
- Log ma rozszerzenie `*.kglog` i jest append-only.

### `user_short_uid` (instalacja/config)
- UID generowany losowo przy instalacji aplikacji.
- UID zapisywany w konfiguracji `kg`.
- Jeśli UID nie istnieje w configu, aplikacja generuje go przy pierwszym uruchomieniu i utrwala.

## Indeks szybkiego dostępu (`*.kgindex`)

### Cel
- Przyspieszyć wyszukiwanie `node_id` i skakanie do node'ów w pliku `*.kg`.

### Tworzenie indeksu
- Podczas skanowania grafu `<name>.kg` silnik generuje plik `<name>.kgindex`.
- Każda linia indeksu ma format:
  - `<node_id> <graph_line_number>`
- `graph_line_number` oznacza numer linii w pliku `*.kg`, od której zaczyna się rekord node (`@ <TYPE>:<node_id>`).

### Zasady użycia
- Wyszukiwanie po `node_id` w pierwszej kolejności korzysta z `*.kgindex`.
- Jeśli indeksu brak, silnik wykonuje pełny skan i po nim tworzy indeks.

### Inwalidacja indeksu
- Każda modyfikacja grafu `*.kg` unieważnia odpowiadający plik `*.kgindex`.
- Inwalidacja może być realizowana przez usunięcie pliku indeksu lub oznaczenie go jako nieaktualny.
- Po inwalidacji indeks jest odbudowywany przy kolejnym skanowaniu/otwarciu grafu.

## Checklista weryfikacyjna

### A. Spójność słowników
- [ ] Brak duplikatów kodów 1-literowych w node.
- [ ] Brak duplikatów kodów 1-literowych w edge.
- [ ] Każdy kod ma jednoznaczną nazwę i opis semantyki.
- [ ] Wszystkie stare kody mają mapowanie migracyjne.

### B. Walidacja formatu
- [ ] Każdy node ma wymagane pola: `@`, `N`, `D`, `E`, `P`.
- [ ] `@` ma poprawny format `<TYPE>:<node_id>`.
- [ ] Wszystkie `D` i `d` maja <= 200 znaków.
- [ ] `C` jest w zakresie `1..10`.
- [ ] `V` jest obecne i jest w zakresie `1..6`.
- [ ] Wszystkie timestampy (`E`, `I`, `X`, `i`, `x`) są parse'owalne i zgodne z ISO-8601 UTC.
- [ ] Każda linia `d/i/x/-` relacji ma poprzedzające `>`.
- [ ] Wszystkie wartości tekstowe przechodzą normalizację (trim/spaces/dedupe).

### C. Sortowanie i deterministyczność
- [ ] Node'y są posortowane po `node_id`.
- [ ] Pola w node są zawsze w tej samej kolejności.
- [ ] `A/F/S` są posortowane alfabetycznie.
- [ ] Relacje `>` są posortowane deterministycznie (np. po `REL`, potem `target_node_id`).
- [ ] Custom properties `-` są posortowane alfabetycznie po `custom_key`.

### D. Reguły custom
- [ ] Wszystkie custom klucze/typy/relacje mają długość `2..10`.
- [ ] Żaden custom klucz/typ/relacja nie zawiera whitespace.
- [ ] Żaden custom klucz/typ/relacja nie koliduje z kodami systemowymi.

### E. Jakość migracji
- [ ] Raport migracji zawiera liczbę zmian per kod.
- [ ] Brak utraty danych przy mapowaniu starych kodów.
- [ ] Przypadki niejednoznaczne są wypisane do manual review.
- [ ] Migrator mapuje historyczne warianty nazw typów (case, separator, skróty).
- [ ] Typy niemapowalne automatycznie trafiają jako custom node type (zgodnie z regułami custom).
- [ ] Atrybuty niemapowalne automatycznie trafiają jako custom attribute (`-`).
- [ ] Relacje przychodzące ze starego modelu są poprawnie przeniesione do relacji wychodzących w nowym modelu.
- [ ] Każdy przypadek transformacji kierunku relacji jest raportowany.

### F. Kompatybilność wsteczna
- [ ] Parser akceptuje historyczne rekordy i mapuje je przez warstwę migracji.
- [ ] Po migracji eksport -> import jest idempotentny (brak zmian przy drugim przebiegu).
- [ ] Istnieją snapshot testy dla starych i nowych wariantów danych.
- [ ] Legacy engine jest wydzielony i oznaczony jako `legacy`.
- [ ] Flaga `--legacy` wymusza odczyt `*.json` bez automigracji.
- [ ] Automigracja uruchamia się tylko gdy brak `*.kg` i istnieje `*.json`.
- [ ] Po automigracji plik `*.json` pozostaje nienaruszony.

### H. Logowanie zdarzeń
- [ ] Dla każdego hitu zapisywany jest rekord `H` w `*.kglog`.
- [ ] Dla każdego feedbacku zapisywany jest rekord `F` w `*.kglog`.
- [ ] Logi nie są już zapisywane w głównym pliku grafu.
- [ ] Format linii logu jest zgodny ze specyfikacją timestamp/uid.
- [ ] `user_short_uid` jest generowany i utrwalany w configu `kg`.

### I. Indeksowanie `*.kgindex`
- [ ] Dla każdego zeskanowanego grafu `*.kg` tworzony jest indeks `*.kgindex`.
- [ ] Każdy wpis indeksu ma format `<node_id> <graph_line_number>`.
- [ ] `graph_line_number` wskazuje linię startu rekordu `@` w `*.kg`.
- [ ] Przy modyfikacji `*.kg` indeks jest inwalidowany.
- [ ] Po inwalidacji indeks jest poprawnie odbudowywany przy następnym skanowaniu.

### G. Metryki jakości po migracji
- [ ] Odsetek relacji `L RELATED_TO` nie przekracza uzgodnionego progu (np. 15%).
- [ ] Liczba rekordów odrzuconych przez walidator = 0 po poprawkach.
- [ ] Raport top 20 najczęstszych błędów wejścia jest publikowany po dry-run.

## Krytyczna ocena planu (po poprawkach)

### Mocne strony
- Plan porządkuje format i eliminuje konfliktujące kody.
- Daje deterministyczny zapis (stała kolejność + sortowanie), co ułatwia diffy i review.
- Dodaje ścieżkę migracji zamiast "big bang".
- Domyka brakujące elementy operacyjne: timezone, limity długości, normalizacja i kompatybilność wsteczna.
- Dodaje policy layer dla relacji, co redukuje niespójność semantyczną danych.

### Pozostałe ryzyka
- `D` jako `Datastore` i `D` jako relacja `DOCUMENTS` jest poprawne (inne słowniki), ale wymaga dobrej dokumentacji dla nowych członków zespołu.
- Relacja `L RELATED_TO` może być nadużywana, jeśli review semantyczne nie będzie egzekwowane.
- Custom typy i relacje zwiększają elastyczność, ale wymagają cyklicznego audytu jakości.

### Rekomendowane poprawki przed implementacją
1. Dodać linter CI blokujący merge przy naruszeniu reguł formatu.
2. Dodać kwartalny audyt słownika custom typów i relacji.
3. Dodać dashboard metryk jakości (odrzuty walidacji, udział `RELATED_TO`, liczba remapów).

### Ocena końcowa
Plan jest gotowy do implementacji produkcyjnej i po domknięciu CI/audytów można go traktować jako 10/10.

## Kryteria akceptacji
- Brak duplikatów kodów systemowych w obu słownikach.
- 100% rekordów przechodzi walidator po migracji.
- 100% rekordów spełnia reguły kolejności i sortowania.
- Wszystkie rekordy niespełniające reguł trafiają do raportu z jasnym powodem odrzutu.
- Każdy node ma obowiązkowe `V` (`importance`) i wartości `1..6` zgodne ze słownikiem.
- Nowy silnik domyślnie używa `*.kg`, a tryb `--legacy` działa wyłącznie na `*.json`.
- Automigracja `*.json -> *.kg` działa tylko przy braku `*.kg` i nie usuwa pliku źródłowego `*.json`.
- Access/feedback są zapisywane w dedykowanym `*.kglog` zgodnie z formatem linii.
- Dla grafu `*.kg` dostępny jest aktualny indeks `*.kgindex` z mapowaniem `node_id -> line_number`.
- Każda modyfikacja grafu unieważnia indeks i wymusza jego odbudowę przy kolejnym skanowaniu.
- Migracja poprawnie mapuje historyczne typy node i przenosi relacje `(<-)` do modelu relacji wychodzących (`->`).
- Typy i atrybuty niemapowalne automatycznie nie są tracone: trafiają odpowiednio do custom node type i custom attribute (`-`).
