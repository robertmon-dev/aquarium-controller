# Sterowanie światłem przez API — override harmonogramu

Data: 2026-10-28
Status: zaakceptowany, gotowy do rozpisania planu

## Problem

`LightingSchedule` liczy profil oświetlenia z tabeli wschodów i zachodów słońca na
dany miesiąc. Wynik trafia do `TankTelemetryDto.lighting` i dalej do `/api/telemetry`
oraz SSE. Jest to wyłącznie odczyt — nie da się z zewnątrz zmienić stanu światła.

Celem jest umożliwienie klientowi API nadpisania harmonogramu: wymuszenia
konkretnego profilu albo surowych poziomów kanałów, czasowo lub bezterminowo.

## Zakres

W zakresie: stan logiczny oświetlenia (domena, store, HTTP, telemetria).

Poza zakresem: sterownik sprzętowy. W repo nie ma dziś żadnego wyjścia PWM/LEDC —
oświetlenie jest tylko liczone i raportowane. Port `LightingOverrideSource`
powstający w tej pracy jest punktem zaczepienia, w który sterownik LEDC wepnie się
później bez zmiany tego projektu.

## Ograniczenia wynikające z zależności

Trzy rzeczy ustalone przez czytanie źródeł `picoserve 0.20.1` i `serde-json-core 0.6.0`,
które kształtują kontrakt:

1. **Brak `deserialize_any` w `serde_json_core`** (`de/mod.rs:387`, `Error::AnyIsUnsupported`).
   Wyklucza to `#[serde(untagged)]`, tag wewnętrzny `#[serde(tag = "...")]` oraz
   `#[serde(flatten)]`. Działa enum tagowany zewnętrznie (domyślny serde) — i to on
   wymusza kształt pola `target`. Dodatkowo brak alokacji oznacza zakaz pól `String`
   w ciele żądania.
2. **`picoserve` ma `post`/`put`/`delete`** (`routing.rs:471`,
   `generate_method_router!`) oraz `FromRef` (`extract.rs:503`), co pozwala na złożony
   stan routera bez obejść. `IntoResponse for Result<T, E>` (`response.rs:684`) i
   `StatusCode::NO_CONTENT` / `BAD_REQUEST` też istnieją.
3. **Collector tyka co 30 s.** Bez dodatkowego mechanizmu zmiana przez API byłaby
   widoczna w telemetrii i SSE dopiero po pełnym ticku.

## Model domenowy

W `core/src/domain/light.rs`:

```rust
pub struct ChannelLevels { pub white_pct: u8, pub rgb_pct: u8 }   // dozwolone 0..=100

#[serde(rename_all = "snake_case")]
pub enum LightingTarget {
    Profile(LightingProfile),
    Channels(ChannelLevels),
}

pub struct LightingOverride {
    pub target: LightingTarget,
    pub expires_in_s: Option<u32>,   // REPORTED field, never the comparison basis
}
```

### Mapowanie profilu na poziomy kanałów

Dozwolone jest każde z pięciu wariantów `LightingProfile`. Rampy mapują się na swój
punkt końcowy:

| Profil | white_pct | rgb_pct |
|---|---|---|
| `Off` | 0 | 0 |
| `Sunrise` | 60 | 40 |
| `Daylight` | 60 | 40 |
| `Sunset` | 60 | 40 |
| `Moonlight` | 0 | 5 |

`Moonlight` nie miał dotąd przypisanych wartości nigdzie w repo — powyższe to nowa
definicja (ciemny nocny RGB, biały wygaszony).

Wartości 60/40 są dziś wpisane na sztywno w `LightingSchedule::evaluate`. Zostają
wyciągnięte do stałych `DAYLIGHT_WHITE_PCT` i `DAYLIGHT_RGB_PCT`, używanych zarówno
przez harmonogram, jak i przez mapowanie override'u. Bez tego ta sama liczba żyje
w dwóch miejscach i rozjedzie się przy pierwszej zmianie.

### Scalanie

```rust
pub fn resolve(
    scheduled: LightingMetricsDto,
    over: Option<&LightingOverride>,
) -> LightingMetricsDto
```

Brak override'u zwraca `scheduled` bez zmian. Obecny override wygrywa. Funkcja nie
przyjmuje czasu — wygasanie jest w całości odpowiedzialnością store'u (patrz niżej),
więc `None` oznacza tu równocześnie „brak" i „wygasł".

### Zmiana w telemetrii

Do `LightingMetricsDto` dochodzi `override_active: bool`. Jest to dodatek — istniejące
pola (`profile`, `channel_white_pct`, `channel_rgb_pct`) zachowują znaczenie, więc
zmiana JSON-a jest wstecznie zgodna dla obecnych konsumentów.

`profile` raportuje stan **efektywny**: dla override'u typu `Profile` jest to wybrany
profil, dla override'u typu `Channels` — profil wyliczony z harmonogramu, bo surowe
kanały nie odpowiadają żadnemu profilowi. `override_active` jest jedynym rozstrzygającym
sygnałem, że harmonogram nie rządzi.

## Porty

Dwa nowe pliki, zgodne z istniejącym podziałem inbound/outbound:

```rust
// core/src/ports/inbound/lighting.rs — konsument: HTTP
pub trait LightingControl {
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>);
    fn clear_override(&self);
    fn current_override(&self) -> Option<LightingOverride>;
}

// core/src/ports/outbound/lighting.rs — konsument: collector
pub trait LightingOverrideSource {
    fn active_override(&self) -> Option<LightingOverride>;
}
```

Metoda outbound nazywa się `active_override`, a nie `current_override`, **celowo**.
`InMemoryLightingStore` implementuje oba traity; gdyby obie metody nazywały się tak
samo, każde wywołanie `store.current_override()` byłoby w Ruscie niejednoznaczne
i wymagałoby składni `LightingControl::current_override(&store)`. Rozważany supertrait
(`LightingControl: LightingOverrideSource`) też by to usunął, ale sprzęgałby port
wejściowy z wyjściowym — rozróżnienie nazw jest tańsze.

Sygnał budzenia (`Notify`) celowo **nie** wchodzi do żadnego z traitów — to detal
mechaniki odświeżania, nie część kontraktu „źródło override'u".

## Store

`InMemoryLightingStore` w `core/src/adapters/outbound/memory.rs`, implementujący oba
porty, plus forwardy dla `Arc<InMemoryLightingStore>` — dokładnie tak, jak istniejący
`InMemoryTelemetryStore` robi to dla `TelemetrySink`.

### Wygasanie

Store trzyma `Option<tokio::time::Instant>` jako moment wygaśnięcia i sam ocenia
ważność: **obie** metody odczytu — `LightingControl::current_override`
i `LightingOverrideSource::active_override` — zwracają `None`, gdy czas minął.
HTTP i collector widzą więc dokładnie ten sam stan.

Rozważana i **odrzucona** alternatywa: TTL liczony na `Clock::uptime_seconds()`.
`SystemClock` posiada `EspSntp<'static>` i nie jest klonowalny, więc współdzielenie go
ze storem wymagałoby `Arc<SystemClock>` oraz nowego `impl Clock for Arc<C>` — sporo
plumbingu dla jednej liczby. Gorsze jest jednak ryzyko merytoryczne: przy dwóch
źródłach czasu o różnych punktach zerowych łatwo porównać niewłaściwą parę. Trzymanie
całej arytmetyki czasu w jednym typie to ryzyko eliminuje.

`Clock::epoch_seconds()` nie nadaje się na podstawę TTL niezależnie od powyższego —
zwraca `None`, dopóki SNTP się nie zsynchronizuje.

`expires_in_s` jest wyliczane przez store przy każdym odczycie jako pozostały czas.

## Warstwa HTTP

### Stan routera

```rust
pub struct AppState<Q, L> { pub telemetry: Arc<Q>, pub lighting: Arc<L> }
pub struct LightingState<L>(pub Arc<L>);

impl<Q, L> FromRef<AppState<Q, L>> for Arc<Q> { /* Arc::clone(&input.telemetry) */ }
impl<Q, L> FromRef<AppState<Q, L>> for LightingState<L> { /* Arc::clone(&input.lighting) */ }
```

Światła wychodzą przez **nowy typ opakowujący `LightingState<L>`, nie przez gołe
`Arc<L>`** — i to jest wymóg kompilatora, nie estetyka. Para impl-ów
`FromRef<AppState<Q, L>> for Arc<Q>` oraz `FromRef<AppState<Q, L>> for Arc<L>`
nachodzi na siebie dla `Q == L`, więc coherence checker Rusta ją odrzuca. Dwa różne
konstruktory typu (`Arc<_>` i `LightingState<_>`) nie mogą być nigdy równe, więc
konflikt znika.

Nie koliduje to też z blanketem picoserve `impl<T: Clone> FromRef<T> for T`: tamten
dotyczy `FromRef<Arc<Q>>`, nasz `FromRef<AppState<Q, L>>` — nakładka wymagałaby
`AppState<Q, L> == Arc<Q>`, co jest niemożliwe.

Istniejące handlery (`telemetry::snapshot`, `health::probe`, `events::stream`)
**nie zmieniają sygnatur** — dalej przyjmują `State<Arc<Q>>`. Opakowanie dotyka
wyłącznie nowych handlerów świateł.

### Trasy

Jedna ścieżka, trzy metody, wszystkie na `/api/lighting/override`:

| Metoda | Sukces | Uwagi |
|---|---|---|
| `GET` | `200` + JSON override'u | `204 No Content`, gdy brak albo wygasł |
| `PUT` | `200` + JSON ustawionego stanu | `400` przy niepoprawnym ciele |
| `DELETE` | `204 No Content` | idempotentne |

`PUT` zamiast `POST`, bo to idempotentne zastąpienie całego stanu override'u, a nie
tworzenie zasobu w kolekcji.

### Ciało żądania PUT

```json
{ "target": { "profile": "daylight" }, "duration_s": 3600 }
{ "target": { "channels": { "white_pct": 80, "rgb_pct": 20 } } }
```

`duration_s` jest opcjonalne; pominięte oznacza override bezterminowy (serde domyślnie
daje `None` dla brakującego pola `Option<T>`). Podane musi być **większe od zera** —
`"duration_s": 0` to żądanie sprzeczne (ustaw i natychmiast wygaś) i zwraca `400`,
zamiast tworzyć override, którego następny odczyt i tak nie zobaczy.

### Obsługa błędów

Niepoprawny JSON obsługuje sam picoserve — `JsonRejection` ma `#[status_code(BAD_REQUEST)]`.

Dwie reguły własne, obie egzekwowane w handlerze przez wywołanie walidacji z domeny:
`white_pct` i `rgb_pct` są typu `u8`, więc typ przepuszcza wartości 101–255
(`"channel level out of range\n"`), oraz `duration_s` musi być > 0
(`"duration must be positive\n"`). Obie zwracają `StatusCode::BAD_REQUEST`. Format odpowiedzi
błędu naśladuje istniejące `"unauthorized\n"` z `ApiTokenLayer`.

### Autoryzacja

`ApiTokenLayer` opakowuje cały router, więc nowe trasy są chronione bez zmian w nim.

Furtka pozwalająca podać token w query stringu jest zahardkodowana wyłącznie na
`/api/events` (`auth.rs:7`, `SSE_PATH`). Zostaje tak celowo: `PUT` i `DELETE` wymagają
nagłówka `Authorization: Bearer`, dzięki czemu token sterujący nie trafia do logów
proxy ani do historii przeglądarki.

### Pliki

- nowy `core/src/adapters/inbound/http/handlers/lighting.rs`
- zmiany w `routes.rs` (stan złożony, nowe trasy) i `http/mod.rs` (`run` przyjmuje
  dodatkowo store świateł)

## Collector

`TelemetryCollector<R, C, H, S, O>` zyskuje piąty generyk `O: LightingOverrideSource`.

`evaluate_lighting()` liczy harmonogram jak dotąd, po czym przepuszcza wynik przez
`light::resolve(scheduled, overrides.active_override().as_ref())`. Collector nie
dotyka TTL.

### Natychmiastowe odświeżenie

Collector cache'uje ostatnie odczyty czujników i zyskuje drugą metodę:

```rust
pub async fn sample(&mut self)   // czyta DS18B20 -> cache -> publish
pub fn republish(&self)          // przelicza z cache'u -> publish; bez magistrali, bez async
```

Pętla w `src/main.rs`:

```rust
tokio::select! {
    _ = ticker.tick()     => collector.sample().await,
    _ = notify.notified() => collector.republish(),
}
```

Dwie decyzje:

- **`notify_one()`, nie `notify_waiters()`.** `notify_waiters()` budzi wyłącznie
  zarejestrowanych oczekujących, więc żądanie przychodzące w trakcie `sample().await`
  (konwersja DS18B20 trwa ~750 ms) zostałoby zgubione. `notify_one()` zostawia permit.
- **`republish()` nie czyta czujników.** Seria żądań z API nie może zamienić się
  w młócenie magistrali 1-Wire. Gdy ostatni odczyt padł, cache zawiera `None`
  i telemetria uczciwie raportuje brak odczytu, zamiast podawać stare dane jako świeże.

`Arc<Notify>` wychodzi ze store'u metodą inherentną i jest wstrzykiwany do
`sensor_task` obok collectora.

## Testy

Wszystkie uruchamialne na hoście przez **`make test`**.

Uwaga: samo `cargo test -p aquarium-core` **nie zadziała** — `.cargo/config.toml`
wymusza `target = "xtensa-esp32s3-espidf"`, przez co linkowanie pada na `ldproxy`
(„Cannot locate argument '--ldproxy-linker'"). Cel `test` w Makefile dokłada
`--target x86_64-unknown-linux-gnu`. Baseline przed zmianami: 58 testów jednostkowych
i 8 integracyjnych, wszystkie zielone.

**Domena (`light.rs`)**
- `resolve` oddaje pierwszeństwo override'owi
- brak override'u zwraca harmonogram bez zmian
- mapowanie każdego z pięciu profili na poziomy kanałów
- walidacja odrzuca poziom > 100 i `duration_s` równe 0
- `override_active` ustawiane poprawnie w obu kierunkach

**Store**
- set / clear / current
- wygaśnięcie zwraca `None` pod `tokio::time::pause()` + `advance()`
- `expires_in_s` maleje z upływem czasu
- `notify` odpala przy `set_override` i przy `clear_override`

**Collector**
- override wygrywa z harmonogramem
- wygasły override wraca do harmonogramu
- `republish()` nie woła czujników — fake reader zliczający wywołania

**Integracja HTTP (`core/tests/http_api.rs`)**
- `PUT` bez tokenu → 401
- token w query na `PUT` → 401 (furtka ma zostać wyłącznie na SSE)
- niepoprawny JSON → 400
- `white_pct: 150` → 400
- `duration_s: 0` → 400
- `PUT` profilu → 200, a `GET /api/telemetry` pokazuje nowy stan
- `DELETE` → 204 i powrót do harmonogramu
- `GET` bez override'u → 204

Helper `start()` w `http_api.rs` przyjmuje dziś `Arc<InMemoryTelemetryStore>` —
dostanie drugi argument ze store'em świateł.

## Odrzucone alternatywy

**Override trzymany w `InMemoryTelemetryStore`.** Mniej typów i żadnego nowego pliku,
ale store telemetrii (dziś: „ostatni snapshot do odczytu") stałby się równocześnie
magazynem komend. `TelemetryQuery` przestałby znaczyć to, co znaczy, a przy dokładaniu
CO2, ATO i grzałki ten sam skrót trzeba by powtórzyć albo cofnąć.

**Kanał komend `mpsc` z handlera do collectora.** Jednokierunkowy przepływ, ale
`GET /api/lighting/override` i tak potrzebuje odczytu stanu, więc kanał nie zastępuje
store'u, tylko go dubluje. Dodatkowo odpowiedź na `PUT` nie mogłaby potwierdzić stanu,
który dopiero zostanie zastosowany.

## Konwencja językowa

Kod jest po angielsku — nazwy testów, komunikaty asercji, komentarze i stringi
w `expect`/`Err`. Część istniejących testów ma jeszcze polskie komunikaty; to
zaszłość czyszczona osobnym refaktorem, a nie przy okazji tej funkcjonalności.
Dokumentacja (ten spec i plan) zostaje po polsku.

## Praca poza zakresem tego specu

Dwa osobne zadania, oba zgłoszone przez właściciela, oba do wykonania PO tej
funkcjonalności i nie razem z nią — to czyste refaktory bez zmiany zachowania:

1. Wyeksportowanie testów z inline `#[cfg(test)] mod tests` do osobnych plików.
2. Przepisanie pozostałych polskich komunikatów asercji w istniejących testach
   na angielskie.

Ponieważ dotyczą tego samego kodu testowego, powinny trafić do jednego planu.
