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

### ADS1115 analog front-end (shared I2C bus)

Both analog probes read through one ADS1115, a 16-bit ADC on I2C. The ESP32-S3 internal ADC is not used for them: it is noisier and non-linear, and a single external converter keeps both channels on the same reference.

```text
ESP32-S3                               ADS1115 module (address 0x48)

3V3 ─────────────────────────────────  VDD
GND ─────────────────────────────────  GND
GPIO8 (I2C SDA) ─────────────────────  SDA
GPIO9 (I2C SCL) ─────────────────────  SCL
                                       ADDR   (module pull-down -> 0x48)
                                       ALRT   (unused, comparator disabled)

                                       A0 ──  TDS board analog out
                                       A1 ──  pH board analog out
                                       A2 ──  unused
                                       A3 ──  unused
```

Notes:
* **Both probe boards must run from the same `3V3` rail as the ADS1115.** Absolute maximum on an analog input is VDD + 0.3 V; a board fed from 5 V can exceed that, and some pH boards drive their output towards the supply rail when the electrode is unplugged. Nothing in firmware can protect the converter from that.
* `ADDR` needs no wiring: the breakout module pulls it down on board, which selects address `0x48`. Only a bare ADS1115 chip has to have `ADDR` driven explicitly — the die itself has no pull-down, and TI's datasheet requires the pin to sit at `VDD`, `GND`, `SDA` or `SCL` rather than float.
* Most breakout modules already carry 10 kΩ pull-ups on `SDA`/`SCL`. If yours does not, add 4.7 kΩ from each line to `3V3`.
* Powering the ADS1115 from 5 V while driving I2C at 3.3 V is not reliable: its logic-high threshold is 0.7 × VDD = 3.5 V, which 3.3 V does not meet.
* The driver uses PGA ±4.096 V (125 µV per count), single-shot conversions at 128 SPS, and averages 16 samples per reading — about 150 ms per channel. `A2` and `A3` are defined in the driver but unused.
* `GPIO8` and `GPIO9` are free on the ESP32-S3: the strapping pins are `GPIO0`, `GPIO3`, `GPIO45`, `GPIO46`, and USB uses `GPIO19`/`GPIO20`.

#### pH probe — channel A1

The electrode sits in the sump. Its board is a high-impedance buffer: the BNC side must stay dry and clean, and the output is a plain analog voltage.

```text
ADS1115 / ESP32-S3                     pH interface board            Electrode

3V3 ─────────────────────────────────  VCC
GND ─────────────────────────────────  GND
ADS1115 A1 ──────────────────────────  Po (analog out)
                                       BNC ───────────────────────── pH electrode
```

Notes:
* `Po` idles near half the supply — roughly 1.65 V at `3V3` — for a pH 7 buffer.
* Three pH units move the output by about 0.18 V (59 mV per unit at 25 °C). The firmware rejects a calibration whose two buffer points sit closer than 30 mV, which is how a dead electrode or the same buffer measured twice is caught.
* Board polarity does not matter: the conversion accepts an acid point either above or below the neutral point.
* An electrode left in air gives a stable, meaningless voltage. Read it as "not measuring", not as a pH.

#### TDS probe — channel A0

This probe sits in the top-up tank, which has no temperature sensor — the compensation temperature is entered by hand (see [Probe calibration](#probe-calibration)).

```text
ADS1115 / ESP32-S3                     TDS interface board           Probe

3V3 ─────────────────────────────────  VCC
GND ─────────────────────────────────  GND
ADS1115 A0 ──────────────────────────  A (analog out, 0-2.3 V)
                                       Probe ─────────────────────── two-electrode TDS probe
```

Notes:
* Output spans 0–2.3 V. Through the conversion polynomial the rated 1000 ppm lands near 2.16 V, and the 2.3 V ceiling reads about 1121 ppm — readings above the rating are reported and flagged, not discarded.
* The polynomial is DFRobot's, derived for a board fed from 5 V. On `3V3` expect a scale shift; `tds_factor` corrects it against a reference solution and is set over the API, not recompiled.
* The probe has no polarity and is excited with AC, so it does not electroplate. It does not read anything useful in air: with the electrodes bridged by nothing, the board reports an open-circuit voltage, not a low concentration.

### Pin map

| Peripheral | Pin(s) | Mode |
|---|---|---|
| DS18B20 1-Wire bus (tank + sump) | `GPIO4` | open-drain input/output, external 4.7 kΩ pull-up |
| ADS1115 I2C bus — data | `GPIO8` | I2C master at 400 kHz, pull-up on the module |
| ADS1115 I2C bus — clock | `GPIO9` | I2C master at 400 kHz, pull-up on the module |
| pH electrode (analog front-end) | ADS1115 `A1` | single-ended, PGA ±4.096 V, single-shot 128 SPS |
| TDS probe (analog front-end) | ADS1115 `A0` | single-ended, PGA ±4.096 V, single-shot 128 SPS |
| XKC-Y25 — sump mechanical chamber | `GPIO11` | input, internal pull-up, active-low |
| XKC-Y25 — sump biological chamber | `GPIO12` | input, internal pull-up, active-low |
| XKC-Y25 — sump pump chamber | `GPIO13` | input, internal pull-up, active-low |
| XKC-Y25 — main tank | `GPIO14` | input, internal pull-up, active-low |
| XKC-Y25 — ATO reservoir | `GPIO15` | input, internal pull-up, active-low |

Board configuration lives in `sdkconfig.defaults`: 240 MHz, 8 MiB flash, octal PSRAM at 80 MHz, and a 128 KiB main-task stack (one `picoserve` connection costs roughly 78 KiB of stack on that task).

### Identifying the probes

The firmware addresses each DS18B20 by its ROM, so both ROMs must be known. Flip `run_mode` to `RunMode::RomDiscovery` in `crates/firmware/src/settings.rs`, flash, and watch the serial monitor: it runs a SEARCH ROM, prints every address it finds, and then loops printing a live reading per ROM. Hold one probe in your hand — the ROM whose temperature climbs is that probe. Paste the two addresses into `sensors.tank_rom` / `sensors.sump_rom` and switch `run_mode` back to `RunMode::Normal`.

### Probe calibration

Both analog probes need calibration before their readings mean anything. The stored defaults in `settings.rs` are only a starting point — treat a pH value as false until both buffer points have been captured.

**First contact with the hardware.** Set `run_mode: RunMode::AdcProbe` in `crates/firmware/src/settings.rs`, flash, and watch the serial monitor. It prints the raw count and the voltage for `A0` and `A1` every two seconds and starts nothing else — no WiFi, no HTTP — so a wiring fault cannot hide behind a network problem. A count stuck at `0` or `32767` means the input sits on a rail; `ADS1115 did not accept the config write` means nothing answers at `0x48`. Check `SDA`/`SCL` first — swapping them is the usual cause — then power, then whether anything pulls `ADDR` high: a jumper or solder bridge to `VDD` moves the module to `0x49` and the default address stops matching.

**Everything after that happens in the apps.** Switch `run_mode` back to `RunMode::Normal`, set `ph` and `tds` to `ComponentMode::Running`, flash, and open Settings in the mobile or web client. The calibration panel there shows the live probe voltage next to the derived reading and writes to NVS, so a calibration survives a reboot:

1. Put the pH electrode in a 7.00 buffer and wait until the panel stops reporting "settling" — it wants five consecutive samples within 2 mV, so about ten seconds. Press capture.
2. Rinse, repeat in a 4.00 buffer, press the other capture button.
3. Type the buffer temperature. There is no sensor in the beaker, so this value cannot be measured for you.
4. Enter the top-up tank temperature. Same reason: no sensor there either.
5. Put the TDS probe in a reference solution, enter its ppm, and the panel computes `tds_factor` from the current reading.
6. Save. The device validates the whole set and answers with what it actually stored.

The panel refuses to capture a point while the reading is still moving. That gate is not cosmetic: a point captured from a drifting electrode quietly corrupts every later pH reading until the next calibration.

## Key Features

* **Hexagonal, host-testable core:** `aquarium-core` holds the domain, the wire protocols (1-Wire/DS18B20, XKC-Y25 debouncing, ADS1115 registers, pH and TDS conversion, Influx line protocol) and the HTTP adapter behind ports, with no ESP dependency — `make test` runs the whole suite on the host target.
* **Shared-bus temperature sensing:** ROM search, per-ROM addressing, CRC validation, and rejection of implausible readings (NaN, the 85 °C power-on-reset value, out-of-range values).
* **Water level with fault detection:** five XKC-Y25 inputs sampled at 10 Hz, debounced over a sample window, with flap detection that marks a chattering sensor faulty instead of acting on it.
* **pH and TDS on one external ADC:** an ADS1115 over I2C serves both probes; 16 averaged samples per reading at 16-bit resolution. pH goes through a two-point calibration with a nominal Nernst slope and sump-temperature compensation; TDS through DFRobot's polynomial with a hand-entered compensation temperature and a correction factor.
* **Calibration over the API, persisted in NVS:** buffer points, the top-up tank temperature and the TDS factor are set from the apps and survive a reboot. The write goes to flash before the value takes effect, so a calibration that cannot be stored is refused rather than silently lost at the next power cycle.
* **HTTP API + SSE:** a bearer-token-guarded `picoserve` router serving telemetry snapshots, a live `text/event-stream` feed, a health probe, and lighting override control.
* **Lighting overrides:** set a profile (`off`, `sunrise`, `daylight`, `sunset`, `moonlight`) or explicit channel percentages, optionally time-boxed; overrides republish telemetry immediately rather than waiting for the next sample tick.
* **InfluxDB 2.x export:** optional, compile-time gated on the `INFLUX_*` variables being present. Snapshots are buffered and batched, with bounded retry backoff, on a dedicated worker.
* **Component modes:** every subsystem (WiFi, HTTP, telemetry, temperature, water level, pH, TDS, Influx, health) is independently `Off` / `Initialized` / `Running`, so peripherals can be brought up without being wired into the runtime — useful when bisecting a hardware fault.
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

Everything that is not a secret is a `const` in `crates/firmware/src/settings.rs`, evaluated at compile time: per-component modes, the HTTP port (`80`), connection limit and SSE keepalive, sampling intervals, the two DS18B20 ROM addresses, the target temperature, the ADS1115 front-end (address, gain, data rate, sample count, channel assignment), and the water-level polarity and debounce/flap windows.

Calibration is the exception: `sensors.calibration` holds only the **defaults used when NVS is empty**. Once a calibration has been written over the API it comes from flash, and editing the constants no longer changes what the device uses. `make erase` wipes NVS — and with it both the calibration and the stored WiFi credentials.

## HTTP API

All routes sit behind a bearer-token layer: `Authorization: Bearer <API_TOKEN>`. `/api/events` additionally accepts `?token=` — a deliberate exception for `EventSource`, which cannot set headers — and that fallback is rejected on every other route.

| Method | Route | Description |
|---|---|---|
| `GET` | `/api/telemetry` | Latest telemetry snapshot as JSON |
| `GET` | `/api/events` | SSE stream (`event: telemetry`) pushed on every change, with periodic keepalives |
| `GET` | `/api/health` | Liveness/health probe |
| `GET` | `/api/probe` | Raw probe voltages (`ph`, `tds`) with the epoch second they were sampled; `0` means the device has not sampled yet |
| `GET` | `/api/calibration` | Stored calibration: pH buffer points, top-up tank temperature, TDS factor |
| `PATCH` | `/api/calibration` | Partial update, merged onto the stored values; `400` on values the device rejects, `500` when it cannot write them to NVS |
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

curl -H "Authorization: Bearer $API_TOKEN" http://192.168.1.50/api/probe

curl -X PATCH http://192.168.1.50/api/calibration \
  -H "Authorization: Bearer $API_TOKEN" -H 'Content-Type: application/json' \
  -d '{"makeup_tank_celsius":22.0,"tds_factor":1.23}'
```

A `PATCH` carries only the fields being changed. The three pH points are one unit: send all of them or none, because the device validates them together and will not accept half a calibration.

A snapshot covers `system_mode`, `system_health` (uptime, RSSI, free heap), `temperature` (tank/sump/target, heater state, sensor health), `filtration`, `co2` (including the current pH and electrode health), `tds` (parts per million and conductivity), `ato`, `lighting`, `water_level` (per-sensor wet/faulty flags) and `leak_detected`. Raw probe voltages are deliberately **not** part of the snapshot — they are diagnostics for the calibration screen, served by `/api/probe`, and are not exported to InfluxDB.

The same contract is available to TypeScript clients as `packages/api-client`,
generated from these Rust types — see [The HTTP contract](#the-http-contract).

## Project Structure

```text
├── crates/
│   ├── core/                    # aquarium-core — hardware-free, host-testable
│   │   ├── src/
│   │   │   ├── domain/          # light, water, metrics, calibration (DTOs + invariants)
│   │   │   ├── protocol/        # ds18b20, xkc_y25, ads1115, ph, tds, influx line protocol
│   │   │   ├── ports/           # inbound (telemetry, lighting, calibration, probe) / outbound
│   │   │   ├── application/     # collector, water_level sampler, calibration, metrics_export
│   │   │   ├── adapters/
│   │   │   │   ├── inbound/http/  # picoserve router, auth layer, SSE, handlers
│   │   │   │   └── outbound/    # in-memory stores
│   │   │   └── secret.rs        # redacting Secret<T> newtype
│   │   └── tests/               # http_auth/_cors/_telemetry/_lighting/_events/_probe/_calibration
│   └── firmware/                # aquarium-controller — ESP32-S3 firmware
│       ├── .cargo/config.toml   # xtensa target, linker, ESP-IDF env
│       ├── src/
│       │   ├── adapters/outbound/  # ds18b20, xkc_y25, ads1115, nvs_calibration, temperature, influx
│       │   ├── infrastructure/  # wifi, clock (SNTP), system_health
│       │   ├── settings.rs      # compile-time configuration
│       │   └── main.rs          # peripheral wiring, task graph, ROM discovery + ADC probe modes
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

#### Component architecture

Atomic Design, mirroring the conventions of the portfolio design system — each
component is a PascalCase directory holding `types.ts`, `styles.ts` and the
component:

```text
src/app/                  # route files only, each mounting one page
src/components/
├── atoms/                # Button, Input, Card, Alert, Badge, Spinner, TextDisplay
├── molecules/            # FormField, MetricList, ScreenMessage
├── organisms/            # DeviceForm, TelemetryPanel
├── pages/                # TelemetryPage, DeviceSettingsPage
└── utility/              # theme, providers, formatters
src/device/               # form schema, secure storage, fetch seam, SSE hook
```

`components/utility/theme.ts` is the Tokyo Night palette from the portfolio's
`styles/palette.scss`, ported to TypeScript: `rem` spacing resolved to px, Sass
colour functions evaluated to literals, and shadows expressed as React Native
`boxShadow` arrays. Gradient button variants use `expo-linear-gradient`.

The portfolio's components themselves are not reusable here — they are DOM
components using `className` and imported `.scss`, which React Native has no
equivalent for. The atoms are reimplemented on RN primitives with the same prop
vocabulary, so `Button`, `Card`, `Input` and friends take the same variants and
sizes as on the web.

### Continuous integration

Two workflows, split by toolchain and by path:

* `rust_ci.yml` — formatting, clippy (core on host, firmware on xtensa), host tests, contract generation, and a release firmware build, via `esp-rs/xtensa-toolchain`.
* `js_ci.yml` — installs stable Rust only to run `make types`, then lints, type-checks and tests the yarn workspaces.

## Support

The firmware logs through `tracing` to the serial console (`make monitor`). With the `health` component set to `Running`, it reports stack high-water mark, free internal heap and the minimum free heap every 30 seconds, and raises an error-level log once stack headroom drops below 16 KiB. `CONFIG_FREERTOS_WATCHPOINT_END_OF_STACK` is enabled on purpose — it turns a stack overrun into an immediate, legible abort instead of corruption elsewhere.

For a flashed board on the network, `make smoke IP=...` exercises authentication, telemetry, the SSE stream and the full lighting-override lifecycle in one run.
