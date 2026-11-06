# Pomiar pH — Sensorex S222CD przez DFRobot Gravity V2

Data: 2026-11-01
Status: zaakceptowany, w implementacji

## Problem

`Co2MetricsDto.current_ph` i `drop_checker_ph_target` istnieją w DTO od początku
i są wypełniane `None`. Nie ma za nimi żadnego sprzętu.

Celem jest odczyt pH z elektrody Sensorex S222CD i wystawienie go w telemetrii.

## Ścieżka sprzętowa

```
S222CD --BNR->BNC--> Gravity pH V2 (SEN0161-V2) --0..3,0 V--> GPIO1 (ADC1_CH0)
```

### Dlaczego nie wprost na ADC

Elektroda pH to ogniwo o impedancji wewnętrznej ~100 MΩ dające ±414 mV. ADC
w ESP32-S3 ma impedancję wejściową ~100 kΩ, czyli trzy rzędy wielkości za nisko.
Podłączona bezpośrednio daje odczyt bez związku z rzeczywistością. Gravity V2
zawiera wzmacniacz buforujący i to jest cały powód jego obecności.

**Wymagane V2 (SEN0161-V2), nie V1.** V2 ma regulator 3,3–5,5 V i wyjście
0–3,0 V, więc pracuje z ESP32 bezpośrednio. V1 był układem 5-woltowym.

### Pin: ADC1, nigdy ADC2

GPIO1 (ADC1_CH0). **ADC2 jest bezużyteczny przy włączonym Wi-Fi**, a w tym
urządzeniu Wi-Fi jest włączone zawsze. Na ESP32-S3 ADC1 to GPIO1–GPIO10.

Przydział czujników poziomu wody został przy okazji tego projektu przeniesiony
z GPIO5/6/7/15 na GPIO11–14 właśnie po to, żeby zwolnić kanały ADC1 — szczegóły
w specu XKC-Y25.

### Elektroda

S222CD: kombinowana, 12 mm, korpus epoksyd/poliwęglan, referencja Ag/AgCl,
0–14 pH, 0–80 °C, izopotencjał 7,00 pH, złącze **BNR (quick disconnect)**.

Złącze BNR, nie BNC — potrzebny jest przewód przejściowy.

Podwójne złącze jest istotne dla akwarium, nie kosmetyczne: bufor (KNO₃) w dolnej
komorze blokuje reakcje z siarczkami, białkami i metalami ciężkimi, czyli z tym,
co jest w wodzie akwariowej. Elektroda z pojedynczym złączem zatruwa się tam szybko.

### Czego ten układ nie rozwiązuje

**Brak izolacji galwanicznej.** W wodzie siedzą już sondy DS18B20 podłączone do
masy ESP32, a obok pracują pompy i grzałka zasilane z sieci. Nieizolowany
front-end pH zbiera te prądy i daje dryf. To ograniczenie sprzętowe, nieusuwalne
w firmware — do rozważenia osobno, albo świadoma zgoda na częstszą kalibrację.

## Zakres

W zakresie: odczyt ADC, konwersja na pH z kalibracją dwupunktową i kompensacją
temperaturową, walidacja, sygnał zdrowia elektrody, pole w telemetrii.

Poza zakresem: **zapis kalibracji w NVS i endpointy kalibracyjne w API.**
Współczynniki są w tym specu stałymi w `settings.rs`, więc rekalibracja wymaga
przeflashowania. To jest świadomie zostawione na drugi spec.

Powód podziału jest ten sam co przy XKC-Y25 i ATO: realnych napięć **swojej**
elektrody w **swojej** wodzie nie da się przewidzieć. Workflow kalibracji
projektuje się lepiej, mając te liczby. Ten spec je dostarcza.

Uwaga operacyjna, która z tego wynika: DFRobot zaleca rekalibrację raz w miesiącu
(częściej przy intensywnym użyciu). Dopóki drugi spec nie wejdzie, oznacza to
przeflashowanie raz w miesiącu. Jest to akceptowalne tylko jako stan przejściowy.

## Matematyka

`core/src/protocol/ph.rs` — czysta, bez ADC, bez zegara, bez NVS.

```rust
pub struct PhCalibration {
    pub neutral_volts: f32,
    pub acid_volts: f32,
    pub calibrated_at_celsius: f32,
}

pub fn convert(volts: f32, celsius: f32, cal: &PhCalibration) -> Result<Ph, PhError>
```

### Kalibracja dwupunktowa

Bufory 7,00 i 4,00 (te, które DFRobot dokłada do zestawu).

```
slope_ph_per_volt = (4,00 - 7,00) / (acid_volts - neutral_volts)
pH                = 7,00 + (volts - neutral_volts) * slope_ph_per_volt
```

**Formuła nie zakłada polaryzacji płytki.** Jeśli `acid_volts > neutral_volts`,
nachylenie wychodzi ujemne i wynik jest poprawny; jeśli odwrotnie — również.
Nie trzeba wiedzieć, czy Gravity odwraca znak, bo kalibracja to mierzy.

### Kompensacja temperaturowa

Nachylenie elektrody jest proporcjonalne do temperatury bezwzględnej (Nernst):

```
slope(T) = slope_cal * (T_cal + 273,15) / (T + 273,15)
```

Temperatura pochodzi z DS18B20 w roli `MainTank` — czujnik już jest w tej wodzie,
nie dochodzi żaden nowy.

**Gdy czujnik akwarium nie odpowiada**, kompensacja używa skonfigurowanego
`target_celsius`. Rozważano zwracanie `None`, ale w zakresie pH akwariowego
(6,5–8,2, czyli do ~1,2 jednostki od neutralnego) rozjazd 10 °C daje błąd
poniżej 0,05 pH — mniej, niż warte jest wyrzucenie odczytu. Konsument i tak wie,
że temperatura padła, bo `temperature.sensors_healthy` jest wtedy `false`;
osobna flaga byłaby duplikacją tego sygnału.

**To, co ta kompensacja poprawia, i to, czego nie poprawia** — spec musi to
powiedzieć wprost, bo jest to klasyczne nieporozumienie:

- **poprawia** odpowiedź elektrody, czyli fakt, że ta sama różnica pH daje inne
  napięcie w 20 °C i w 30 °C
- **nie poprawia** zmiany pH samego roztworu z temperaturą; to jest własność
  chemiczna wody i prawdziwy pomiar, nie błąd

Kto to myli, „naprawia" w firmware rzecz, która jest poprawna.

### Walidacja

Każda reguła odpowiada realnej awarii, nie teoretycznej:

| Reguła | Awaria, którą wykrywa |
|---|---|
| `\|neutral_volts - acid_volts\|` poniżej progu | zmierzono dwa razy ten sam bufor, albo elektroda jest martwa |
| wartości nieskończone lub `NaN` na wejściu | odłączony ADC, błąd konwersji |
| wynik poza 0–14 pH | kalibracja z innego świata, zwarcie na wejściu |

`Ph` jest typem domenowym z walidacją w konstruktorze, dokładnie jak
`WaterTemperature` w `domain/metrics.rs`, i tam też mieszka.

### Zdrowie elektrody

```rust
impl PhCalibration {
    pub fn volts_per_ph(&self) -> f32
}
```

Zużyta elektroda ma mniejsze nachylenie. Porównanie z nominałem daje sygnał
„czas na nową", zanim odczyty zaczną kłamać.

**Nominału nie da się wyliczyć z fizyki**, bo wzmocnienie płytki Gravity nie jest
znane z dokumentacji. Idealne 59,16 mV/pH w 25 °C dotyczy elektrody, nie wyjścia
płytki. Dlatego nominalne `volts_per_ph` jest **wartością konfiguracyjną**
zmierzoną raz na świeżej elektrodzie, z domyślną 0,05916 V/pH wynikającą
z założenia wzmocnienia jednostkowego — założenia, które trzeba potwierdzić
pomiarem i które spec jawnie oznacza jako niepotwierdzone.

Zdrowie raportujemy jako stosunek zmierzonego nachylenia do nominalnego. Poniżej
0,90 elektroda jest zużyta.

## Refaktor collectora — w zakresie

`TelemetryCollector::new` przyjmuje dziś sześć źródeł i ma
`#[allow(clippy::too_many_arguments)]`. pH byłoby siódmym argumentem pozycyjnym.

Źródła przechodzą w strukturę z nazwanymi polami:

```rust
pub struct TelemetrySources<R, C, H, O, W, P> {
    pub temperature: R,
    pub clock: C,
    pub health: H,
    pub overrides: O,
    pub levels: W,
    pub ph: P,
}

impl ... {
    pub fn new(sources: TelemetrySources<R, C, H, O, W, P>, sink: S, target_celsius: f32) -> Self
}
```

Liczba parametrów generycznych się nie zmienia — zmienia się miejsce wywołania,
które przestaje być ciągiem siedmiu pozycji bez nazw. `allow` znika.

Zapisałem tę rekomendację w specu trybów komponentów („następny czujnik powinien
zacząć od niego"); pH jest tym czujnikiem.

## Port

```rust
// core/src/ports/outbound/ph.rs
#[allow(async_fn_in_trait)]
pub trait PhReader {
    async fn read_volts(&mut self) -> Result<f32, PhError>;
}
```

Port zwraca **wolty, nie pH.** Konwersja wymaga temperatury i kalibracji, a obie
należą do collectora, nie do adaptera ADC. Adapter ma jedno zadanie: zamienić
kanał ADC na napięcie.

## Adapter

`src/adapters/outbound/ph.rs`, za `#[cfg(target_os = "espidf")]`.

`AdcDriver` + `AdcChannelDriver` w trybie kalibrowanym, żeby zadziałała krzywa
z eFuse — bez niej ADC w S3 ma błąd rzędu procentów. Do tego uśrednianie N próbek:
przy ~214 mV na jednostkę pH szum ADC przekłada się wprost na odczyt, a średnia
z kilkudziesięciu próbek jest najtańszym sposobem go stłumić.

Komponent dostaje `ComponentMode`. Wyłączone pH wyraża się jednak **nie enumem
źródła** (jak `TemperatureSource` przy DS18B20), a blanket implem w porcie:

```rust
impl<T: PhReader> PhReader for Option<T> {
    // None -> Err(PhError::Unavailable("pH component is disabled in settings"))
}
```

Powód jest praktyczny: `PhProbe` jest generyczny po kanale ADC, więc nazwanie
wariantu `Disabled` wymagałoby w `main.rs` wypisania tych generyków ręcznie.
Z `Option` typ wywnioskuje się z gałęzi `Some` i miejsce wywołania zostaje czyste.
Blanket impl jest przy tym testowalny na hoście, czego enum w crate'cie firmware
nie byłby.

`current_ph` jest wtedy `None`, czyli uczciwie „brak odczytu".

## Telemetria

`Co2MetricsDto.current_ph: Option<f32>` — pole **już istnieje**, nie dochodzi
nowa sekcja. Błąd odczytu lub wyłączony komponent dają `None`, nigdy liczbę
zastępczą. Zdrowie elektrody wchodzi jako `ph_electrode_health: Option<f32>`
w tej samej sekcji.

### Eksport do Influxa

Dochodzi pomiar `aquarium_co2 ph=...,electrode_health=... <ts>`.

`solenoid_open` **nie** jest eksportowane, dopóki nie ma sterowania CO2 —
eksportowanie stałego `false` to szum. Przez to pomiar `aquarium_co2` jest
zbudowany wyłącznie z pól opcjonalnych i staje się **pierwszym realnie osiągalnym
przypadkiem** zabezpieczenia „pomiar bez pól nie generuje linii" ze specu Influxa.
Przy niepodłączonej sondzie linia po prostu nie powstaje, zamiast wysyłać
niepoprawny Line Protocol i unieważniać całą partię.

## Testy

Na hoście przez `make test`. Baseline: **132 jednostkowe + 19 integracyjnych**.

`protocol/ph.rs`
- napięcie równe `neutral_volts` daje dokładnie 7,00
- napięcie równe `acid_volts` daje dokładnie 4,00
- interpolacja w połowie daje 5,50
- ekstrapolacja powyżej punktu neutralnego działa w obie strony
- odwrócona polaryzacja płytki daje ten sam wynik
- wyższa temperatura niż kalibracyjna zmniejsza odchylenie od 7,00
- temperatura kalibracyjna nie zmienia nic
- dwa identyczne punkty kalibracji są odrzucane
- `NaN` i nieskończoność są odrzucane
- wynik poza 0–14 jest odrzucany
- `volts_per_ph` liczy się z rozpiętości trzech jednostek pH

`domain/metrics.rs`
- `Ph::new` przyjmuje 0–14 i odrzuca resztę, `NaN` oraz nieskończoności

`ports/outbound/ph.rs`
- `Some(probe)` deleguje odczyt
- `None` raportuje sondę jako niedostępną

`protocol/line_protocol.rs`
- pH i zdrowie elektrody trafiają do `aquarium_co2`
- brak odczytu pH pomija pole, zostawiając zdrowie
- sekcja CO2 bez żadnego pola nie generuje linii

`application/collector.rs`
- `current_ph` odzwierciedla źródło przepuszczone przez kalibrację
- błąd odczytu daje `None`, nie wartość zastępczą
- kompensacja używa temperatury akwarium, nie sumpa
- istniejące testy przechodzą po refaktorze źródeł

## Konwencja

Kod bez komentarzy poza uzasadnieniem wybranej wartości. Dokumentacja po polsku.
**Nie commitować.**
