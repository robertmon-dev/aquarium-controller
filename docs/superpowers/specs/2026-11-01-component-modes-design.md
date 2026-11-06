# Tryby komponentów w ustawieniach

Data: 2026-11-01
Status: zaakceptowany, zaimplementowany

## Problem

Wybór tego, co kontroler uruchamia, był rozsiany po pięciu niezależnych
mechanizmach:

| Mechanizm | Komponent |
|---|---|
| `const ROM_DISCOVERY_MODE: bool` w `main.rs` | tryb odczytu ROM |
| ręczne usuwanie linii z `run()` | collector + DS18B20 |
| `Option<InfluxSettings>` w ustawieniach | eksport do Influxa |
| usunięcie wywołania commitem (48b17e4) | `health_watch` |
| brak jakiejkolwiek kontroli | Wi-Fi, HTTP, poziom wody |

Konsekwencja praktyczna: bisekcja przepełnienia stosu przy picoserve wymagała
edycji kodu, a wyłączony collector został w drzewie roboczym jako martwy kod,
przez co `make lint` padał na ośmiu ostrzeżeniach o nieużywanych elementach.

## Model

```rust
pub enum ComponentMode { Off, Initialized, Running }
```

- `Off` — nie konstruowane, peryferia zostają niezajęte
- `Initialized` — skonstruowane, więc błędy inicjalizacji się ujawniają, ale
  niewpięte w żadną pętlę
- `Running` — skonstruowane i wpięte: zadanie wystartowane albo obiekt przekazany
  konsumentowi, który go napędza

Trzy stany, a nie dwa niezależne `bool`-e, bo **awaria w systemie wbudowanym
przychodzi z obu połówek osobno**: zajęcie peryferium albo spawn wątku potrafi
zawieść przy konstrukcji, a wyczerpanie stosu i rywalizacja o magistralę dopiero
pod obciążeniem zadania okresowego. Rozdzielenie tych dwóch rzeczy jest całym
sensem tej zmiany. Jednocześnie kombinacja bez sensu — „w pętli, ale
nieskonstruowane" — jest w tym modelu niereprezentowalna, czego para `bool`-i nie
daje bez walidacji w runtime.

Odrzucono `Option<Config>` na komponent (jak dotychczasowy `Option<InfluxSettings>`):
typowo bezpieczniejsze, bo nie da się włączyć czegoś nieskonfigurowanego, ale gubi
rozróżnienie init/pętla i nie pozwala wyłączyć samej pętli bez usunięcia całej
konfiguracji.

## Komponenty

`wifi`, `http`, `temperature`, `telemetry`, `water_level`, `influx`, `health`.

`temperature` jest osobny od `telemetry` celowo — to dokładnie ten komponent,
który był wyłączany ręcznie, i jego inicjalizacja (magistrala 1-Wire, walidacja
ROM-ów, kontrola zasilania pasożytniczego) jest osobnym miejscem awarii niż tick
collectora.

`influx` ma dwie bramki: tryb **oraz** obecność czterech zmiennych `INFLUX_*`.
Brak zmiennych wyłącza eksport niezależnie od trybu — konfiguracja odpowiada na
„czy może", tryb na „czy ma".

## Źródło temperatury

`TelemetryCollector` wymaga `R: TemperatureReader`, więc wyłączenie czujnika
potrzebuje czegoś na jego miejsce:

```rust
pub enum TemperatureSource<'d> {
    Ds18b20(Ds18b20Adapter<'d>),
    Disabled,
}
```

Dyspozycja przez enum, nie `Box<dyn>`: trait ma metody `async`, a przypadków jest
dokładnie dwa.

**Collector nie wymaga żadnej zmiany.** `read_sensors()` już zamienia nieudaną
konwersję na `(None, None)` i raportuje `sensors_healthy: false`, więc wyłączony
czujnik daje w telemetrii uczciwe „brak odczytu", a nie wymyśloną wartość.

Analogicznie poziom wody nie potrzebuje atrapy: przy niedziałającym samplerze
`InMemoryWaterLevelStore` serwuje `WaterLevelSnapshot::dry()`, więc odczyt też
degraduje się sam.

## `RunMode`

`ROM_DISCOVERY_MODE` przeniesiony z `main.rs` do `Settings` jako
`enum RunMode { Normal, RomDiscovery }`. To nie komponent — zwiera całe `run()` —
ale jest piątym mechanizmem wyboru tego, co się uruchamia, więc zostawienie go
w kodzie przeczyłoby celowi zmiany.

## Zadania opcjonalne

```rust
async fn optional<T, F: Future<Output = T>>(task: Option<F>)
```

Nieobecny komponent parkuje na `core::future::pending()`, zamiast kończyć się od
razu. Firmware nie ma prawa wypaść z `run()`, a `tokio::join!` czeka na wszystkich
uczestników — zakończenie jednego natychmiast nie zmieniłoby niczego dziś, ale
byłoby pułapką przy pierwszym `select!`.

Helper jest generyczny po typie wyniku, bo `http::run` zwraca `!`, a
`WifiManager::run` zwraca `()`.

## Stan `Initialized` dla HTTP

`Initialized` bez `Running` **binduje port i zwalnia go** — `TcpListener` jest
dropowany na końcu ramienia `match`. Dowiedzenie, że bind się udaje, jest całym
sensem tego stanu; utrzymywanie otwartego gniazda, którego nikt nie obsługuje,
nie dawałoby nic więcej.

## Efekt uboczny: lint przechodzi

Przed zmianą `make lint` padał pod `-D warnings` na ośmiu nieużywanych elementach
(`Ds18b20Adapter`, `EspSystemHealth`, `clock`, `lighting_changed`, `sensor_task`,
`health_watch`, `log_health`, `HEALTH_INTERVAL`) — część osierocona commitem
48b17e4, część wyłączonym collectorem. Wszystkie wróciły do użycia, więc
`make lint` jest zielony.

## Weryfikacja

**Ta zmiana nie ma testów hostowych i to jest ograniczenie, nie przeoczenie.**
`make test` buduje wyłącznie `aquarium-core`; `ComponentMode` i całe `run()`
żyją w crate'cie firmware, który nie ma na hoście harnessu testowego. Zmiana jest
sprawdzona `make check` (kompilacja na `xtensa-esp32s3-espidf`) i `make lint`
(clippy pod `-D warnings`) — nie testami.

Same predykaty `is_initialized` / `is_running` to dwa `matches!`; przeniesienie ich
do `core` tylko po to, by je przetestować, testowałoby `matches!`, nie zachowanie.
Realna weryfikacja tej zmiany jest na sprzęcie: przełączyć każdy komponent przez
trzy stany i sprawdzić logi.

## Nie commitować
