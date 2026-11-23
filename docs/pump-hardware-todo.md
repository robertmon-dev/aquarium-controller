# Pompy i podmiany — do zrobienia po stronie sprzętu i hydrauliki

Rzeczy, których firmware nie załatwi. Kolejność według wagi.

## 1. Niezależna blokada przelania sumpa (pływak w obwodzie zasilania)

Wszystkie zabezpieczenia w kodzie opierają się na jednym czujniku na funkcję. XKC-Y25
przez szkło z czasem potrafi dawać fałszywe „mokro” (nalot, glony, koralina).

- pływak (NC, otwiera się przy wysokim poziomie) **szeregowo w przewodzie zasilania
  pompy podmian** — wysoki poziom w sumpie odcina pompę bez udziału ESP32
- niezależne odcięcie obiegowej przy niskim poziomie w komorze 5 opisano w punkcie 6
- czujniki XKC-Y25 czyścić z zewnątrz szyby co kilka tygodni

## 2. Mechaniczny limit spustu

Spust jest ograniczony tylko czasem (`drain_s` ≤ 900 s) i suchym `SumpPump`; w
zbiorniku nie ma czujnika minimum.

- wlot pompy zrzutu (albo rurki ssącej) ustawić **na wysokości minimalnego
  dopuszczalnego poziomu wody** w zbiorniku — pompa fizycznie nie spuści więcej
- zmierzyć wiadrem i stoperem wydajność pompy zrzutu i pompy podmian, wpisać w
  `PATCH /api/pumps/settings` → `flow_lph`

## 3. Lewarowanie

- jeśli wylot zrzutu jest niżej niż lustro wody w zbiorniku: **otwór
  napowietrzający** tuż za pompą albo przerwa powietrzna na wylocie (wąż nad
  odpływem, nie zanurzony)
- jeśli zbiornik uzupełnień stoi wyżej niż sump: to samo na wężu pompy podmian
- test: włączyć pompę na kilka sekund (`RunMode::RelayProbe` albo dolewka 5 s),
  wyłączyć i sprawdzić, czy woda przestaje płynąć

## 4. Watchdog sprzętowy (opcjonalnie, druga linia)

Firmware dostaje watchdog zadań ESP-IDF z resetem (zawieszony ESP32 → reset → piny
LOW → fail-safe NC/NO). Druga, niezależna linia:

- zewnętrzny układ watchdog (np. TPL5010 / timer 555 z pinem „heartbeat”) albo
  przekaźnik czasowy, który odcina zasilanie pomp NO, jeśli ESP32 przestanie
  machać pinem

## 5. Szósty czujnik poziomu w komorze pompy obiegowej

Czujniki w komorach 2 i 3 wiszą za przegrodami — ich poziom się nie zmienia, więc
nie zakończą uzupełniania. Nowy XKC-Y25 (rola `sump_return_level`):

- komora 5 (pompa obiegowa), **na poziomie roboczym** (poziom przy pracującej
  obiegowej, po uzupełnieniu odparowania)
- wejście `GPIO21` (ta sama polaryzacja i okablowanie co pozostałe XKC-Y25)
- po montażu sprawdzić w `GET /api/telemetry` (`water_level.sump_return_level`),
  że mokry/suchy zgadza się z lustrem wody

## 6. Ochrona obiegowej przed suchobiegiem niezależna od `SumpPump`

**Celem jest 0 s suchobiegu każdej pompy**, również podczas restartu ESP32 i przy
błędnym wskazaniu poziomu. W symulacji obiegowa traciła wodę w dwóch sytuacjach.

- **Restart w trakcie spustu przy zachowanym zasilaniu pomp.** W teście 30 dni
  (ziarno 18) obiegowa była wyłączona przed restartem. Po zwolnieniu przekaźnika
  styk normalnie zamknięty (NC) załączył ją ponownie. Nad wlotem było 39,5 l
  wody, co przy 3000 l/h wystarczyło na 47,4 s. Z 90 s przerwy w sterowaniu
  pozostało **42,6 s suchobiegu**. To cały wynik tego miesiąca. Skrócenie tylko
  tej przerwy do 60 s dało 12,6 s suchobiegu, a do 30 s — 0 s. Obecny test
  dopuszcza suchobieg podczas takiego restartu. [Test miesięczny](../crates/core/tests/e2e_soak.rs#L346-L355)
- **Fałszywe „mokro” podczas napełniania.** Po spuście 125 l czujnik `SumpPump`
  stale zgłaszał wodę, choć poziom w komorze obiegowej opadł do wlotu. Model
  odnotował **378,6 s suchobiegu**, a podmiana zakończyła się wynikiem `SumpFull`.
  Symulator przelicza niedobór przepływu na równoważny czas suchobiegu.
  [Test podmiany](../crates/core/tests/e2e_water_change.rs#L869-L908) · [Model przepływu](../crates/core/tests/sim/mod.rs#L488-L495)

W komorze 5 zamontować niezależny pływak minimum, który odetnie obiegową,
zanim poziom spadnie do wlotu. Odcięcie musi działać także przy wyłączonym ESP32
i stale mokrym `SumpPump`. Przy dostatecznym poziomie wody styk NC nadal pozwala
utrzymać obieg podczas restartu. Pomiar przepływu lub prądu może dodatkowo
potwierdzać pracę pompy.

**Sprawdzenie po montażu.** Przy zanurzonej pompie zasymulować niski poziom
pływakiem i potwierdzić zatrzymanie obiegowej w obu powyższych warunkach.

## 7. Wymiarowanie spustu i zbiornika uzupełnień

Z testu e2e (zbiornik 700 L, zrzut 1500 L/h, podmian 600 L/h):

- `drain_s` dobierać z zapasem tak, żeby `drain_s × flow_lph.drain / 3600`
  litrów mieściło się w objętości wody nad wlotem zrzutu. W teście pompa
  o wydajności 1500 l/h miała do dyspozycji 137 l. Przy zadanych 600 s
  przepracowała ostatnie **271,1 s na sucho**. [Test długiego spustu](../crates/core/tests/e2e_water_change.rs#L422-L444)
- czujnik `reservoir_ready` zamontować na wysokości, przy której w zbiorniku
  uzupełnień jest **co najmniej objętość spustu + zapas** (dla 125 L spustu
  ≥ ~140 L nad czujnikiem `sump_reserve`). Jeśli zbiornik uzupełnień skończy się w
  trakcie, w sumpie brakuje wody, obiegowa stoi na suchym `SumpPump`, a z nią UV
- obiegowa przy uzupełnianiu startuje ~16–20 razy na podmianę 125 L — sprawdzić,
  czy pompa to znosi (DC z soft-startem: tak; starsze synchroniczne: OK, ale
  głośne)

## 8. Grzałka sumpa (komora 3)

Komora 3 oddaje wodę tylko górą (przegroda 3→4), więc przy podmianie jej poziom nie
spada — firmware nie ma i nie potrzebuje blokady grzałki od poziomu. Grzałkę przed
pracą w powietrzu (wyciek z komory 3, opróżnianie sumpa przy serwisie) chronią:

- wbudowany termostat grzałki ustawiony 1–2 °C nad celem
- bezpiecznik termiczny / ochrona przed suchobiegiem w grzałce (sprawdzić model)
- **przy serwisie sumpa: przed opróżnianiem komory odłączyć zasilanie grzałki
  (wtyczka z gniazda) na cały czas prac.** Podłączyć ponownie dopiero po napełnieniu
  komory i sprawdzeniu wymaganego zanurzenia. Polecenie OFF w aplikacji
  (`PUT /api/heaters/sump/override`) **wygasa po restarcie sterownika** — po
  restarcie termostat przy zimnej wodzie załączy grzałkę ponownie, więc samo OFF z
  aplikacji nie zabezpiecza serwisu

## 9. Konfiguracja po montażu (software, ale zależy od montażu)

- czujnik kończący uzupełnianie: domyślnie `sump_return_level` (punkt 5); jeśli
  wcześniej zapisano inny, ustawić
  `PATCH /api/pumps/settings` → `{"refill_stop_sensor":"sump_return_level"}`
- sprawdzić `RunMode::RelayProbe`: każdy kanał klika i świeci diodą przy 3,3 V
- build z wyłączonym WiFi: zegar startuje bez SNTP, nie ma assertu
- pierwsza podmiana „na sucho”: małe `drain_s`/`refill_s`, obserwacja przy
  akwarium

## 10. Beczka uzupełnień

- Zamontować pływak NC szeregowo w zasilaniu obu zaworów lub mechaniczny zawór pływakowy na dopływie. Sprawdzić odcięcie przy pełnej beczce.
- Umieścić sondę TDS poniżej `sump_reserve`. Cyrkulator ma mieszać całą objętość i być zanurzony, kiedy dolny czujnik jest mokry.
- Zmierzyć przepływy RO z boosterem i kranówki w litrach na godzinę (`flow_lph`). Stoperem zmierzyć czas od pustej beczki do zadziałania `reservoir_full` na samym RO i wpisać wynik w sekundach jako `empty_fill_s`.
- Sprawdzić GPIO40/41/42/47 przy resecie i w bootloaderze. W `RunMode::RelayProbe` zweryfikować kolejność modułu 2 i dwusekundowe wyprzedzenie boostera przez zawór RO.
- Zarezerwować GPIO38 pod czujnik wycieku. Po pierwszych dolewkach sprawdzić historię TDS akwarium, ponieważ dolewka odparowanej wody wnosi także rozpuszczone sole.
