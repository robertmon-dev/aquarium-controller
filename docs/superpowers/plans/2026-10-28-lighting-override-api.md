# Override oświetlenia przez HTTP API — plan implementacji

> **Dla agentów:** WYMAGANY SUB-SKILL: `superpowers:subagent-driven-development`
> (zalecany) albo `superpowers:executing-plans`. Kroki mają składnię checkboxów
> (`- [ ]`) do śledzenia postępu.

**Cel:** Umożliwić klientowi HTTP API nadpisanie harmonogramu słonecznego — wymuszenie
profilu albo surowych poziomów kanałów, czasowo lub bezterminowo.

**Architektura:** Override mieszka we własnym store'rze (`InMemoryLightingStore`),
wystawionym przez dwa porty: `LightingControl` (dla HTTP) i `LightingOverrideSource`
(dla collectora). Czysta funkcja `light::resolve` scala harmonogram z override'em.
Store sam ocenia wygasanie na `tokio::time::Instant`, więc domena nie zna czasu,
a collector nie zna TTL. Zmiana przez API budzi collector przez `Notify`, żeby
telemetria i SSE nie czekały do 30 s na następny tick.

**Stack:** Rust 2021, `picoserve 0.20.1` (HTTP), `serde-json-core 0.6.0` (JSON),
`tokio` (sync/time), ESP32-S3 przez `esp-idf-svc`.

**Spec:** `docs/superpowers/specs/2026-10-28-lighting-override-api-design.md`

---

## ZANIM ZACZNIESZ — przeczytaj

**NIE COMMITUJ NICZEGO.** Właściciel repozytorium prowadzi historię gita sam.
Każdy task kończy się weryfikacją, nie commitem. Nie wołaj `git add`, `git commit`,
`git push` ani `git stash`.

**Komenda testowa to `make test`, nie `cargo test`.** Plik `.cargo/config.toml`
wymusza `target = "xtensa-esp32s3-espidf"`, więc samo `cargo test -p aquarium-core`
pada na linkowaniu (`ldproxy: Cannot locate argument '--ldproxy-linker'`).
Cel `test` z Makefile dokłada `--target x86_64-unknown-linux-gnu`.

**Baseline przed zmianami:** `make test` → 58 testów jednostkowych + 8 integracyjnych,
wszystkie zielone. Po każdym tasku ta liczba ma rosnąć, a zero ma zostać zerem.

**Jedna przerwa w kompilacji firmware.** Task 5 zmienia liczbę parametrów generycznych
`TelemetryCollector`, przez co crate `aquarium-controller` (firmware) przestaje się
kompilować aż do Taska 7. To jest zamierzone i akceptowalne, bo `make test` buduje
wyłącznie `aquarium-core`. Nie próbuj naprawiać `src/main.rs` wcześniej niż w Tasku 7.

**Język kodu: angielski.** Nazwy testów, komunikaty asercji, komentarze
i stringi w `expect`/`Err` — wszystko po angielsku. Część istniejących testów ma
jeszcze polskie komunikaty (np. `"niebezpieczna temperatura MUSI dotrzeć do
telemetrii"`); to zaszłość, którą czyści osobny refaktor, nie ten plan. Nie pisz
nowych polskich stringów i nie przerabiaj przy okazji starych — to osobna robota.

---

## Struktura plików

| Plik | Odpowiedzialność | Task |
|---|---|---|
| `core/src/domain/light.rs` | typy override'u, mapowanie profil→poziomy, `resolve` | 1, 2 |
| `core/src/domain/metrics.rs` | pole `override_active` w `LightingMetricsDto` | 2 |
| `core/src/ports/inbound/lighting.rs` | **nowy** — trait `LightingControl` | 3 |
| `core/src/ports/outbound/lighting.rs` | **nowy** — trait `LightingOverrideSource` | 3 |
| `core/src/adapters/outbound/memory.rs` | `InMemoryLightingStore` + wygasanie + `Notify` | 4 |
| `core/src/application/collector.rs` | piąty generyk, cache odczytów, `republish` | 5 |
| `core/src/adapters/inbound/http/handlers/lighting.rs` | **nowy** — trzy handlery | 6 |
| `core/src/adapters/inbound/http/routes.rs` | `AppState`, `FromRef`, nowe trasy | 6 |
| `core/src/adapters/inbound/http/mod.rs` | `run` przyjmuje store świateł | 6 |
| `core/tests/http_api.rs` | testy integracyjne nowych tras | 6 |
| `src/main.rs` | złożenie zależności, pętla `select!` | 7 |
| `Makefile` | rozszerzony `make smoke` | 8 |

---

## Task 1: Typy domenowe i mapowanie profilu na poziomy

**Pliki:**
- Modyfikuj: `core/src/domain/light.rs`

- [ ] **Krok 1: Dopisz testy na końcu `mod tests` w `core/src/domain/light.rs`**

Wstaw przed zamykającym nawiasem `mod tests`:

```rust
    #[test]
    fn channel_levels_accept_the_full_legal_range() {
        assert!(ChannelLevels::new(0, 0).validate().is_ok());
        assert!(ChannelLevels::new(100, 100).validate().is_ok());
        assert!(ChannelLevels::new(60, 40).validate().is_ok());
    }

    #[test]
    fn channel_levels_reject_anything_above_one_hundred() {
        assert!(ChannelLevels::new(101, 0).validate().is_err());
        assert!(ChannelLevels::new(0, 101).validate().is_err());
        assert!(ChannelLevels::new(255, 255).validate().is_err());
    }

    #[test]
    fn every_profile_maps_to_concrete_levels() {
        assert_eq!(LightingProfile::Off.static_levels(), ChannelLevels::new(0, 0));
        assert_eq!(
            LightingProfile::Daylight.static_levels(),
            ChannelLevels::new(DAYLIGHT_WHITE_PCT, DAYLIGHT_RGB_PCT)
        );
        assert_eq!(
            LightingProfile::Moonlight.static_levels(),
            ChannelLevels::new(MOONLIGHT_WHITE_PCT, MOONLIGHT_RGB_PCT)
        );
    }

    #[test]
    fn ramps_map_to_their_endpoint_which_equals_daylight() {
        assert_eq!(
            LightingProfile::Sunrise.static_levels(),
            LightingProfile::Daylight.static_levels(),
            "sunrise as a static target is its endpoint"
        );
        assert_eq!(
            LightingProfile::Sunset.static_levels(),
            LightingProfile::Daylight.static_levels()
        );
    }

    #[test]
    fn target_levels_follow_the_variant() {
        assert_eq!(
            LightingTarget::Profile(LightingProfile::Off).levels(),
            ChannelLevels::new(0, 0)
        );
        assert_eq!(
            LightingTarget::Channels(ChannelLevels::new(80, 20)).levels(),
            ChannelLevels::new(80, 20)
        );
    }

    #[test]
    fn a_profile_target_is_always_valid_but_channels_are_checked() {
        assert!(LightingTarget::Profile(LightingProfile::Moonlight)
            .validate()
            .is_ok());
        assert!(LightingTarget::Channels(ChannelLevels::new(100, 100))
            .validate()
            .is_ok());
        assert!(LightingTarget::Channels(ChannelLevels::new(101, 0))
            .validate()
            .is_err());
    }
```

- [ ] **Krok 2: Uruchom testy i potwierdź, że NIE kompilują się**

Uruchom: `make test`

Oczekiwane: błąd kompilacji, `cannot find struct, variant or union type 'ChannelLevels'`
oraz `no function or associated item named 'static_levels'`. To jest poprawny stan —
test opisuje coś, czego jeszcze nie ma.

- [ ] **Krok 3: Dopisz typy i stałe na początku `core/src/domain/light.rs`**

Zamień istniejącą pierwszą linię pliku:

```rust
use super::metrics::{LightingMetricsDto, LightingProfile};
```

na:

```rust
use serde::{Deserialize, Serialize};

use super::metrics::{LightingMetricsDto, LightingProfile};

/// Channel levels at full daylight. Used by both the schedule and the
/// profile-to-target mapping: one number, one place.
pub const DAYLIGHT_WHITE_PCT: u8 = 60;
pub const DAYLIGHT_RGB_PCT: u8 = 40;

/// Night light: white off, RGB barely glowing.
pub const MOONLIGHT_WHITE_PCT: u8 = 0;
pub const MOONLIGHT_RGB_PCT: u8 = 5;

pub const MAX_CHANNEL_PCT: u8 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelLevels {
    pub white_pct: u8,
    pub rgb_pct: u8,
}

impl ChannelLevels {
    pub const fn new(white_pct: u8, rgb_pct: u8) -> Self {
        Self {
            white_pct,
            rgb_pct,
        }
    }

    /// `u8` admits 101..=255, so the type alone is not validation.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.white_pct > MAX_CHANNEL_PCT || self.rgb_pct > MAX_CHANNEL_PCT {
            return Err("channel level out of range");
        }

        Ok(())
    }
}

/// Override target. EXTERNALLY tagged enum (serde's default): `serde_json_core`
/// has no `deserialize_any`, so `untagged`, internal tags and `flatten` are out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightingTarget {
    Profile(LightingProfile),
    Channels(ChannelLevels),
}

impl LightingTarget {
    pub fn levels(&self) -> ChannelLevels {
        match self {
            Self::Profile(profile) => profile.static_levels(),
            Self::Channels(levels) => *levels,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Profile(_) => Ok(()),
            Self::Channels(levels) => levels.validate(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightingOverride {
    pub target: LightingTarget,
    /// Seconds remaining. A REPORTED field, never the basis of a comparison:
    /// expiry is judged by the store alone.
    pub expires_in_s: Option<u32>,
}

impl LightingProfile {
    /// Channel levels when the profile is a static override target.
    /// Ramps (`Sunrise`, `Sunset`) map to their endpoint.
    pub const fn static_levels(self) -> ChannelLevels {
        match self {
            Self::Off => ChannelLevels::new(0, 0),
            Self::Sunrise | Self::Daylight | Self::Sunset => {
                ChannelLevels::new(DAYLIGHT_WHITE_PCT, DAYLIGHT_RGB_PCT)
            }
            Self::Moonlight => ChannelLevels::new(MOONLIGHT_WHITE_PCT, MOONLIGHT_RGB_PCT),
        }
    }
}
```

- [ ] **Krok 4: Uruchom testy i potwierdź, że przechodzą**

Uruchom: `make test`

Oczekiwane: `test result: ok` dla biblioteki, **64 passed** (58 z baseline + 6 nowych),
0 failed. Testy integracyjne dalej 8 passed.

- [ ] **Krok 5: Sprawdź lint i formatowanie**

Uruchom: `make fmt-check && make lint-core`

Oczekiwane: oba przechodzą bez ostrzeżeń. Jeśli `fmt-check` zgłosi różnice, uruchom
`cargo fmt --all` i powtórz.

---

## Task 2: Scalanie harmonogramu z override'em

**Pliki:**
- Modyfikuj: `core/src/domain/metrics.rs` (pole `override_active`)
- Modyfikuj: `core/src/domain/light.rs` (funkcja `resolve`, użycie stałych)

- [ ] **Krok 1: Dodaj pole `override_active` do `LightingMetricsDto`**

W `core/src/domain/metrics.rs` zamień:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LightingMetricsDto {
    pub profile: LightingProfile,
    pub channel_white_pct: u8,
    pub channel_rgb_pct: u8,
}
```

na:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LightingMetricsDto {
    pub profile: LightingProfile,
    pub channel_white_pct: u8,
    pub channel_rgb_pct: u8,
    /// `true` when the schedule was overridden through the API. For a `Channels`
    /// target `profile` still shows the scheduled value, so THIS is the only
    /// conclusive signal that the schedule is not in charge.
    pub override_active: bool,
}
```

- [ ] **Krok 2: Uzupełnij konstrukcję w `TankTelemetryDto::empty`**

W tym samym pliku, w `TankTelemetryDto::empty()`, zamień:

```rust
            lighting: LightingMetricsDto {
                profile: LightingProfile::Off,
                channel_white_pct: 0,
                channel_rgb_pct: 0,
            },
```

na:

```rust
            lighting: LightingMetricsDto {
                profile: LightingProfile::Off,
                channel_white_pct: 0,
                channel_rgb_pct: 0,
                override_active: false,
            },
```

- [ ] **Krok 3: Uruchom testy i zobacz, które miejsca konstrukcji pękły**

Uruchom: `make test`

Oczekiwane: błędy kompilacji `missing field 'override_active' in initializer of
'LightingMetricsDto'` w `core/src/domain/light.rs` (cztery miejsca w `evaluate`)
oraz w `core/src/application/collector.rs` (jedno miejsce w `evaluate_lighting`).
To jest mapa roboty na kolejne kroki.

- [ ] **Krok 4: Napraw `LightingSchedule::evaluate` i wprowadź stałe**

W `core/src/domain/light.rs` zamień całe ciało `evaluate` (od `if current_minute <
sunrise_min` do zamykającego nawiasu metody) na:

```rust
        if current_minute < sunrise_min || current_minute >= night_start_min {
            LightingMetricsDto {
                profile: LightingProfile::Off,
                channel_white_pct: 0,
                channel_rgb_pct: 0,
                override_active: false,
            }
        } else if current_minute >= sunrise_min && current_minute < daylight_start_min {
            let elapsed = current_minute - sunrise_min;
            let progress = (elapsed as f32 / 60.0).clamp(0.0, 1.0);

            LightingMetricsDto {
                profile: LightingProfile::Sunrise,
                channel_white_pct: (progress * DAYLIGHT_WHITE_PCT as f32) as u8,
                channel_rgb_pct: (progress * DAYLIGHT_RGB_PCT as f32) as u8,
                override_active: false,
            }
        } else if current_minute >= daylight_start_min && current_minute < sunset_start_min {
            LightingMetricsDto {
                profile: LightingProfile::Daylight,
                channel_white_pct: DAYLIGHT_WHITE_PCT,
                channel_rgb_pct: DAYLIGHT_RGB_PCT,
                override_active: false,
            }
        } else {
            let elapsed = current_minute - sunset_start_min;
            let progress = 1.0 - (elapsed as f32 / 60.0).clamp(0.0, 1.0);

            LightingMetricsDto {
                profile: LightingProfile::Sunset,
                channel_white_pct: (progress * DAYLIGHT_WHITE_PCT as f32) as u8,
                channel_rgb_pct: (progress * DAYLIGHT_RGB_PCT as f32) as u8,
                override_active: false,
            }
        }
```

Liczby `60.0` i `40.0` znikają na rzecz stałych. Wartości wynikowe są identyczne,
więc istniejące testy harmonogramu muszą dalej przechodzić bez zmian — jeśli któryś
padnie, popełniłeś błąd w przepisaniu.

- [ ] **Krok 5: Napraw tymczasowo `collector.rs`, żeby crate się kompilował**

W `core/src/application/collector.rs`, w `evaluate_lighting`, zamień:

```rust
            _ => LightingMetricsDto {
                profile: LightingProfile::Off,
                channel_white_pct: 0,
                channel_rgb_pct: 0,
            },
```

na:

```rust
            _ => LightingMetricsDto {
                profile: LightingProfile::Off,
                channel_white_pct: 0,
                channel_rgb_pct: 0,
                override_active: false,
            },
```

Pełne wpięcie override'u w collector przychodzi w Tasku 5.

- [ ] **Krok 6: Uruchom testy i potwierdź powrót do zieleni**

Uruchom: `make test`

Oczekiwane: 64 passed, 0 failed — dokładnie tyle co po Tasku 1. Ten krok nie dodaje
testów, tylko przywraca kompilację.

- [ ] **Krok 7: Dopisz testy `resolve` w `mod tests` w `core/src/domain/light.rs`**

```rust
    fn scheduled_daylight() -> LightingMetricsDto {
        LightingSchedule::new().evaluate(600, JUNE)
    }

    #[test]
    fn without_an_override_the_schedule_passes_through_untouched() {
        let scheduled = scheduled_daylight();
        let resolved = resolve(scheduled.clone(), None);
        assert_eq!(resolved, scheduled);
        assert!(!resolved.override_active);
    }

    #[test]
    fn a_profile_override_replaces_profile_and_levels() {
        let over = LightingOverride {
            target: LightingTarget::Profile(LightingProfile::Off),
            expires_in_s: None,
        };

        let resolved = resolve(scheduled_daylight(), Some(&over));

        assert_eq!(resolved.profile, LightingProfile::Off);
        assert_eq!(resolved.channel_white_pct, 0);
        assert_eq!(resolved.channel_rgb_pct, 0);
        assert!(resolved.override_active);
    }

    #[test]
    fn a_channels_override_keeps_the_scheduled_profile() {
        let over = LightingOverride {
            target: LightingTarget::Channels(ChannelLevels::new(80, 20)),
            expires_in_s: None,
        };

        let resolved = resolve(scheduled_daylight(), Some(&over));

        assert_eq!(
            resolved.profile,
            LightingProfile::Daylight,
            "raw channels match no profile, so the scheduled profile stays"
        );
        assert_eq!(resolved.channel_white_pct, 80);
        assert_eq!(resolved.channel_rgb_pct, 20);
        assert!(
            resolved.override_active,
            "override_active is the ONLY signal that the schedule is not in charge"
        );
    }

    #[test]
    fn an_override_wins_even_when_the_schedule_says_off() {
        let night = LightingSchedule::new().evaluate(60, JUNE);
        assert_eq!(night.profile, LightingProfile::Off);

        let over = LightingOverride {
            target: LightingTarget::Profile(LightingProfile::Moonlight),
            expires_in_s: Some(120),
        };

        let resolved = resolve(night, Some(&over));

        assert_eq!(resolved.profile, LightingProfile::Moonlight);
        assert_eq!(resolved.channel_rgb_pct, MOONLIGHT_RGB_PCT);
        assert!(resolved.override_active);
    }
```

- [ ] **Krok 8: Uruchom testy i potwierdź, że NIE kompilują się**

Uruchom: `make test`

Oczekiwane: `cannot find function 'resolve' in this scope`.

- [ ] **Krok 9: Dopisz `resolve` w `core/src/domain/light.rs`**

Wstaw bezpośrednio przed `impl Default for LightingSchedule`:

```rust
/// Merges the schedule result with an optional override.
///
/// Takes no time value: expiry is entirely the store's job, so `None` here means
/// both "no override" and "the override expired".
pub fn resolve(
    scheduled: LightingMetricsDto,
    over: Option<&LightingOverride>,
) -> LightingMetricsDto {
    let Some(over) = over else {
        return scheduled;
    };

    let levels = over.target.levels();

    LightingMetricsDto {
        profile: match over.target {
            LightingTarget::Profile(profile) => profile,
            LightingTarget::Channels(_) => scheduled.profile,
        },
        channel_white_pct: levels.white_pct,
        channel_rgb_pct: levels.rgb_pct,
        override_active: true,
    }
}
```

- [ ] **Krok 10: Uruchom testy i potwierdź, że przechodzą**

Uruchom: `make test`

Oczekiwane: **68 passed** (64 + 4 nowe), 0 failed. Integracyjne dalej 8 passed.

- [ ] **Krok 11: Lint i formatowanie**

Uruchom: `make fmt-check && make lint-core`

Oczekiwane: oba czyste.

---

## Task 3: Porty

**Pliki:**
- Utwórz: `core/src/ports/inbound/lighting.rs`
- Utwórz: `core/src/ports/outbound/lighting.rs`
- Modyfikuj: `core/src/ports/inbound/mod.rs`
- Modyfikuj: `core/src/ports/outbound/mod.rs`

Ten task nie dodaje testów — same definicje traitów. Weryfikacją jest kompilacja.

- [ ] **Krok 1: Utwórz `core/src/ports/inbound/lighting.rs`**

```rust
use crate::domain::light::{LightingOverride, LightingTarget};

/// Lighting override control. Consumer: the HTTP layer.
pub trait LightingControl {
    /// Replaces the current override. `duration_s` of `None` means open-ended.
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>);

    /// Clears the override; the schedule rules again. Idempotent.
    fn clear_override(&self);

    /// The current override, or `None` when absent or already expired.
    fn current_override(&self) -> Option<LightingOverride>;
}
```

- [ ] **Krok 2: Utwórz `core/src/ports/outbound/lighting.rs`**

```rust
use crate::domain::light::LightingOverride;

/// Override read path used when composing telemetry. Consumer: the collector.
///
/// The method is called `active_override`, NOT `current_override`, on purpose:
/// `InMemoryLightingStore` implements this trait alongside `LightingControl`, and
/// two identically named methods on one type would make every call ambiguous and
/// force `LightingControl::current_override(&store)` syntax.
pub trait LightingOverrideSource {
    fn active_override(&self) -> Option<LightingOverride>;
}
```

- [ ] **Krok 3: Zarejestruj moduł inbound**

Zamień całą zawartość `core/src/ports/inbound/mod.rs`:

```rust
pub mod lighting;
pub mod telemetry;

pub use lighting::LightingControl;
pub use telemetry::TelemetryQuery;
```

- [ ] **Krok 4: Zarejestruj moduł outbound**

Zamień całą zawartość `core/src/ports/outbound/mod.rs`:

```rust
pub mod clock;
pub mod lighting;
pub mod system_health;
pub mod telemetry;
pub mod temperature;

pub use clock::Clock;
pub use lighting::LightingOverrideSource;
pub use system_health::SystemHealthProvider;
pub use telemetry::TelemetrySink;
pub use temperature::{TemperatureError, TemperatureReader, TemperatureSensorRole};
```

- [ ] **Krok 5: Potwierdź kompilację i brak regresji**

Uruchom: `make test`

Oczekiwane: 68 passed, 0 failed — bez zmian względem Taska 2. Nowe traity nie mają
jeszcze implementacji, więc liczba testów nie rośnie.

- [ ] **Krok 6: Lint**

Uruchom: `make lint-core`

Oczekiwane: czysto. Clippy nie powinien narzekać na nieużywane traity, bo są `pub`
i reeksportowane.

---

## Task 4: Store override'u z wygasaniem i sygnałem budzenia

**Pliki:**
- Modyfikuj: `core/src/adapters/outbound/memory.rs`

- [ ] **Krok 1: Dopisz testy na końcu `mod tests` w `core/src/adapters/outbound/memory.rs`**

```rust
    mod lighting {
        use std::time::Duration;

        use super::super::InMemoryLightingStore;
        use crate::domain::light::{ChannelLevels, LightingTarget};
        use crate::domain::metrics::LightingProfile;
        use crate::ports::inbound::LightingControl;
        use crate::ports::outbound::LightingOverrideSource;

        fn moonlight() -> LightingTarget {
            LightingTarget::Profile(LightingProfile::Moonlight)
        }

        #[test]
        fn starts_with_no_override() {
            let store = InMemoryLightingStore::new();
            assert!(store.current_override().is_none());
            assert!(store.active_override().is_none());
        }

        #[test]
        fn set_then_read_returns_the_target() {
            let store = InMemoryLightingStore::new();
            store.set_override(moonlight(), None);

            let over = store.current_override().expect("override was just set");
            assert_eq!(over.target, moonlight());
            assert_eq!(over.expires_in_s, None, "a missing duration_s means open-ended");
        }

        #[test]
        fn setting_twice_replaces_rather_than_stacks() {
            let store = InMemoryLightingStore::new();
            store.set_override(moonlight(), None);
            store.set_override(LightingTarget::Channels(ChannelLevels::new(80, 20)), None);

            let over = store.current_override().expect("an override exists");
            assert_eq!(
                over.target,
                LightingTarget::Channels(ChannelLevels::new(80, 20))
            );
        }

        #[test]
        fn clear_removes_the_override() {
            let store = InMemoryLightingStore::new();
            store.set_override(moonlight(), None);
            store.clear_override();
            assert!(store.current_override().is_none());
        }

        #[test]
        fn clearing_nothing_does_not_panic() {
            let store = InMemoryLightingStore::new();
            store.clear_override();
            assert!(store.current_override().is_none());
        }

        #[tokio::test(start_paused = true)]
        async fn a_timed_override_expires_on_its_own() {
            let store = InMemoryLightingStore::new();
            store.set_override(moonlight(), Some(60));

            assert!(store.current_override().is_some(), "alive right after being set");

            tokio::time::advance(Duration::from_secs(61)).await;

            assert!(
                store.current_override().is_none(),
                "once the TTL elapses the override MUST disappear on its own"
            );
            assert!(
                store.active_override().is_none(),
                "the collector and HTTP must see exactly the same state"
            );
        }

        #[tokio::test(start_paused = true)]
        async fn remaining_time_counts_down() {
            let store = InMemoryLightingStore::new();
            store.set_override(moonlight(), Some(100));

            tokio::time::advance(Duration::from_secs(40)).await;

            let remaining = store
                .current_override()
                .expect("not expired yet")
                .expires_in_s
                .expect("a timed override reports its remaining time");

            assert!(
                (55..=60).contains(&remaining),
                "expected ~60 s, got {remaining}"
            );
        }

        #[tokio::test(start_paused = true)]
        async fn an_open_ended_override_never_expires() {
            let store = InMemoryLightingStore::new();
            store.set_override(moonlight(), None);

            tokio::time::advance(Duration::from_secs(86_400)).await;

            assert!(
                store.current_override().is_some(),
                "without a TTL the override survives a full day"
            );
        }

        #[tokio::test]
        async fn setting_an_override_wakes_the_collector() {
            let store = InMemoryLightingStore::new();
            let signal = store.change_signal();

            store.set_override(moonlight(), None);

            tokio::time::timeout(Duration::from_millis(100), signal.notified())
                .await
                .expect("notify_one leaves a permit, so waiting AFTER the signal still works");
        }

        #[tokio::test]
        async fn clearing_an_override_also_wakes_the_collector() {
            let store = InMemoryLightingStore::new();
            let signal = store.change_signal();

            store.clear_override();

            tokio::time::timeout(Duration::from_millis(100), signal.notified())
                .await
                .expect("returning to the schedule must refresh telemetry too");
        }
    }
```

- [ ] **Krok 2: Uruchom testy i potwierdź, że NIE kompilują się**

Uruchom: `make test`

Oczekiwane: `cannot find type 'InMemoryLightingStore'`.

- [ ] **Krok 3: Dopisz store w `core/src/adapters/outbound/memory.rs`**

Zamień nagłówek importów na początku pliku:

```rust
use std::sync::Arc;

use tokio::sync::watch;

use crate::domain::metrics::TankTelemetryDto;
use crate::ports::inbound::TelemetryQuery;
use crate::ports::outbound::TelemetrySink;
```

na:

```rust
use std::sync::{Arc, Mutex};

use tokio::sync::{watch, Notify};
use tokio::time::Instant;

use crate::domain::light::{LightingOverride, LightingTarget};
use crate::domain::metrics::TankTelemetryDto;
use crate::ports::inbound::{LightingControl, TelemetryQuery};
use crate::ports::outbound::{LightingOverrideSource, TelemetrySink};
```

Następnie wstaw przed `#[cfg(test)]` na końcu pliku:

```rust
/// What is actually stored. `expires_at` is an `Instant` rather than a second count
/// because all time arithmetic must live in one place: with two time sources that
/// have different origins it is easy to compare the wrong pair.
struct StoredOverride {
    target: LightingTarget,
    expires_at: Option<Instant>,
}

pub struct InMemoryLightingStore {
    state: Mutex<Option<StoredOverride>>,
    notify: Arc<Notify>,
}

impl InMemoryLightingStore {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(None),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Handle the collector waits on so telemetry refreshes immediately after
    /// a change instead of waiting for the next tick.
    pub fn change_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    /// Shared read path for both ports. An expired override is indistinguishable
    /// from no override at all.
    fn read(&self) -> Option<LightingOverride> {
        let guard = self.state.lock().expect("mutex poisoned by a panic");
        let stored = guard.as_ref()?;

        let expires_in_s = match stored.expires_at {
            None => None,
            Some(at) => {
                let now = Instant::now();
                if now >= at {
                    return None;
                }
                Some(at.saturating_duration_since(now).as_secs() as u32)
            }
        };

        Some(LightingOverride {
            target: stored.target,
            expires_in_s,
        })
    }
}

impl Default for InMemoryLightingStore {
    fn default() -> Self {
        Self::new()
    }
}

impl LightingControl for InMemoryLightingStore {
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>) {
        let expires_at = duration_s.map(|s| Instant::now() + std::time::Duration::from_secs(s as u64));

        *self.state.lock().expect("mutex poisoned by a panic") = Some(StoredOverride {
            target,
            expires_at,
        });

        // notify_one, not notify_waiters: a request arriving mid sensor read
        // (a DS18B20 conversion takes ~750 ms) must never be lost.
        self.notify.notify_one();
    }

    fn clear_override(&self) {
        *self.state.lock().expect("mutex poisoned by a panic") = None;
        self.notify.notify_one();
    }

    fn current_override(&self) -> Option<LightingOverride> {
        self.read()
    }
}

impl LightingOverrideSource for InMemoryLightingStore {
    fn active_override(&self) -> Option<LightingOverride> {
        self.read()
    }
}

impl LightingControl for Arc<InMemoryLightingStore> {
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>) {
        self.as_ref().set_override(target, duration_s);
    }

    fn clear_override(&self) {
        self.as_ref().clear_override();
    }

    fn current_override(&self) -> Option<LightingOverride> {
        self.as_ref().current_override()
    }
}

impl LightingOverrideSource for Arc<InMemoryLightingStore> {
    fn active_override(&self) -> Option<LightingOverride> {
        self.as_ref().active_override()
    }
}
```

- [ ] **Krok 4: Uruchom testy i potwierdź, że przechodzą**

Uruchom: `make test`

Oczekiwane: **78 passed** (68 + 10 nowych), 0 failed.

Jeśli `remaining_time_counts_down` pada na granicy zakresu, sprawdź czy używasz
`tokio::time::Instant` (sterowany przez `start_paused`), a nie `std::time::Instant`
(który `advance` ignoruje).

- [ ] **Krok 5: Lint i formatowanie**

Uruchom: `make fmt-check && make lint-core`

Oczekiwane: czysto.

---

## Task 5: Collector — wpięcie override'u i odświeżanie bez magistrali

**Pliki:**
- Modyfikuj: `core/src/application/collector.rs`

**Uwaga:** po tym tasku crate `aquarium-controller` (firmware) przestaje się kompilować,
bo `TelemetryCollector` zyskuje piąty parametr generyczny, a `src/main.rs` o nim nie wie.
Naprawa jest w Tasku 7. `make test` dotyczy wyłącznie `aquarium-core`, więc ma zostać zielony.

- [ ] **Krok 1: Dopisz atrapę źródła override'u w `mod tests`**

W `core/src/application/collector.rs`, w `mod tests`, dopisz po `struct FakeHealth`:

```rust
    struct FakeOverrides(Option<LightingOverride>);

    impl FakeOverrides {
        fn none() -> Self {
            Self(None)
        }

        fn forcing(target: LightingTarget) -> Self {
            Self(Some(LightingOverride {
                target,
                expires_in_s: None,
            }))
        }
    }

    impl LightingOverrideSource for FakeOverrides {
        fn active_override(&self) -> Option<LightingOverride> {
            self.0
        }
    }
```

Rozszerz import w `mod tests` — zamień:

```rust
    use crate::ports::outbound::{
        Clock, SystemHealthProvider, TelemetrySink, TemperatureError, TemperatureReader,
        TemperatureSensorRole,
    };
```

na:

```rust
    use crate::domain::light::{ChannelLevels, LightingOverride, LightingTarget};
    use crate::ports::outbound::{
        Clock, LightingOverrideSource, SystemHealthProvider, TelemetrySink, TemperatureError,
        TemperatureReader, TemperatureSensorRole,
    };
```

- [ ] **Krok 2: Przepisz helper `collector` i dopisz nowe testy**

Zamień istniejący helper:

```rust
    fn collector(
        reader: FakeReader,
        clock: FakeClock,
        sink: &RecordingSink,
    ) -> TelemetryCollector<FakeReader, FakeClock, FakeHealth, &RecordingSink> {
        TelemetryCollector::new(reader, clock, FakeHealth, sink, 27.0)
    }
```

na:

```rust
    fn collector(
        reader: FakeReader,
        clock: FakeClock,
        sink: &RecordingSink,
    ) -> TelemetryCollector<FakeReader, FakeClock, FakeHealth, &RecordingSink, FakeOverrides> {
        with_overrides(reader, clock, sink, FakeOverrides::none())
    }

    fn with_overrides(
        reader: FakeReader,
        clock: FakeClock,
        sink: &RecordingSink,
        overrides: FakeOverrides,
    ) -> TelemetryCollector<FakeReader, FakeClock, FakeHealth, &RecordingSink, FakeOverrides> {
        TelemetryCollector::new(reader, clock, FakeHealth, sink, overrides, 27.0)
    }
```

Dopisz na końcu `mod tests`:

```rust
    #[tokio::test]
    async fn an_override_beats_the_schedule() {
        let sink = RecordingSink::default();
        with_overrides(
            FakeReader::healthy(),
            FakeClock::synced_at_noon_in_june(),
            &sink,
            FakeOverrides::forcing(LightingTarget::Profile(LightingProfile::Off)),
        )
        .sample()
        .await;

        let s = sink.last();
        assert_eq!(
            s.lighting.profile,
            LightingProfile::Off,
            "at noon the schedule wants Daylight, but the override wins"
        );
        assert_eq!(s.lighting.channel_white_pct, 0);
        assert!(s.lighting.override_active);
    }

    #[tokio::test]
    async fn without_an_override_the_schedule_still_rules() {
        let sink = RecordingSink::default();
        collector(
            FakeReader::healthy(),
            FakeClock::synced_at_noon_in_june(),
            &sink,
        )
        .sample()
        .await;

        let s = sink.last();
        assert_eq!(s.lighting.profile, LightingProfile::Daylight);
        assert!(!s.lighting.override_active);
    }

    #[tokio::test]
    async fn a_channels_override_applies_raw_levels() {
        let sink = RecordingSink::default();
        with_overrides(
            FakeReader::healthy(),
            FakeClock::synced_at_noon_in_june(),
            &sink,
            FakeOverrides::forcing(LightingTarget::Channels(ChannelLevels::new(80, 20))),
        )
        .sample()
        .await;

        let s = sink.last();
        assert_eq!(s.lighting.channel_white_pct, 80);
        assert_eq!(s.lighting.channel_rgb_pct, 20);
        assert!(s.lighting.override_active);
    }

    #[tokio::test]
    async fn republish_does_not_touch_the_sensor_bus() {
        let sink = RecordingSink::default();
        let mut c = collector(
            FakeReader::healthy(),
            FakeClock::synced_at_noon_in_june(),
            &sink,
        );

        c.sample().await;
        // A child module can read private fields of its ancestor, so the test
        // reaches the counter directly instead of widening the public surface.
        let after_sample = c.reader.conversions;

        c.republish();
        c.republish();

        assert_eq!(sink.count(), 3, "every call publishes a snapshot");
        assert_eq!(
            c.reader.conversions, after_sample,
            "a burst of API requests MUST NOT hammer the 1-Wire bus"
        );
    }

    #[tokio::test]
    async fn republish_reuses_the_last_readings() {
        let sink = RecordingSink::default();
        let mut c = collector(
            FakeReader::healthy(),
            FakeClock::synced_at_noon_in_june(),
            &sink,
        );

        c.sample().await;
        c.republish();

        assert_eq!(sink.last().temperature.tank_celsius, Some(26.5));
    }

    #[tokio::test]
    async fn republish_before_any_sample_reports_no_readings() {
        let sink = RecordingSink::default();
        let c = collector(
            FakeReader::healthy(),
            FakeClock::synced_at_noon_in_june(),
            &sink,
        );

        c.republish();

        let s = sink.last();
        assert_eq!(
            s.temperature.tank_celsius, None,
            "an empty cache reports no reading instead of faking data"
        );
        assert!(!s.temperature.sensors_healthy);
    }
```

`FakeReader` musi umieć policzyć konwersje. Zamień jego definicję i impl:

```rust
    struct FakeReader {
        tank: Option<f32>,
        sump: Option<f32>,
        trigger_fails: bool,
        conversions: usize,
    }

    impl FakeReader {
        fn healthy() -> Self {
            Self {
                tank: Some(26.5),
                sump: Some(26.0),
                trigger_fails: false,
                conversions: 0,
            }
        }
    }

    impl TemperatureReader for FakeReader {
        async fn trigger_conversion(&mut self) -> Result<(), TemperatureError> {
            self.conversions += 1;

            if self.trigger_fails {
                Err(TemperatureError::BusError("fake: the bus went down"))
            } else {
                Ok(())
            }
        }

        async fn read_temperature(
            &mut self,
            role: TemperatureSensorRole,
        ) -> Result<WaterTemperature, TemperatureError> {
            let value = match role {
                TemperatureSensorRole::MainTank => self.tank,
                TemperatureSensorRole::Sump => self.sump,
            };

            let celsius = value.ok_or(TemperatureError::SensorNotFound(role))?;
            WaterTemperature::new(celsius).map_err(TemperatureError::InvalidReading)
        }
    }
```

Pozostałe trzy miejsca konstruujące `FakeReader` bezpośrednio (w testach
`one_dead_sensor_does_not_lose_the_other`, `bus_failure_still_publishes_a_snapshot`
i `out_of_safe_range_reading_is_reported_not_dropped`) potrzebują dopisanego pola
`conversions: 0`.

- [ ] **Krok 3: Uruchom testy i potwierdź, że NIE kompilują się**

Uruchom: `make test`

Oczekiwane: `this struct takes 5 generic arguments but 4 were supplied`,
`no method named 'republish'` oraz `struct 'FakeReader' has no field named 'conversions'`.

- [ ] **Krok 4: Przepisz `TelemetryCollector`**

W `core/src/application/collector.rs` zamień wszystko od pierwszej linii do końca
`fn evaluate_lighting` (czyli cały kod przed `#[cfg(test)]`) na:

```rust
use crate::domain::light::{self, LightingSchedule};
use crate::domain::metrics::{LightingMetricsDto, LightingProfile, TankTelemetryDto};
use crate::ports::outbound::{
    Clock, LightingOverrideSource, SystemHealthProvider, TelemetrySink, TemperatureReader,
    TemperatureSensorRole,
};

pub struct TelemetryCollector<R, C, H, S, O> {
    reader: R,
    clock: C,
    health: H,
    sink: S,
    overrides: O,
    lighting: LightingSchedule,
    target_celsius: f32,
    /// Last sensor readings, so `republish` can recompute telemetry without
    /// touching the 1-Wire bus.
    last_readings: (Option<f32>, Option<f32>),
}

impl<R, C, H, S, O> TelemetryCollector<R, C, H, S, O>
where
    R: TemperatureReader,
    C: Clock,
    H: SystemHealthProvider,
    S: TelemetrySink,
    O: LightingOverrideSource,
{
    pub fn new(reader: R, clock: C, health: H, sink: S, overrides: O, target_celsius: f32) -> Self {
        Self {
            reader,
            clock,
            health,
            sink,
            overrides,
            lighting: LightingSchedule::new(),
            target_celsius,
            last_readings: (None, None),
        }
    }

    /// Full cycle: read the sensors, store them in the cache, publish.
    pub async fn sample(&mut self) {
        self.last_readings = self.read_sensors().await;
        self.publish();
    }

    /// Recomputes and publishes telemetry from the last readings. Not `async`
    /// and never touches the bus, so it is safe to call on every override change.
    pub fn republish(&self) {
        self.publish();
    }

    fn publish(&self) {
        let (tank, sump) = self.last_readings;
        self.sink.publish(self.compose(tank, sump));
    }

    async fn read_sensors(&mut self) -> (Option<f32>, Option<f32>) {
        if self.reader.trigger_conversion().await.is_err() {
            return (None, None);
        }

        let tank = self
            .reader
            .read_temperature(TemperatureSensorRole::MainTank)
            .await
            .ok()
            .map(|t| t.as_celsius());

        let sump = self
            .reader
            .read_temperature(TemperatureSensorRole::Sump)
            .await
            .ok()
            .map(|t| t.as_celsius());

        (tank, sump)
    }

    fn compose(&self, tank: Option<f32>, sump: Option<f32>) -> TankTelemetryDto {
        let mut dto = TankTelemetryDto::empty();

        dto.timestamp_epoch_s = self.clock.epoch_seconds().unwrap_or(0);

        dto.system_health.uptime_seconds = self.clock.uptime_seconds();
        dto.system_health.wifi_rssi_dbm = self.health.wifi_rssi_dbm();
        dto.system_health.free_heap_kb = self.health.free_heap_kb();

        dto.temperature.tank_celsius = tank;
        dto.temperature.sump_celsius = sump;
        dto.temperature.target_celsius = self.target_celsius;
        dto.temperature.sensors_healthy = tank.is_some() && sump.is_some();

        dto.lighting = self.evaluate_lighting();

        dto
    }

    fn evaluate_lighting(&self) -> LightingMetricsDto {
        let scheduled = match (self.clock.local_minute_of_day(), self.clock.local_month()) {
            (Some(minute), Some(month)) => self.lighting.evaluate(minute, month),
            _ => LightingMetricsDto {
                profile: LightingProfile::Off,
                channel_white_pct: 0,
                channel_rgb_pct: 0,
                override_active: false,
            },
        };

        light::resolve(scheduled, self.overrides.active_override().as_ref())
    }
}
```

- [ ] **Krok 5: Uruchom testy i potwierdź, że przechodzą**

Uruchom: `make test`

Oczekiwane: **84 passed** (78 + 6 nowych), 0 failed.

- [ ] **Krok 6: Lint i formatowanie**

Uruchom: `make fmt-check && make lint-core`

Oczekiwane: czysto. **Nie uruchamiaj `make lint-firmware`** — firmware jest teraz
celowo niespójny i naprawia się w Tasku 7.

---

## Task 6: Warstwa HTTP

**Pliki:**
- Utwórz: `core/src/adapters/inbound/http/handlers/lighting.rs`
- Modyfikuj: `core/src/adapters/inbound/http/handlers/mod.rs`
- Modyfikuj: `core/src/adapters/inbound/http/routes.rs`
- Modyfikuj: `core/src/adapters/inbound/http/mod.rs`
- Modyfikuj: `core/tests/http_api.rs`

- [ ] **Krok 1: Dopisz testy integracyjne w `core/tests/http_api.rs`**

Zamień nagłówek pliku (importy plus `start`) na:

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::LocalSet;

use aquarium_core::adapters::inbound::http::{self, HttpConfig};
use aquarium_core::adapters::outbound::memory::{InMemoryLightingStore, InMemoryTelemetryStore};
use aquarium_core::domain::metrics::TankTelemetryDto;
use aquarium_core::ports::outbound::TelemetrySink;

const TOKEN: &str = "test-token";

fn config() -> HttpConfig {
    HttpConfig {
        token: TOKEN,
        max_connections: 4,
        sse_keepalive: Duration::from_secs(15),
    }
}

async fn start(store: Arc<InMemoryTelemetryStore>) -> SocketAddr {
    start_with_lighting(store, Arc::new(InMemoryLightingStore::new())).await
}

async fn start_with_lighting(
    store: Arc<InMemoryTelemetryStore>,
    lighting: Arc<InMemoryLightingStore>,
) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    tokio::task::spawn_local(http::run(listener, store, lighting, config()));

    addr
}

fn body(method: &str, path: &str, auth: Option<&str>, json: &str) -> String {
    let auth_line = match auth {
        Some(token) => format!("Authorization: Bearer {token}\r\n"),
        None => String::new(),
    };

    format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{auth_line}\
         Content-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{json}",
        json.len()
    )
}
```

Dopisz na końcu pliku:

```rust
const OVERRIDE_PATH: &str = "/api/lighting/override";

#[tokio::test]
async fn setting_an_override_without_a_token_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(
                addr,
                &body("PUT", OVERRIDE_PATH, None, r#"{"target":{"profile":"off"}}"#),
            )
            .await;
            assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn a_query_token_does_not_open_the_override_route() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(
                addr,
                &body(
                    "PUT",
                    &format!("{OVERRIDE_PATH}?token={TOKEN}"),
                    None,
                    r#"{"target":{"profile":"off"}}"#,
                ),
            )
            .await;
            assert!(
                response.starts_with("HTTP/1.1 401"),
                "the query-token fallback must apply ONLY to /api/events, got: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn no_override_set_yields_no_content() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get(OVERRIDE_PATH, Some(TOKEN))).await;
            assert!(response.starts_with("HTTP/1.1 204"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn setting_a_profile_override_is_reflected_in_telemetry() {
    LocalSet::new()
        .run_until(async {
            let telemetry = Arc::new(InMemoryTelemetryStore::new());
            let lighting = Arc::new(InMemoryLightingStore::new());
            let addr = start_with_lighting(Arc::clone(&telemetry), Arc::clone(&lighting)).await;

            let set = raw(
                addr,
                &body(
                    "PUT",
                    OVERRIDE_PATH,
                    Some(TOKEN),
                    r#"{"target":{"profile":"moonlight"}}"#,
                ),
            )
            .await;
            assert!(set.starts_with("HTTP/1.1 200"), "got: {set}");

            let read = raw(addr, &get(OVERRIDE_PATH, Some(TOKEN))).await;
            assert!(read.starts_with("HTTP/1.1 200"), "got: {read}");
            assert!(read.contains("moonlight"), "got: {read}");
        })
        .await;
}

#[tokio::test]
async fn setting_raw_channels_round_trips() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;

            let set = raw(
                addr,
                &body(
                    "PUT",
                    OVERRIDE_PATH,
                    Some(TOKEN),
                    r#"{"target":{"channels":{"white_pct":80,"rgb_pct":20}}}"#,
                ),
            )
            .await;

            assert!(set.starts_with("HTTP/1.1 200"), "got: {set}");
            assert!(set.contains("\"white_pct\":80"), "got: {set}");
        })
        .await;
}

#[tokio::test]
async fn a_channel_level_above_one_hundred_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(
                addr,
                &body(
                    "PUT",
                    OVERRIDE_PATH,
                    Some(TOKEN),
                    r#"{"target":{"channels":{"white_pct":150,"rgb_pct":20}}}"#,
                ),
            )
            .await;
            assert!(
                response.starts_with("HTTP/1.1 400"),
                "u8 admits 150, so validation MUST catch it: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn a_zero_duration_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(
                addr,
                &body(
                    "PUT",
                    OVERRIDE_PATH,
                    Some(TOKEN),
                    r#"{"target":{"profile":"off"},"duration_s":0}"#,
                ),
            )
            .await;
            assert!(
                response.starts_with("HTTP/1.1 400"),
                "set-and-immediately-expire is a contradictory request: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn malformed_json_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &body("PUT", OVERRIDE_PATH, Some(TOKEN), "{nonsense")).await;
            assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn deleting_an_override_returns_to_the_schedule() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;

            let _ = raw(
                addr,
                &body(
                    "PUT",
                    OVERRIDE_PATH,
                    Some(TOKEN),
                    r#"{"target":{"profile":"off"}}"#,
                ),
            )
            .await;

            let deleted = raw(addr, &body("DELETE", OVERRIDE_PATH, Some(TOKEN), "")).await;
            assert!(deleted.starts_with("HTTP/1.1 204"), "got: {deleted}");

            let read = raw(addr, &get(OVERRIDE_PATH, Some(TOKEN))).await;
            assert!(read.starts_with("HTTP/1.1 204"), "got: {read}");
        })
        .await;
}

#[tokio::test]
async fn deleting_a_nonexistent_override_is_idempotent() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &body("DELETE", OVERRIDE_PATH, Some(TOKEN), "")).await;
            assert!(response.starts_with("HTTP/1.1 204"), "got: {response}");
        })
        .await;
}
```

- [ ] **Krok 2: Uruchom testy i potwierdź, że NIE kompilują się**

Uruchom: `make test`

Oczekiwane: `cannot find type 'InMemoryLightingStore' in ...` albo
`this function takes 3 arguments but 4 arguments were supplied` dla `http::run`.

- [ ] **Krok 3: Utwórz `core/src/adapters/inbound/http/handlers/lighting.rs`**

```rust
use picoserve::extract::State;
use picoserve::response::{Json, StatusCode};
use serde::Deserialize;

use crate::adapters::inbound::http::routes::LightingState;
use crate::domain::light::{LightingOverride, LightingTarget};
use crate::ports::inbound::LightingControl;

#[derive(Debug, Deserialize)]
pub struct SetOverrideRequest {
    pub target: LightingTarget,
    pub duration_s: Option<u32>,
}

impl SetOverrideRequest {
    fn validate(&self) -> Result<(), &'static str> {
        self.target.validate()?;

        if self.duration_s == Some(0) {
            return Err("duration must be positive");
        }

        Ok(())
    }
}

pub async fn current<L: LightingControl>(
    State(LightingState(lighting)): State<LightingState<L>>,
) -> Result<Json<LightingOverride>, StatusCode> {
    tracing::info!("GET /api/lighting/override");

    match lighting.current_override() {
        Some(over) => Ok(Json(over)),
        None => Err(StatusCode::NO_CONTENT),
    }
}

pub async fn set<L: LightingControl>(
    State(LightingState(lighting)): State<LightingState<L>>,
    Json(request): Json<SetOverrideRequest>,
) -> Result<Json<LightingOverride>, (StatusCode, &'static str)> {
    if let Err(reason) = request.validate() {
        tracing::warn!(reason, "PUT /api/lighting/override rejected");
        return Err((StatusCode::BAD_REQUEST, reason));
    }

    lighting.set_override(request.target, request.duration_s);
    tracing::info!(?request.target, "PUT /api/lighting/override");

    Ok(Json(
        lighting
            .current_override()
            .expect("override was just set and duration_s > 0 is enforced"),
    ))
}

pub async fn clear<L: LightingControl>(
    State(LightingState(lighting)): State<LightingState<L>>,
) -> StatusCode {
    tracing::info!("DELETE /api/lighting/override");
    lighting.clear_override();
    StatusCode::NO_CONTENT
}
```

- [ ] **Krok 4: Zarejestruj handler**

Zamień całą zawartość `core/src/adapters/inbound/http/handlers/mod.rs`:

```rust
pub mod events;
pub mod health;
pub mod lighting;
pub mod telemetry;
```

- [ ] **Krok 5: Przepisz `core/src/adapters/inbound/http/routes.rs`**

```rust
use std::sync::Arc;
use std::time::Duration;

use picoserve::extract::{FromRef, State};
use picoserve::routing::{get, PathRouter};
use picoserve::Router;

use crate::adapters::inbound::http::auth::ApiTokenLayer;
use crate::adapters::inbound::http::handlers;
use crate::ports::inbound::{LightingControl, TelemetryQuery};

pub struct AppState<Q, L> {
    pub telemetry: Arc<Q>,
    pub lighting: Arc<L>,
}

/// Lighting leaves the state through its own type, NOT through a bare `Arc<L>`.
/// The pair `FromRef<AppState<Q, L>> for Arc<Q>` and `... for Arc<L>` overlaps when
/// `Q == L`, so Rust's coherence checker rejects it. Two distinct type constructors
/// can never be equal, so the conflict disappears.
pub struct LightingState<L>(pub Arc<L>);

impl<Q, L> FromRef<AppState<Q, L>> for Arc<Q> {
    fn from_ref(input: &AppState<Q, L>) -> Self {
        Arc::clone(&input.telemetry)
    }
}

impl<Q, L> FromRef<AppState<Q, L>> for LightingState<L> {
    fn from_ref(input: &AppState<Q, L>) -> Self {
        LightingState(Arc::clone(&input.lighting))
    }
}

pub fn router<Q, L>(
    telemetry: Arc<Q>,
    lighting: Arc<L>,
    token: &'static str,
    sse_keepalive: Duration,
) -> Router<impl PathRouter<AppState<Q, L>>>
where
    Q: TelemetryQuery + 'static,
    L: LightingControl + 'static,
{
    Router::new()
        .route("/api/telemetry", get(handlers::telemetry::snapshot::<Q>))
        .route(
            "/api/events",
            get(move |state: State<Arc<Q>>| async move {
                handlers::events::stream(state, sse_keepalive).await
            }),
        )
        .route("/api/health", get(handlers::health::probe::<Q>))
        .route(
            "/api/lighting/override",
            get(handlers::lighting::current::<L>)
                .put(handlers::lighting::set::<L>)
                .delete(handlers::lighting::clear::<L>),
        )
        .layer(ApiTokenLayer::new(token))
        .with_state(AppState {
            telemetry,
            lighting,
        })
}
```

- [ ] **Krok 6: Zaktualizuj `core/src/adapters/inbound/http/mod.rs`**

Zamień import i sygnaturę `run`. Z:

```rust
use crate::ports::inbound::TelemetryQuery;
```

na:

```rust
use crate::ports::inbound::{LightingControl, TelemetryQuery};
```

Oraz z:

```rust
pub async fn run<Q>(listener: TcpListener, telemetry: Arc<Q>, config: HttpConfig) -> !
where
    Q: TelemetryQuery + 'static,
{
    let router: &'static _ = Box::leak(Box::new(routes::router(
        telemetry,
        config.token,
        config.sse_keepalive,
    )));
```

na:

```rust
pub async fn run<Q, L>(
    listener: TcpListener,
    telemetry: Arc<Q>,
    lighting: Arc<L>,
    config: HttpConfig,
) -> !
where
    Q: TelemetryQuery + 'static,
    L: LightingControl + 'static,
{
    let router: &'static _ = Box::leak(Box::new(routes::router(
        telemetry,
        lighting,
        config.token,
        config.sse_keepalive,
    )));
```

- [ ] **Krok 7: Uruchom testy i potwierdź, że przechodzą**

Uruchom: `make test`

Oczekiwane: 84 passed w bibliotece, **18 passed** w `http_api` (8 z baseline + 10 nowych),
0 failed.

Jeśli `malformed_json_is_rejected` zwraca inny kod niż 400, sprawdź czy handler
`set` przyjmuje `Json<SetOverrideRequest>` jako OSTATNI parametr — picoserve pozwala
na jeden ekstraktor ciała i musi on stać na końcu listy argumentów.

- [ ] **Krok 8: Lint i formatowanie**

Uruchom: `make fmt-check && make lint-core`

Oczekiwane: czysto.

---

## Task 7: Złożenie zależności w firmware

**Pliki:**
- Modyfikuj: `src/main.rs`

Ten task przywraca kompilację crate'u `aquarium-controller`. Nie da się go pokryć
testami na hoście — weryfikacją jest clippy na targecie ESP.

- [ ] **Krok 1: Dodaj importy w `src/main.rs`**

Zamień:

```rust
use aquarium_core::adapters::outbound::memory::InMemoryTelemetryStore;
use aquarium_core::application::TelemetryCollector;
use aquarium_core::ports::outbound::{
    Clock, SystemHealthProvider, TelemetrySink, TemperatureReader,
};
```

na:

```rust
use aquarium_core::adapters::outbound::memory::{InMemoryLightingStore, InMemoryTelemetryStore};
use aquarium_core::application::TelemetryCollector;
use aquarium_core::ports::outbound::{
    Clock, LightingOverrideSource, SystemHealthProvider, TelemetrySink, TemperatureReader,
};
```

oraz dopisz do importów tokio:

```rust
use tokio::sync::Notify;
```

- [ ] **Krok 2: Utwórz store świateł i przekaż go dalej**

W funkcji `run()` zamień:

```rust
    let clock = SystemClock::new().context("Failed to start SNTP")?;
    let store = Arc::new(InMemoryTelemetryStore::new());
```

na:

```rust
    let clock = SystemClock::new().context("Failed to start SNTP")?;
    let store = Arc::new(InMemoryTelemetryStore::new());
    let lighting = Arc::new(InMemoryLightingStore::new());
    let lighting_changed = lighting.change_signal();
```

Następnie zamień:

```rust
    let collector = TelemetryCollector::new(
        sensors,
        clock,
        EspSystemHealth,
        Arc::clone(&store),
        SETTINGS.sensors.target_celsius,
    );

    let server = http::run(
        listener,
        store,
        HttpConfig {
            token: SETTINGS.http.token.expose(),
            max_connections: SETTINGS.http.max_connections,
            sse_keepalive: SETTINGS.http.sse_keepalive,
        },
    );

    tokio::join!(sensor_task(collector), wifi.run(), server);
```

na:

```rust
    let collector = TelemetryCollector::new(
        sensors,
        clock,
        EspSystemHealth,
        Arc::clone(&store),
        Arc::clone(&lighting),
        SETTINGS.sensors.target_celsius,
    );

    let server = http::run(
        listener,
        store,
        lighting,
        HttpConfig {
            token: SETTINGS.http.token.expose(),
            max_connections: SETTINGS.http.max_connections,
            sse_keepalive: SETTINGS.http.sse_keepalive,
        },
    );

    tokio::join!(
        sensor_task(collector, lighting_changed),
        wifi.run(),
        server
    );
```

- [ ] **Krok 3: Przepisz pętlę `sensor_task`**

Zamień całą funkcję `sensor_task`:

```rust
async fn sensor_task<R, C, H, S, O>(
    mut collector: TelemetryCollector<R, C, H, S, O>,
    lighting_changed: Arc<Notify>,
) where
    R: TemperatureReader,
    C: Clock,
    H: SystemHealthProvider,
    S: TelemetrySink,
    O: LightingOverrideSource,
{
    let mut ticker = tokio::time::interval(SETTINGS.sensors.read_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => collector.sample().await,
            // republish never touches the bus, so a burst of API requests
            // cannot turn into hammering 1-Wire
            _ = lighting_changed.notified() => collector.republish(),
        }
    }
}
```

- [ ] **Krok 4: Sprawdź, że firmware znów się kompiluje**

Uruchom: `make lint-firmware`

Oczekiwane: `Clippy: firmware`, zakończone bez błędów i bez ostrzeżeń
(cel ma `-D warnings`).

Jeśli pada na braku zmiennych środowiskowych, upewnij się, że plik `.env` istnieje —
`check-env` go wymaga. Nie twórz go sam ani nie zgaduj wartości; poproś właściciela.

- [ ] **Krok 5: Pełna weryfikacja**

Uruchom: `make ci`

Oczekiwane: `fmt-check`, `lint` (core i firmware) oraz `test` przechodzą.
Końcowy komunikat: `CI passed`.

---

## Task 8: Rozszerzenie smoke testu na urządzeniu

**Pliki:**
- Modyfikuj: `Makefile`

- [ ] **Krok 1: Dopisz sondy override'u do celu `smoke`**

W `Makefile`, w celu `smoke`, wstaw przed linią
`@$(OK) "Smoke test passed"`:

```makefile
	@$(SUB) "override: no token -> expecting 401"
	@code=$$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 -X PUT \
	  -H 'Content-Type: application/json' -d '{"target":{"profile":"off"}}' \
	  http://$(IP)/api/lighting/override); \
	[ "$$code" = "401" ] || { $(ERR) "got $$code instead of 401"; exit 1; }
	@$(SUB) "override: level above 100 -> expecting 400"
	@$(LOAD_ENV) code=$$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 -X PUT \
	  -H "Authorization: Bearer $$API_TOKEN" -H 'Content-Type: application/json' \
	  -d '{"target":{"channels":{"white_pct":150,"rgb_pct":20}}}' \
	  http://$(IP)/api/lighting/override); \
	[ "$$code" = "400" ] || { $(ERR) "channel validation is broken: $$code"; exit 1; }
	@$(SUB) "override: set moonlight -> expecting 200"
	@$(LOAD_ENV) curl -s --max-time 5 -X PUT \
	  -H "Authorization: Bearer $$API_TOKEN" -H 'Content-Type: application/json' \
	  -d '{"target":{"profile":"moonlight"},"duration_s":60}' \
	  http://$(IP)/api/lighting/override | python3 -m json.tool
	@$(SUB) "override: telemetry reports override_active -> expecting true"
	@$(LOAD_ENV) curl -s --max-time 5 -H "Authorization: Bearer $$API_TOKEN" \
	  http://$(IP)/api/telemetry \
	  | python3 -c 'import json,sys; d=json.load(sys.stdin)["lighting"]; sys.exit(0 if d["override_active"] else 1)' \
	  || { $(ERR) "the override never reached telemetry"; exit 1; }
	@$(SUB) "override: delete -> expecting 204"
	@$(LOAD_ENV) code=$$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 -X DELETE \
	  -H "Authorization: Bearer $$API_TOKEN" http://$(IP)/api/lighting/override); \
	[ "$$code" = "204" ] || { $(ERR) "got $$code instead of 204"; exit 1; }
```

- [ ] **Krok 2: Sprawdź składnię Makefile bez urządzenia**

Uruchom: `make -n smoke IP=192.168.1.50 > /dev/null && echo OK`

Oczekiwane: `OK`. Ten tryb tylko rozwija komendy, nic nie wykonuje, więc nie
potrzebuje działającego ESP32. Jeśli zgłosi błąd składni, najczęstszą przyczyną
są spacje zamiast tabulatorów na początku linii receptury.

- [ ] **Krok 3: Uruchom smoke na urządzeniu — TYLKO jeśli jest wgrane i dostępne**

Uruchom: `make smoke IP=<adres urządzenia>`

Oczekiwane: `Smoke test passed`.

**Nie wgrywaj firmware sam.** Flashowanie to operacja na cudzym sprzęcie — jeśli
urządzenie nie ma jeszcze nowej wersji, zgłoś to właścicielowi i zakończ task na
Kroku 2.

---

## Weryfikacja końcowa

- [ ] `make ci` przechodzi w całości
- [ ] `make test` pokazuje **84 testy jednostkowe** i **18 integracyjnych**, 0 failed
- [ ] `git status` pokazuje wyłącznie zmodyfikowane i nowe pliki — **żadnego commita**
