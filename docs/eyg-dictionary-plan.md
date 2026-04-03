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
- Pola oznaczone `*` są wymagane.
- Lista node'ów jest sortowana rosnąco alfabetycznie po `node id`.
- Pola wielowartościowe (`A`, `F`, `S`, linki `>`) są sortowane alfabetycznie.

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
10. `P <provenance>` *
11. `S <source>` (0..n)
12. `- <custom_key> <value>` (0..n)

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
- `X` CONTRADICTS (zamiast duplikatu `C`)
- `C` CREATES

## Plan wdrożenia
1. Zatwierdzić finalne słowniki kodów node i edge.
2. Spisać formalną specyfikację parsera (gramatyka linii i kontekst relacji).
3. Dodać walidator formatu i słowników.
4. Dodać normalizer wymuszający kolejność pól i sortowanie.
5. Przygotować migrator starych kodów do nowych (`? -> O`, `Doubt:D -> L`, duplikaty edge).
6. Uruchomić dry-run migracji i raport różnic.
7. Wykonać pełną migrację i blokadę zapisów niespełniających specyfikacji.

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
- [ ] Wszystkie timestampy (`E`, `I`, `X`, `i`, `x`) są parse'owalne.
- [ ] Każda linia `d/i/x/-` relacji ma poprzedzające `>`.

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

## Krytyczna ocena planu

### Mocne strony
- Plan porządkuje format i eliminuje konfliktujące kody.
- Daje deterministyczny zapis (stała kolejność + sortowanie), co ułatwia diffy i review.
- Dodaje ścieżkę migracji zamiast "big bang".

### Ryzyka i luki
- Kod `X` jest używany jako `valid_to` dla node i relacji oraz jako relacja `CONTRADICTS`; to jest legalne, ale może mylić ludzi i parser debug logs.
- `D` jako `Datastore` i `D` jako relacja `DOCUMENTS` jest poprawne (inne słowniki), ale zwiększa koszt poznawczy.
- Brakuje formalnej decyzji o strefie czasowej i formacie timestampu (np. ISO-8601 UTC), co grozi niespójnymi danymi.
- Brakuje maksymalnych długości dla `N`, `A`, `F`, `S` i wartości custom, co może pogorszyć jakość danych.
- Brakuje zasad deduplikacji semantycznej (np. aliasy różniące się tylko wielkością liter).

### Rekomendowane poprawki przed implementacją
1. Ustalić jeden format czasu: ISO-8601 w UTC (`YYYY-MM-DDTHH:MM:SSZ`).
2. Rozważyć zmianę kodu relacji `CONTRADICTS` z `X` na mniej kolizyjny (np. `V`).
3. Dodać limity długości dla `N/A/F/S` i wartości custom.
4. Zdefiniować normalizację tekstu (trim, wielkość liter, redukcja duplikatów).
5. Dodać zestaw testów kontraktowych parsera z przypadkami błędnymi i granicznymi.

## Kryteria akceptacji
- Brak duplikatów kodów systemowych w obu słownikach.
- 100% rekordów przechodzi walidator po migracji.
- 100% rekordów spełnia reguły kolejności i sortowania.
- Wszystkie rekordy niespełniające reguł trafiają do raportu z jasnym powodem odrzutu.
