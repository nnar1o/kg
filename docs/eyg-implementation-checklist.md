# Plan implementacji `eyg` (checklista)

Dokument roboczy do wdrozenia zmian opisanych w glownym planie:
- [Glowny dokument: `docs/eyg-dictionary-plan.md`](./eyg-dictionary-plan.md)

## 0) Przygotowanie
- [ ] Potwierdzic scope MVP (tylko parser/walidacja/migracja) vs rozszerzenia (telemetria, dodatkowe raporty).
- [ ] Zablokowac format wejscia dla nowego runtime: preferowany `*.kg`, fallback migracyjny z `*.json`.
- [ ] Uzgodnic i spisac kody bledow walidacji (stabilne identyfikatory dla CLI i testow).

## 1) Model danych i slowniki
- [ ] Dodac/uzupelnic enumy i mapowania dla typow node (`T`) i relacji (`R`) zgodnie ze specyfikacja.
- [ ] Dodac obsluge pola `V` (importance 1..6) jako pola wymaganego.
- [ ] Dodac `A` do slownika `P` (provenance = AI deduction).
- [ ] Wymusic zasady custom typow/relacji/kluczy (`2..10`, bez whitespace, bez kolizji z 1-literowymi kodami).

## 2) Parser formatu `*.kg`
- [ ] Zaimplementowac parser rekordu node z wymuszona kolejnoscia pol.
- [ ] Zaimplementowac parser relacji (`> <REL>:<target_id>` + opcjonalne podpola `d/C/P/S/-`).
- [ ] Dodac walidacje timestampow ISO-8601 UTC (`YYYY-MM-DDTHH:MM:SSZ`).
- [ ] Dodac limity dlugosci dla `N/D/A/F/S/d` i custom values.
- [ ] Dodac normalizacje tekstu (trim, redukcja spacji, deduplikacja case-insensitive dla list).

## 3) Walidator i deterministyczny zapis
- [ ] Wymusic sortowanie node po `node_id`.
- [ ] Wymusic sortowanie wielowartosciowych pol (`A/F/S` i linki `>`).
- [ ] Dodac walidacje semantyczna relacji wg decision table (`DEPENDS_ON`, `HAS`, `DOCUMENTS`, itd.).
- [ ] Dodac czytelne komunikaty bledow (co, gdzie, jak poprawic).

## 4) Migracja `*.json` -> `*.kg`
- [ ] Dodac detekcje: jezeli brak `graph.kg` i istnieje `graph.json`, uruchom auto-migracje.
- [ ] Zachowac plik `*.json` bez modyfikacji (brak destrukcyjnych zmian).
- [ ] Zaimplementowac mapowanie historycznych nazw typow (case/separatory/skroty).
- [ ] Zaimplementowac konwersje relacji incoming (`<-`) do modelu outgoing (`->`).
- [ ] Dodac fallback bez utraty danych:
  - [ ] niemapowalne node type -> custom node type,
  - [ ] niemapowalne atrybuty -> `- <custom_key> <value>`.
- [ ] Generowac raport migracji (co zmapowano automatycznie, co trafilo do fallbacku).

## 5) Runtime legacy i przelaczanie
- [ ] Oznaczyc/stabilnie wydzielic stary silnik jako `legacy`.
- [ ] Dodac flage `--legacy` wymuszajaca odczyt JSON przez stary silnik.
- [ ] Zapewnic, ze domyslny runtime dziala na `*.kg`.

## 6) Telemetria dostepu i feedback (`*.kglog`)
- [ ] Wydzielic logowanie hitow/feedbacku z glownego pliku grafu do sidecara `*.kglog`.
- [ ] Dodac zapis linii:
  - [ ] `<timestamp> <user_short_uid> H <node_id>`
  - [ ] `<timestamp> <user_short_uid> F <node_id> <feedback>`
- [ ] Dodac generowanie `user_short_uid` (install-time lub first-run) i zapis w konfiguracji KG.
- [ ] Dodac testy odpornosci na brak/uszkodzenie pliku logu.

## 7) Indeks `*.kgindex`
- [ ] Dodac budowe indeksu podczas skanowania grafu (`<node_id> <line_number>`).
- [ ] Dodac szybki lookup po `node_id` z wykorzystaniem indeksu.
- [ ] Dodac invalidacje indeksu przy kazdej modyfikacji `*.kg`.
- [ ] Dodac bezpieczny rebuild indeksu po invalidacji.

## 8) Testy i jakosc
- [ ] Dodac testy parsera: przypadki poprawne + bledne (kolejnosc pol, limity, timestampy, custom).
- [ ] Dodac testy migracji: mapowania inteligentne, fallback, brak utraty danych.
- [ ] Dodac testy runtime: domyslny `*.kg`, fallback auto-migracji, `--legacy`.
- [ ] Dodac testy `*.kglog` i `*.kgindex` (tworzenie, odczyt, invalidacja, rebuild).
- [ ] Dodac test deterministycznosci serializacji (ten sam input -> ten sam output).

## 9) Rollout
- [x] Dodac notke migracyjna do changeloga/release notes (jak przejsc z JSON na KG) — `docs/eyg-rollout-notes.md`.
- [x] Przygotowac plan rollbacku (w razie bledow: uruchamianie przez `--legacy`) — `docs/eyg-rollout-notes.md`.
- [x] Dodac metryki sukcesu po wdrozeniu (czas lookupu, procent auto-mapowan, bledy walidacji):
  - p95 `node get` <= 50 ms na lokalnym grafie referencyjnym;
  - p95 `node find` <= 120 ms dla 1 zapytania i limitu 10;
  - >= 95% mapowan typow/relacji podczas automigracji bez fallbacku do custom;
  - 0 krytycznych bledow walidacji na danych produkcyjnych po migracji;
  - < 1% operacji z warningiem sidecar (`kglog/kgindex`) w 7-dniowym oknie.

## Kryteria done (Definition of Done)
- [ ] Wszystkie pola wymagane (`*`) sa walidowane i egzekwowane.
- [ ] Pole `V` jest wymagane i poprawnie serializowane/deserializowane.
- [ ] Migracja jest bezstratna (brak cichej utraty informacji).
- [ ] `*.kglog` i `*.kgindex` dzialaja jako sidecary, niezaleznie od glownego pliku grafu.
- [ ] Tryb `--legacy` pozwala bezpiecznie utrzymac kompatybilnosc wsteczna.
