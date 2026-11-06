# Eksport telemetrii do InfluxDB 2.x

Data: 2026-11-01
Status: zaakceptowany, w implementacji

## Problem

Telemetria żyje dziś wyłącznie w pamięci: `InMemoryTelemetryStore` trzyma ostatni
snapshot, a `/api/telemetry` i `/api/events` go wystawiają. Po restarcie kontrolera
nie zostaje nic. Nie da się odpowiedzieć na pytanie „jak zmieniała się temperatura
w nocy" ani „ile razy w tym tygodniu zadziałał czujnik poziomu".

Celem jest wypychanie snapshotów do InfluxDB 2.x, z której wykresy i alarmy
historyczne robi już coś zewnętrznego (Grafana).

## Zakres

W zakresie: serializacja do Line Protocol, bufor na czas awarii sieci, transport
HTTPS, konfiguracja.

Poza zakresem: dashboardy, reguły alarmowe, retencja po stronie Influxa, zapis
na flash (bufor ginie przy restarcie — patrz „Odrzucone alternatywy").

## Topologia

```
InMemoryTelemetryStore --watch--> exporter task (tokio, async)
                                        |  mpsc + oneshot
                                        v
                                  transport worker (std::thread, blokujący TLS)
                                        |  EspHttpConnection
                                        v
                                  POST /api/v2/write
```

Eksporter **nie jest drugim sinkiem collectora.** Subskrybuje
`TelemetryQuery::subscribe()` — ten sam `watch::Receiver<TankTelemetryDto>`, z którego
korzysta `TelemetryEventSource` dla SSE. Dzięki temu `TelemetryCollector` nie zmienia
się wcale, a dodanie kolejnego konsumenta telemetrii w przyszłości nie będzie
wymagało dotykania collectora. Jego lista parametrów generycznych i tak jest już za
długa (szósty doszedł ze sterownikiem XKC-Y25).

### Dlaczego osobny wątek

`EspHttpConnection` jest **blokujący**, a runtime to `new_current_thread` z `LocalSet`.
Blokujący POST zatrzymałby serwer HTTP i zadanie próbkujące poziom wody na czas
całego round-tripu — przy nieosiągalnym serwerze na czas timeoutu.

TLS jest wymagany (InfluxDB Cloud albo LAN z TLS), więc ręcznie pisany POST na
`tokio::net::TcpStream` nie wchodzi w grę. Zostaje blokujący klient ESP-IDF na
własnym wątku `std::thread`, z `mpsc` w jedną stronę i `oneshot` z odpowiedzią.

Konsekwencja do wymiarowania: wątek dostaje własny stos. Main task zjada już 128 KiB
(`CONFIG_ESP_MAIN_TASK_STACK_SIZE`), bo jedno połączenie picoserve kosztuje ~78 KiB —
patrz komentarz w `sdkconfig.defaults`. Stos wątku transportowego trzeba ustawić
jawnie przez `ThreadSpawnConfiguration`, a nie liczyć na domyślny.

## Granica testowalności

Port po stronie tokio jest zwykłym traitem:

```rust
// core/src/ports/outbound/metrics_export.rs
#[allow(async_fn_in_trait)]
pub trait MetricsWriter {
    async fn write(&self, body: &str) -> Result<(), ExportError>;
}
```

Implementacja firmware'owa tylko przepycha `(body, oneshot::Sender<_>)` przez `mpsc`
do wątku i czeka na odpowiedź — żadnej logiki. Na hoście podstawia się atrapę, która
zapisuje treści i na żądanie zwraca błąd.

Dzięki temu **serializator, bufor i cała pętla ponawiania są testowalne na hoście.**
Niesprawdzalne na hoście zostaje tylko kilkadziesiąt linii wątku transportowego.

## Line Protocol

`core/src/protocol/line_protocol.rs` — czysta funkcja, bez zegara i bez sieci:

```rust
pub fn encode(snapshot: &TankTelemetryDto) -> Option<String>
```

Zwraca `None`, gdy snapshot nie nadaje się do zapisu (patrz bramka na zegarze).

Cztery pomiary, po jednym na sekcję DTO. Poziomy wody tagowane rolą, bo to daje
jedną serię na czujnik — idiomatyczne dla Influxa i wygodne w Grafanie:

```
aquarium_temperature tank_c=26.5,sump_c=26.0,target_c=27.0,healthy=true <ts>
aquarium_water_level,sensor=sump_mechanical wet=false,faulty=false <ts>
aquarium_water_level,sensor=sump_biological wet=false,faulty=false <ts>
aquarium_water_level,sensor=sump_pump wet=true,faulty=false <ts>
aquarium_water_level,sensor=main_tank wet=true,faulty=false <ts>
aquarium_lighting profile="daylight",white_pct=60i,rgb_pct=40i,override=false <ts>
aquarium_health uptime_s=3600i,rssi_dbm=-57i,free_heap_kb=180i <ts>
```

Typy pól zgodnie z Line Protocol: liczby całkowite z sufiksem `i`, `bool` jako
`true`/`false`, łańcuchy w cudzysłowach, liczby zmiennoprzecinkowe bez sufiksu.

### Brak escapowania — świadomie

Line Protocol wymaga escapowania przecinków, spacji i znaków równości w nazwach
pomiarów, kluczach i wartościach tagów. **Nie piszemy tego kodu.** Wszystkie nazwy
pomiarów, klucze pól i wartości tagów w tym eksporcie są stałymi z zamkniętego
zbioru (nazwy pól DTO i warianty `WaterLevelRole`), a jedyna wartość łańcuchowa
(`profile`) pochodzi z pięciowariantowego enuma `LightingProfile`. Żadna z nich nie
zawiera znaku wymagającego escapowania. Pisanie uniwersalnego escapera byłoby
martwym kodem; jeśli kiedyś dojdzie tag o wartości pochodzącej z zewnątrz, escaper
powstanie razem z nim.

### Pola bez odczytu

`Option<f32>` o wartości `None` jest **pomijane**, nie zapisywane jako zero —
inaczej zerwany czujnik wyglądałby na wykresie jak 0 °C, czyli jak awaria chłodzenia.

Z tego wynika zabezpieczenie: **pomiar, którego wszystkie pola są puste, nie generuje
linii.** Linia bez pól to niepoprawny Line Protocol i Influx odrzuci całą partię —
czyli jeden zerwany czujnik zablokowałby eksport wszystkiego innego.

Uwaga co do stanu faktycznego w chwili pisania: żaden z czterech pierwotnych pomiarów
nie mógł do tego stanu dojść, bo każdy miał co najmniej jedno pole nieopcjonalne
(`healthy`, liczby w `aquarium_health`, `profile`, `wet`). Zabezpieczenie było więc
obroną na przyszłość i było testowane bezpośrednio na `Fields`/`line`.

**Ta przyszłość nadeszła przy pomiarze pH.** Dodany pomiar `aquarium_co2` jest
zbudowany wyłącznie z pól opcjonalnych (`ph`, `electrode_health`), bo `solenoid_open`
nie jest eksportowane, dopóki nie ma sterowania CO2. Przy niepodłączonej sondzie pH
sekcja nie ma żadnego pola i zabezpieczenie realnie działa — patrz spec pomiaru pH.

Wartości niebędące liczbami skończonymi (`NaN`, `inf`) są pomijane tak samo.
`WaterTemperature::new` odrzuca już `NaN`, ale `target_celsius` przychodzi
z konfiguracji jako surowy `f32` i nie przechodzi przez żadną walidację.

## Bramka na zegarze

`timestamp_epoch_s` jest zerem, dopóki SNTP się nie zsynchronizuje
(`Clock::epoch_seconds()` zwraca wtedy `None`, a `compose()` podstawia 0).
Z `precision=s` zapisałoby to punkty w roku 1970 i zatruło bucket.

**Snapshot z zerowym znacznikiem czasu jest odrzucany, nie buforowany.** Buforowanie
go nie ma sensu: po synchronizacji nie da się go już uczciwie datować, bo nie wiadomo,
jak dawno powstał. Lepiej mieć dziurę na początku wykresu niż punkty z fałszywą datą.

## Bufor

`core/src/application/metrics_export.rs`:

```rust
pub struct MetricsExporter<W> {
    writer: W,
    pending: VecDeque<String>,
    capacity: usize,
    batch_max_points: usize,
}
```

Interfejs bufora celowo **nie ma ścieżki „wrzuć z powrotem po błędzie"**:

```rust
fn body(&self, max: usize) -> Option<(String, usize)>  // treść + liczba objętych punktów
fn commit(&mut self, count: usize)                     // tylko po sukcesie
```

Nieudana partia po prostu zostaje w buforze. Alternatywa `take_batch` + requeue
tworzy ścieżkę, na której da się zgubić albo zdublować punkty; tutaj nie ma gdzie.

Przy pełnym buforze wypadają **najstarsze** punkty. Pojemność domyślna 1000: przy
ticku 30 s to ~8 h odporności na awarię sieci. PSRAM jest włączony
(`CONFIG_SPIRAM=y`, `CONFIG_SPIRAM_MODE_OCT=y`), więc kilkaset kilobajtów kolejki
nie jest problemem.

## Transport

`src/adapters/outbound/influx.rs`, w całości za `#[cfg(target_os = "espidf")]`:

- wątek `std::thread` ze stosem ustawionym przez `ThreadSpawnConfiguration`
- `EspHttpConnection` z `crt_bundle_attach: Some(esp_crt_bundle_attach)` — symbol
  jest w bindingach `esp-idf-sys`, więc TLS działa na wbudowanym bundlu certyfikatów
  i nie trzeba wklejać własnego PEM-a
- `POST {url}/api/v2/write?org={org}&bucket={bucket}&precision=s`
- nagłówki: `Authorization: Token {token}`, `Content-Type: text/plain; charset=utf-8`
- sukces to **2xx** (Influx odpowiada `204 No Content`); `4xx` inne niż `429`
  traktujemy jako błąd trwały i punkty są **odrzucane**, bo ponawianie żądania
  odrzuconego z powodu złego tokenu albo nieistniejącego bucketu nigdy się nie uda
  i tylko zapchałoby bufor

## Ustawienia

```rust
pub struct InfluxSettings {
    pub url: &'static str,
    pub org: &'static str,
    pub bucket: &'static str,
    pub token: Secret<&'static str>,
    pub buffer_capacity: usize,     // 1000
    pub batch_max_points: usize,    // 100
    pub retry_backoff: Duration,    // 30 s
}
```

Token przez `Secret<&'static str>` — ten typ już istnieje i redaguje się w `Debug`,
więc nie wycieknie do logów. Wartość z `env!()`, jak `API_TOKEN` i hasło Wi-Fi;
`Makefile` dostaje nowe zmienne do `check-env`.

## Testy

Na hoście przez `make test`. Baseline przed tą pracą: **112 testów jednostkowych
+ 19 integracyjnych**.

`protocol/line_protocol.rs`
- pełny snapshot daje wszystkie cztery pomiary
- każda rola poziomu wody dostaje własną linię z własnym tagiem
- `None` w `tank_celsius` pomija pole, nie zapisuje zera
- zestaw pól, z którego wszystko wypadło, nie generuje linii (test na `Fields`/`line`)
- `NaN` i nieskończoność w `target_celsius` są pomijane
- liczby całkowite mają sufiks `i`, `bool` jest bez cudzysłowów, `profile` w cudzysłowach
- zerowy znacznik czasu daje `None`

`application/metrics_export.rs`
- udany zapis opróżnia bufor
- nieudany zapis zostawia punkty i ponawia je przy następnej próbie
- partia jest ograniczona do `batch_max_points`
- przepełnienie bufora wyrzuca najstarszy punkt, nie najnowszy
- trwały błąd (`4xx` inny niż `429`) odrzuca partię
- `429` i `5xx` zostawiają partię do ponowienia
- snapshot bez zsynchronizowanego zegara nie wchodzi do bufora

## Odrzucone alternatywy

**Drugi sink w collectorze.** Wymagałby albo siódmego parametru generycznego, albo
`Vec<Box<dyn TelemetrySink>>`. Subskrypcja `watch` daje to samo bez dotykania
collectora i jest już w repo używana przez SSE.

**Ręczny POST na `tokio::net::TcpStream`.** W pełni asynchroniczny, bez dodatkowego
wątku i stosu, i testowalny na hoście przeciw prawdziwemu listenerowi. Odrzucony
wyłącznie dlatego, że nie obsłuży TLS — a ten jest wymagany. Gdyby cel kiedyś
przeniósł się do LAN-u bez TLS, to jest wariant, do którego warto wrócić.

**Bufor na flash.** Przetrwałby restart kontrolera, ale wymaga własnej partycji
i ostrożności z wear-levelingiem. Osobny spec, jeśli okaże się potrzebny.

**MQTT zamiast zapisu po HTTP.** `esp-idf-svc` ma klienta MQTT, a Telegraf potrafi
z niego czytać i pisać do Influxa. Odrzucone, bo dokłada brokera i Telegrafa jako
dwa dodatkowe elementy, które mogą paść — a zapisu po HTTP potrzebujemy i tak.

## Konwencja

Kod i testy po angielsku, dokumentacja po polsku. **Nie commitować.**
