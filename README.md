# aquarium-controller

**aquarium-controller** is ESP32-S3 firmware for a reef/planted tank controller, written in Rust on top of ESP-IDF. It samples the tank (DS18B20 temperature probes, XKC-Y25 non-contact water-level sensors, an analog pH electrode), publishes a single telemetry snapshot over HTTP and SSE, and optionally streams the same snapshots to InfluxDB. The code is split into a hardware-free `aquarium-core` crate — domain, protocols, ports and the HTTP layer, all testable on the host — and a thin firmware crate that wires real peripherals into those ports.

## Hardware

### DS18B20 temperature probes (shared 1-Wire bus)

Both probes sit on one 1-Wire bus. A single 4.7 kΩ pull-up between `3V3` and the data line serves the whole bus — do not add one per probe.

```text
         [4.7 kΩ pull-up]
         ┌─────[===]─────┐
         │               │
ESP32-S3 │               │          Junction (solder joint or terminal block)        Probe 1 (sump)
3V3 ─────┴───────────────┼──────────●──────────────────────────────────────────────  Red (VCC)
                         │          └──────────────────────────────────────────────  Red (VCC)              -> Probe 2 (main tank)
                         │
GPIO 4 ──────────────────┴──────────●──────────────────────────────────────────────  Yellow / White (DATA)
(1-Wire data)                       └──────────────────────────────────────────────  Yellow / White (DATA)  -> Probe 2

GND ────────────────────────────────●──────────────────────────────────────────────  Black / Blue (GND)
                                    └──────────────────────────────────────────────  Black / Blue (GND)     -> Probe 2
```

Notes:
* Probes are powered normally (VCC wired), not parasitically. The bus driver still detects parasite-powered devices and reports them.
* Each DS18B20 has a unique 64-bit ROM address, so both probes share the bus and are addressed individually — see [Identifying the probes](#identifying-the-probes).
* Keep the bus below a few metres of cable; for longer runs use shielded/twisted cable and keep the pull-up at the ESP32 end.

### Pin map

| Peripheral | Pin(s) | Mode |
|---|---|---|
| DS18B20 1-Wire bus (tank + sump) | `GPIO4` | open-drain input/output, external 4.7 kΩ pull-up |
| pH electrode (analog front-end) | `GPIO1` (ADC1) | ADC one-shot, 12 dB attenuation, curve calibration |
| XKC-Y25 — sump mechanical chamber | `GPIO11` | input, internal pull-up, active-low |
| XKC-Y25 — sump biological chamber | `GPIO12` | input, internal pull-up, active-low |
| XKC-Y25 — sump pump chamber | `GPIO13` | input, internal pull-up, active-low |
| XKC-Y25 — main tank | `GPIO14` | input, internal pull-up, active-low |
| XKC-Y25 — ATO reservoir | `GPIO15` | input, internal pull-up, active-low |

Board configuration lives in `sdkconfig.defaults`: 240 MHz, 8 MiB flash, octal PSRAM at 80 MHz, and a 128 KiB main-task stack (one `picoserve` connection costs roughly 78 KiB of stack on that task).

### Identifying the probes

The firmware addresses each DS18B20 by its ROM, so both ROMs must be known. Flip `run_mode` to `RunMode::RomDiscovery` in `crates/firmware/src/settings.rs`, flash, and watch the serial monitor: it runs a SEARCH ROM, prints every address it finds, and then loops printing a live reading per ROM. Hold one probe in your hand — the ROM whose temperature climbs is that probe. Paste the two addresses into `sensors.tank_rom` / `sensors.sump_rom` and switch `run_mode` back to `RunMode::Normal`.

## Key Features

* **Hexagonal, host-testable core:** `aquarium-core` holds the domain, the wire protocols (1-Wire/DS18B20, XKC-Y25 debouncing, pH conversion, Influx line protocol) and the HTTP adapter behind ports, with no ESP dependency — `make test` runs the whole suite on the host target.
* **Shared-bus temperature sensing:** ROM search, per-ROM addressing, CRC validation, and rejection of implausible readings (NaN, the 85 °C power-on-reset value, out-of-range values).
* **Water level with fault detection:** five XKC-Y25 inputs sampled at 10 Hz, debounced over a sample window, with flap detection that marks a chattering sensor faulty instead of acting on it.
* **pH measurement:** oversampled ADC reads (64 samples by default) converted through a two-point calibration with a nominal Nernst slope.
* **HTTP API + SSE:** a bearer-token-guarded `picoserve` router serving telemetry snapshots, a live `text/event-stream` feed, a health probe, and lighting override control.
* **Lighting overrides:** set a profile (`off`, `sunrise`, `daylight`, `sunset`, `moonlight`) or explicit channel percentages, optionally time-boxed; overrides republish telemetry immediately rather than waiting for the next sample tick.
* **InfluxDB 2.x export:** optional, compile-time gated on the `INFLUX_*` variables being present. Snapshots are buffered and batched, with bounded retry backoff, on a dedicated worker.
* **Component modes:** every subsystem (WiFi, HTTP, telemetry, temperature, water level, pH, Influx, health) is independently `Off` / `Initialized` / `Running`, so peripherals can be brought up without being wired into the runtime — useful when bisecting a hardware fault.
* **Secrets that do not leak:** credentials and the API token are wrapped in a `Secret` newtype whose `Debug`/`Display` redact the value.

## Installation

### Requirements
* The Espressif Rust toolchain — `espup` + `. ~/export-esp.sh` (the `esp` channel is pinned in `rust-toolchain.toml`)
* `ldproxy` and `espflash` (`cargo install ldproxy espflash`)
* `python3` and `make`
* An ESP32-S3 with 8 MiB flash and PSRAM, on `/dev/ttyACM0` (override with `PORT=`)
* Node 22+ with `corepack enable`, for the mobile client (yarn 4 is pinned in `package.json`)

A ready-made environment is also provided in `.devcontainer/` if you would rather not install the toolchain locally.

### Build and Installation

```bash
git clone https://github.com/robertmon-dev/aquarium-controller.git
cd aquarium-controller

make env-example   # writes a .env skeleton with a freshly generated API token
$EDITOR .env       # fill in WIFI_SSID and WIFI_PASS
make flash         # build release, flash the board, attach the serial monitor
```

`make` with no target prints every available recipe.

### Other useful targets
* `make build` / `make build-debug` — build the firmware without flashing
* `make monitor` — attach the serial monitor without reflashing
* `make size` — measure the real image against the `factory` partition and warn when headroom drops below 15%
* `make partitions` — dump the partition table ESP-IDF actually generated, to confirm it matches `partitions.csv`
* `make smoke IP=192.168.1.50` — end-to-end probe of a flashed device (auth, telemetry, SSE, lighting override lifecycle)
* `make telemetry IP=...` / `make events IP=...` — pretty-print one snapshot, or follow the SSE stream
* `make erase` — wipe the whole flash, NVS included
* `make clean` / `make clean-idf` — drop cargo artifacts, or also the ESP-IDF build tree
* `make types` — regenerate the TypeScript contract from the Rust DTOs
* `make types-check` — regenerate it and verify the mobile client still type-checks against it
* `make app-install` / `make app-start` — install JavaScript dependencies, start the Expo dev server
* `make js-lint` / `make js-test` — lint and test the JavaScript workspaces

## Configuration

### Environment Variables

Secrets are compiled into the firmware (`env!` / `option_env!`), so they are read from `.env` at **build** time — changing them requires a rebuild and reflash. `.env` is gitignored; `make check-env` runs before every build and fails early if anything required is missing.

```bash
WIFI_SSID="<network name>"
WIFI_PASS="<network password>"
API_TOKEN="<bearer token for the HTTP API>"   # make token prints a fresh one

# Optional: telemetry export to InfluxDB 2.x.
# All four must be set, or export stays off and the firmware builds as before.
INFLUX_URL="http://192.168.1.10:8086"
INFLUX_ORG="<org>"
INFLUX_BUCKET="<bucket>"
INFLUX_TOKEN="<influx api token>"
```

### Firmware settings

Everything that is not a secret is a `const` in `crates/firmware/src/settings.rs`, evaluated at compile time: per-component modes, the HTTP port (`80`), connection limit and SSE keepalive, sampling intervals, the two DS18B20 ROM addresses, the target temperature, pH calibration points, and the water-level polarity and debounce/flap windows.

## HTTP API

All routes sit behind a bearer-token layer: `Authorization: Bearer <API_TOKEN>`. `/api/events` additionally accepts `?token=` — a deliberate exception for `EventSource`, which cannot set headers — and that fallback is rejected on every other route.

| Method | Route | Description |
|---|---|---|
| `GET` | `/api/telemetry` | Latest telemetry snapshot as JSON |
| `GET` | `/api/events` | SSE stream (`event: telemetry`) pushed on every change, with periodic keepalives |
| `GET` | `/api/health` | Liveness/health probe |
| `GET` | `/api/lighting/override` | Current override, or `204` when none is active |
| `PUT` | `/api/lighting/override` | Set an override; `400` on an invalid target or a zero duration |
| `DELETE` | `/api/lighting/override` | Clear the override (`204`) |

```bash
curl -H "Authorization: Bearer $API_TOKEN" http://192.168.1.50/api/telemetry

curl -X PUT http://192.168.1.50/api/lighting/override \
  -H "Authorization: Bearer $API_TOKEN" -H 'Content-Type: application/json' \
  -d '{"target":{"profile":"moonlight"},"duration_s":600}'

curl -X PUT http://192.168.1.50/api/lighting/override \
  -H "Authorization: Bearer $API_TOKEN" -H 'Content-Type: application/json' \
  -d '{"target":{"channels":{"white_pct":40,"rgb_pct":20}}}'
```

A snapshot covers `system_mode`, `system_health` (uptime, RSSI, free heap), `temperature` (tank/sump/target, heater state, sensor health), `filtration`, `co2` (including the current pH), `ato`, `lighting`, `water_level` (per-sensor wet/faulty flags) and `leak_detected`.

The same contract is available to TypeScript clients as `packages/api-client`,
generated from these Rust types — see [The HTTP contract](#the-http-contract).

## Project Structure

```text
├── crates/
│   ├── core/                    # aquarium-core — hardware-free, host-testable
│   │   ├── src/
│   │   │   ├── domain/          # light, water, metrics (DTOs + invariants)
│   │   │   ├── protocol/        # ds18b20, xkc_y25, ph, influx line protocol
│   │   │   ├── ports/           # inbound (telemetry, lighting) / outbound traits
│   │   │   ├── application/     # collector, water_level sampler, metrics_export
│   │   │   ├── adapters/
│   │   │   │   ├── inbound/http/  # picoserve router, auth layer, SSE, handlers
│   │   │   │   └── outbound/    # in-memory stores
│   │   │   └── secret.rs        # redacting Secret<T> newtype
│   │   └── tests/               # http_auth, http_telemetry, http_lighting, http_events
│   └── firmware/                # aquarium-controller — ESP32-S3 firmware
│       ├── .cargo/config.toml   # xtensa target, linker, ESP-IDF env
│       ├── src/
│       │   ├── adapters/outbound/  # ds18b20, xkc_y25, ph, temperature, influx
│       │   ├── infrastructure/  # wifi, clock (SNTP), system_health
│       │   ├── settings.rs      # compile-time configuration
│       │   └── main.rs          # peripheral wiring, task graph, ROM discovery mode
│       ├── partitions.csv       # 8 MiB layout (3 MiB factory app + SPIFFS)
│       └── sdkconfig.defaults   # CPU/PSRAM/stack/socket tuning
├── packages/
│   └── api-client/              # typed HTTP + SSE client, generated from the Rust DTOs
│       └── src/generated/       # ts-rs output — not committed, run `make types`
├── apps/
│   └── mobile/                  # Expo client (expo-router, TanStack Query, react-hook-form)
│       └── src/
│           ├── app/             # routes only, one screen each
│           ├── components/      # atoms / molecules / organisms / pages / utility
│           └── device/          # config schema, secure storage, fetch seam, SSE hook
├── docs/                        # design specs and implementation plans
├── Makefile                     # build, flash, monitor, test, lint, size, smoke, types
└── package.json                 # yarn 4 workspace root
```

## Development and Testing

The core crate is built for the host, so tests run without hardware. The xtensa
target lives in `crates/firmware/.cargo/config.toml` rather than at the root, so
host commands need no `--target` override:

* **Run tests:** `make test` (or `make test-watch` with `cargo-watch`)
* **Run clippy:** `make lint` — core on the host target, firmware on `xtensa-esp32s3-espidf`, both `-D warnings`
* **Check formatting:** `make fmt-check` (`make fmt` to apply)
* **JavaScript:** `make js-lint`, `make js-test`
* **Everything CI gates on:** `make ci`

Firmware commands run from `crates/firmware`, which the Makefile handles — run
`make`, not bare `cargo`, or the ESP-IDF environment will not be picked up.

### The HTTP contract

`packages/api-client/src/generated/` is produced from the serializable types in
`crates/core/src/domain/` by `ts-rs`, and is **not** committed — the Rust types
are the only source of truth. After changing a DTO:

```bash
make types         # regenerate
make types-check   # regenerate, then type-check the client against it
```

A fresh checkout has to run `make types` once before the mobile client will
type-check or bundle.

### Mobile client

```bash
make app-install
make types
make app-start     # Expo dev server
```

Enter the controller's host and port in the settings screen, plus the `API_TOKEN`
from `.env`. The screen probes `GET /api/health` before saving, so a wrong
address or token fails at entry. The token is kept in `expo-secure-store`. The
telemetry screen subscribes to `/api/events` over `expo/fetch`, which is the one
`fetch` on React Native that can stream a response body.

Device discovery is out of scope: the firmware does not advertise mDNS, so the
address is entered by hand.

### Continuous integration

Two workflows, split by toolchain and by path:

* `rust_ci.yml` — formatting, clippy (core on host, firmware on xtensa), host tests, contract generation, and a release firmware build, via `esp-rs/xtensa-toolchain`.
* `js_ci.yml` — installs stable Rust only to run `make types`, then lints, type-checks and tests the yarn workspaces.

## Support

The firmware logs through `tracing` to the serial console (`make monitor`). With the `health` component set to `Running`, it reports stack high-water mark, free internal heap and the minimum free heap every 30 seconds, and raises an error-level log once stack headroom drops below 16 KiB. `CONFIG_FREERTOS_WATCHPOINT_END_OF_STACK` is enabled on purpose — it turns a stack overrun into an immediate, legible abort instead of corruption elsewhere.

For a flashed board on the network, `make smoke IP=...` exercises authentication, telemetry, the SSE stream and the full lighting-override lifecycle in one run.
