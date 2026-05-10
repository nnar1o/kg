# Kompresja G-node'ów w `kg`

## Cel
Wprowadzić prostą kompresję tekstu dla node'ów generowanych automatycznie (`G..`) w formacie `.kg`.

## Wymagania
- Kompresja dotyczy wyłącznie node'ów z typem `G..`.
- Kompresja działa na surowym tekście pliku `.kg`, przed parsowaniem.
- Dekomresja działa zawsze, gdy parser napotka znak `` ` ``.
- Słownik ma być tworzony z powtarzających się ciągów znaków.
- Minimalna długość ciągu do kompresji: domyślnie `7`.
- Dla każdego ciągu należy nadać kolejne numery: `1`, `2`, `3`, ...
- Wstawka kompresji ma używać rzadkiego znaku ASCII: `` ` ``.
- Wpis słownika ma pojawić się w pliku przed pierwszym użyciem danego ciągu.
- Mechanizm ma być transparentny dla istniejących metod API `kg`.
- Należy wypisywać statystykę skuteczności kompresji w procentach.

## Krytyczne uwagi
- Nie wolno kompresować node'ów spoza `G..`.
- Dekomresja musi działać przed parserem, inaczej format przestaje być przezroczysty.
- Nie należy wiązać mechanizmu z polami `GraphFile`; to jest warstwa tekstowa, nie model danych.
- Słownik i tokeny muszą być deterministyczne, inaczej zapis będzie niestabilny.
- Trzeba uważać na konflikt z literalnym znakiem `` ` `` w danych wejściowych.
- Testy muszą sprawdzić, że po zapisie i ponownym odczycie dane są identyczne.
