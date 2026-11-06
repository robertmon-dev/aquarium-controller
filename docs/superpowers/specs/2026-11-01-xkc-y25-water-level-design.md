# Sterownik XKC-Y25 — bezkontaktowy pomiar poziomu wody

Data: 2026-11-01
Status: zaakceptowany, w implementacji

## Problem

Kontroler czyta dziś wyłącznie temperaturę (dwa DS18B20 na magistrali 1-Wire).
`TankTelemetryDto` ma już sekcje na filtrację, CO2, ATO i wykrycie wycieku, ale
wszystkie są wypełniane wartościami zerowymi — nie ma za nimi żadnego sprzętu.

Celem jest sterownik czterech czujników XKC-Y25: po jednym w każdej z trzech komór
sumpa i jeden w akwarium.

## Topologia zbiornika

| Rola | Miejsce | Funkcja |
|---|---|---|
| `SumpMechanical` | komora 1 — filtracja mechaniczna | alarmowa |
| `SumpBiological` | komora 2 — filtracja biologiczna | alarmowa |
| `SumpPump` | komora 3 — pompa obiegowa + ATO | regulacyjna |
| `MainTank` | akwarium | alarmowa |

Rozróżnienie nie jest kosmetyczne. W komorze 3 poziom **legalnie** spada od parowania
— to jedyne miejsce, gdzie spadek jest normalny i gdzie przyszłe ATO ma czego słuchać.
W pozostałych trzech punktach poziom trzyma hydraulika (przelewy między komorami,
odpływ z akwarium), więc każda zmiana oznacza awarię: zatkany odpływ, pękniętą rurę
albo zatrzymaną pompę.

Piąta rola, `Reservoir` (zbiornik RO), **nie wchodzi** do tego specu. Pole
`AtoMetricsDto.reservoir_empty` już na nią czeka. Dochodzi razem z ATO, bo dopiero
tam ma konsumenta — ochronę pompki przed pracą na suchym.

## Zakres

W zakresie: odczyt pinów, filtrowanie sygnału, detekcja usterki czujnika, port
w `core`, nowa sekcja w telemetrii.

Poza zakresem: **sterowanie czymkolwiek.** Żadnego wyjścia na pompę, żadnej maszyny
stanów dolewki, żadnych progów alarmowych. Sekcja `ato` w DTO zostaje wypełniona
zerami dokładnie tak jak dziś. Port `WaterLevelSource` jest punktem zaczepienia,
w który ATO wepnie się bez zmiany tego projektu.

Powód podziału jest merytoryczny, nie organizacyjny: dobrych liczb dla ATO (czas
filtrowania, limit nalewania, histereza) nie da się wybrać przed montażem. Wybiera
się je, patrząc w logi z działających czujników. Ten spec dostarcza te logi.

## Wariant czujnika i okablowanie

**Wymagany wariant: XKC-Y25 z wyjściem NPN (otwarty kolektor).** To wymóg, nie
preferencja:

- wyjście NPN zwiera do masy albo zostaje wiszące, więc z wewnętrznym pull-upem
  ESP32-S3 pin widzi wyłącznie 0 V lub 3,3 V — czujnik zasilany z 5 V, zero dzielników
- wariant PNP podaje na wyjściu VCC, czyli 5 V na pin, który nie jest 5 V tolerant;
  wymagałby czterech dzielników napięcia i czterech okazji do pomyłki

Odrzucono wariant UART (`-T12V` / `-RS485`). XKC-Y25 jest czujnikiem **punktowym** —
niezależnie od wariantu zwraca jeden bit („na tej wysokości jest woda"). UART kupuje
oszczędność pinów, których ESP32-S3 ma nadmiar, a płaci protokołem ramkowym
z adresowaniem, CRC i timeoutami. Jedyną realną przewagą UART-a jest diagnostyka
(brak ramki = czujnik padł), co częściowo nadrabia detekcja migotania (niżej).

### Wymaganie montażowe

**Czujnika nie wolno montować na poziomie spoczynkowym wody.** Przy pracującej pompie
lustro wody chlupie o milimetry; czujnik ustawiony dokładnie na progu detekcji będzie
się przełączał tak długo, jak długo poziom tam stoi. Filtr zamrozi wtedy ostatni
stabilny stan i może niesłusznie zgłosić usterkę.

To ograniczenie **mechaniczne, nie programowe** — nie da się go usunąć doborem
parametrów filtra. Lekarstwo: rurka uspokajająca albo montaż poza normalnym zakresem
roboczym.

### Przydzielone piny

GPIO11, GPIO12, GPIO13, GPIO14 — odpowiednio dla komory 1, 2, 3 i akwarium.

Dobór wymuszony przez dwie rzeczy. Niedostępne są: GPIO4 (1-Wire), GPIO26–32
(flash SPI), **GPIO33–37 (ośmiobitowy PSRAM — `CONFIG_SPIRAM_MODE_OCT=y`
w `sdkconfig.defaults`)**, GPIO19/20 (USB) oraz GPIO0/3/45/46 (piny strapujące).

Ważniejsza jest druga reguła: **wejścia cyfrowe idą na piny ADC2, a ADC1 zostaje
wolny dla sond analogowych.** Na ESP32-S3 ADC1 to GPIO1–GPIO10, a ADC2 to
GPIO11–GPIO20 — i **ADC2 jest bezużyteczny przy włączonym Wi-Fi**, które w tym
urządzeniu jest włączone zawsze. Piny ADC2 nie mają więc alternatywnego
zastosowania i są naturalnym miejscem na wejścia cyfrowe, podczas gdy każdy pin
ADC1 zużyty na cyfrowe wejście jest bezpowrotnie odebrany przyszłej sondzie.

Pierwsza wersja tego specu przydzielała GPIO5/6/7/15, czyli trzy kanały ADC1
(CH4/CH5/CH6). Poprawione przy projektowaniu pomiaru pH, który ADC1 potrzebuje.
GPIO4 pod 1-Wire zużywa ADC1_CH3 i to zostaje jako zaszłość.

Numery pinów żyją w `src/main.rs`, nie w `Settings`: `peripherals.pins.gpioN`
to osobne typy, więc nie da się wybrać pinu wartością z konfiguracji.

## Czysta logika

Podział jak przy DS18B20: pojęcia akwarystyczne w `domain/`, dziwactwa konkretnego
czujnika w `protocol/`. Wzorem jest para `domain::metrics::WaterTemperature`
(pojęcie) i `protocol::ds18b20::parse_scratchpad` (format).

### `core/src/domain/water.rs`

```rust
pub enum WaterLevelState { Wet, Dry }

pub enum WaterLevelRole { SumpMechanical, SumpBiological, SumpPump, MainTank }

pub struct WaterLevelReading {
    pub role: WaterLevelRole,
    pub state: WaterLevelState,
    pub faulty: bool,
}

pub struct WaterLevelSnapshot {
    pub sensors: [WaterLevelReading; WaterLevelRole::COUNT],
}
```

### `core/src/protocol/xkc_y25.rs`

```rust
pub enum Polarity { ActiveHigh, ActiveLow }
pub fn interpret(pin_high: bool, polarity: Polarity) -> WaterLevelState

pub struct FilterConfig {
    pub debounce_samples: u16,
    pub flap_window_samples: u16,
    pub flap_max_transitions: u8,
}

pub struct LevelFilter { /* stable, candidate, agreed, transitions, elapsed */ }

impl LevelFilter {
    pub fn new(initial: WaterLevelState, config: FilterConfig) -> Self;
    pub fn push(&mut self, raw: WaterLevelState) -> bool;  // true = stan się zmienił
    pub fn state(&self) -> WaterLevelState;
    pub fn is_faulty(&self) -> bool;
}
```

Dla wariantu NPN z pull-upem domyślną polaryzacją jest `ActiveLow` (zwilżony czujnik
zwiera pin do masy), ale polaryzacja zostaje konfigurowalna — zwora MODE w XKC-Y25
odwraca logikę wyjścia i nie chcemy, by przestawienie zwory wymagało zmiany kodu.

### Filtr liczy próbki, nie czas

`LevelFilter` nie zna zegara. Tempo próbkowania zna zadanie, które go karmi, więc
progi są wyrażone w liczbie próbek. Konsekwencje:

- testy są w pełni deterministyczne i nie potrzebują nawet `tokio::time::pause()`
- cała arytmetyka czasu zostaje w jednym miejscu (zadanie próbkujące), więc nie da się
  porównać dwóch wartości o różnych punktach zerowych

To ta sama zasada, co w specu oświetlenia: domena nie zna czasu, TTL żyje w storze.

### Dwie stałe czasowe, bo dwa różne zjawiska

`debounce_samples` tłumi pojedyncze zakłócenia: zmiana stanu wymaga N zgodnych próbek
z rzędu. Jedna próbka odstająca nie robi nic.

`flap_window_samples` / `flap_max_transitions` wyłapują **usterkę elektryczną**, nie
falowanie. Przełączenia szybsze, niż menisk fizycznie potrafi przejechać przez ~10 mm
strefę detekcji czujnika pojemnościowego, to nie woda — to obluzowany przewód, zimny
lut albo uszkodzony czujnik.

**Usterka nie podmienia stanu.** Filtr dalej raportuje ostatni stabilny odczyt
i osobno podnosi `is_faulty()`. Decyzja, co zrobić z niesprawnym czujnikiem, należy
do ATO i do warstwy alarmów — sterownik ma raportować, nie interpretować.

### Czego ten sterownik nie potrafi

**Nie da się wykryć uszkodzonego cyfrowego czujnika punktowego z jego własnego
sygnału.** Stan „mokro przez tydzień" jest normalny i nieodróżnialny od zwartego
wyjścia. Detekcja migotania łapie jedną klasę usterek (niestabilne połączenie)
i to wszystko, na co uczciwie stać jeden bit.

Diagnostyka krzyżowa między czujnikami — rozważana i **odrzucona z tego specu**.
Komory sumpa są połączone hydraulicznie przelewami, więc ich poziomy są powiązane,
ale nie równe. Ustalenie, który zestaw odczytów jest sprzeczny, wymaga znajomości
wysokości montażu każdego czujnika, a tę wiedzę zdobywa się po montażu. Trafia
do specu ATO.

## Porty

```rust
// core/src/ports/outbound/water_level.rs

// Konsument: zadanie próbkujące. Implementacja: adapter GPIO.
pub trait WaterLevelPins {
    fn read_raw(&mut self) -> [(WaterLevelRole, bool); WaterLevelRole::COUNT];
}

// Konsument: zadanie próbkujące (zapis).
pub trait WaterLevelSink {
    fn publish(&self, snapshot: WaterLevelSnapshot);
}

// Konsument: collector (odczyt).
pub trait WaterLevelSource {
    fn levels(&self) -> WaterLevelSnapshot;
}
```

`read_raw` zwraca wszystkie piny naraz i **nie zwraca `Result`.** Po inicjalizacji
`PinDriver::is_high()` nie może zawieść — błędy są możliwe wyłącznie przy tworzeniu
drivera, czyli w konstruktorze adaptera. Udawany `Result` na ścieżce odczytu kazałby
każdemu wywołaniu obsługiwać wariant, który nigdy nie wystąpi.

Rozdzielenie `WaterLevelSink` / `WaterLevelSource` powtarza istniejącą parę
`TelemetrySink` / `TelemetryQuery`: zapis i odczyt to osobne kontrakty, bo mają
osobnych konsumentów.

## Zadanie próbkujące

Architektura: **osobne zadanie tokio + store.** Zadanie czyta piny co 100 ms,
przepuszcza przez filtry, publikuje odfiltrowany snapshot do store'u. Collector
przy swoim ticku (30 s) tylko czyta store.

Sama pętla próbkująca jest **w `core`**, nie w firmware, żeby dała się testować
na hoście:

```rust
// core/src/application/water_level.rs
pub struct WaterLevelSampler<P, S> {
    pins: P,
    filters: [(WaterLevelRole, LevelFilter); WaterLevelRole::COUNT],
    sink: S,
}

impl<P: WaterLevelPins, S: WaterLevelSink> WaterLevelSampler<P, S> {
    pub fn tick(&mut self);   // czyta piny -> filtry -> publish
}
```

`tick()` jest **synchroniczne**. Odczyt czterech GPIO to kilka instrukcji, nie ma tu
na co czekać — w przeciwieństwie do DS18B20, gdzie konwersja trwa ~750 ms i adapter
musi oddawać sterowanie przez `yield_now()`. Odmierzanie czasu zostaje w `src/main.rs`,
w pętli z `tokio::time::interval`.

### Dobór tempa próbkowania

100 ms × 10 próbek = **1 s na zmianę stanu**. Alarm z opóźnieniem 1 s zamiast 30 s
(tick collectora) albo 1–2 minut (filtrowanie przez kolejne ticki) — przy wycieku
to różnica, która ma znaczenie, a koszt to jedno dodatkowe zadanie tokio.

Odrzucone alternatywy:

- **Odczyt w ticku collectora.** Zero nowych zadań, ale alarm po 1–2 minutach,
  a `read_sensors()` dostaje drugą odpowiedzialność.
- **Przerwania na zboczach GPIO.** Reakcja natychmiastowa, ale czujnik pojemnościowy
  na falującej wodzie generuje burzę przerwań, więc filtr i tak jest potrzebny —
  tylko teraz w kontekście ISR. Cena nieproporcjonalna do zysku przy urządzeniu
  zasilanym z sieci.

## Store

`InMemoryWaterLevelStore` w `core/src/adapters/outbound/memory.rs`, implementujący
`WaterLevelSink` i `WaterLevelSource`, plus forwardy dla `Arc<...>` — dokładnie jak
`InMemoryTelemetryStore` robi to dla `TelemetrySink`.

Nośnikiem jest `tokio::sync::watch`, tak samo jak w store'rze telemetrii:
`send_replace` przy zapisie, `borrow().clone()` przy odczycie. Snapshot zawsze istnieje
(stan początkowy: wszystkie czujniki `Dry`, `faulty: false`), więc odczyt nie zwraca
`Option` i collector nie musi obsługiwać „jeszcze nie próbkowano".

**Bez `Notify`.** Store oświetlenia budzi collector, bo zmiana przez API ma być
natychmiast widoczna w SSE. Tutaj nie — zmiana poziomu wody nie jest komendą
użytkownika, a 30 s opóźnienia w telemetrii jest akceptowalne. Szybkiej reakcji
potrzebuje ATO, a ono będzie czytać store bezpośrednio, nie przez telemetrię.

## Telemetria

Nowa sekcja w `TankTelemetryDto`:

```rust
pub water_level: WaterLevelMetricsDto,

pub struct WaterLevelMetricsDto {
    pub sump_mechanical: WaterLevelEntryDto,
    pub sump_biological: WaterLevelEntryDto,
    pub sump_pump: WaterLevelEntryDto,
    pub main_tank: WaterLevelEntryDto,
    pub sensors_healthy: bool,
}

pub struct WaterLevelEntryDto { pub wet: bool, pub faulty: bool }
```

Pola nazwane, nie tablica: `serde-json-core` działa bez alokacji, a nazwane pola dają
czytelny i stabilny kontrakt JSON. `sensors_healthy` powtarza konwencję
z `TemperatureMetricsDto`.

**Sekcja `ato` zostaje nietknięta** (nadal zera). Wypełnienie `sump_water_low`
z `sump_pump.wet` byłoby interpretacją, a interpretacja należy do specu ATO. Dodanie
sekcji jest wstecznie zgodne — istniejące pola nie zmieniają znaczenia.

## Collector

`TelemetryCollector` zyskuje szósty generyk `W: WaterLevelSource` i w `compose()`
przepisuje snapshot ze store'u do DTO. Collector **nie filtruje i nie próbkuje** —
czyta gotowy stan.

**Zgłoszony dług:** szósty parametr generyczny to już za dużo. Przy dochodzeniu CO2,
pH i przepływu `TelemetryCollector<R, C, H, S, O, W, ...>` stanie się nieczytelny,
a każdy nowy czujnik będzie łamał sygnaturę `new()`. Lekarstwem jest spakowanie
źródeł w jedną strukturę kontekstu. Nie robimy tego tutaj, bo to refaktor
niezależny od tej funkcjonalności — ale następny czujnik powinien zacząć od niego.

## Wpięcie w firmware

**Ustalenie, które musi być w specu:** w `src/main.rs` ścieżka czujników jest dziś
**wyłączona**. Niezacommitowana zmiana w drzewie roboczym usunęła z `run()`
konstrukcję `Ds18b20Adapter`, `TelemetryCollector` i wywołanie `sensor_task(...)`;
zostało `tokio::join!(wifi.run(), server)`. Importy i `sensor_task` wiszą jako martwy
kod. Collector nie działa na sprzęcie wcale.

Wynika z tego, że **weryfikacja na sprzęcie wymaga najpierw przywrócenia tej pętli.**
Ten spec tego nie robi — to osobna decyzja właściciela, powiązana z polowaniem na
przepełnienie stosu przy picoserve (patrz komentarz w `sdkconfig.defaults`).
Implementacja dowozi kod i testy hostowe; uruchomienie na sprzęcie jest zablokowane
do czasu tej decyzji i spec nie udaje inaczej.

Docelowy kształt w `src/main.rs`:

- `Xkc25Adapter::new(gpio5, gpio6, gpio7, gpio15, polarity)` w `src/adapters/outbound/xkc_y25.rs`
- `WaterLevelSampler::new(adapter, store, filter_config)`
- zadanie `level_task` z `tokio::time::interval(100 ms)` wołające `sampler.tick()`
- `SensorSettings` dostaje `level_poll_interval`, `level_polarity` i `FilterConfig`

Adapter trzyma `PinDriver<'d, Input>` z `Pull::Up` i jest w całości za
`#[cfg(target_os = "espidf")]`, jak `ds18b20.rs` — crate `aquarium-core` zostaje
kompilowalny i testowalny na hoście.

## Testy

Wszystkie na hoście przez **`make test`**. Samo `cargo test -p aquarium-core` nie
zadziała: `.cargo/config.toml` wymusza `target = "xtensa-esp32s3-espidf"`, więc
linkowanie pada na `ldproxy`.

**Baseline przed zmianami: 84 testy jednostkowe + 18 integracyjnych, wszystkie zielone.**

`protocol/xkc_y25.rs`
- `interpret` dla obu polaryzacji
- pojedyncza odstająca próbka nie zmienia stanu
- N zgodnych próbek zmienia stan, N-1 nie wystarcza
- przerwana seria zeruje licznik zgodności
- migotanie powyżej progu podnosi `is_faulty()`
- `is_faulty()` nie zmienia `state()`
- usterka gaśnie po spokojnym oknie

`application/water_level.rs`
- `tick()` publikuje snapshot przy każdym wywołaniu — udawane `WaterLevelPins`
- każda rola trafia do snapshotu z własnym, niezamienionym stanem
- `tick()` nie publikuje stanu surowego, tylko odfiltrowany

`adapters/outbound/memory.rs`
- odczyt przed pierwszym zapisem zwraca stan początkowy (wszystko `Dry`)
- `publish` nadpisuje snapshot
- forwardy `Arc<...>` działają dla obu traitów

`application/collector.rs`
- `water_level` w DTO odzwierciedla store
- `sensors_healthy` jest `false`, gdy którykolwiek czujnik ma `faulty: true`
- sekcja `ato` pozostaje zerowa

Test integracyjny HTTP: `GET /api/telemetry` zawiera sekcję `water_level`.

## Konwencja

Kod, nazwy testów i komunikaty asercji po angielsku. Dokumentacja po polsku.

**Nie commitować.** Właściciel prowadzi historię gita sam.
