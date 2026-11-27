# aquarium-controller

**aquarium-controller** is ESP32-S3 firmware for a reef/planted tank controller, written in Rust on top of ESP-IDF. It samples the tank (DS18B20 temperature probes, XKC-Y25 non-contact water-level sensors behind an MCP23017 I2C expander, analog pH and TDS probes behind an ADS1115), drives heaters, pumps, barrel relays and PWM lighting, publishes a single telemetry snapshot over HTTP and SSE, and optionally streams the same snapshots to InfluxDB. The code is split into a hardware-free `aquarium-core` crate — domain, protocols, ports and the HTTP layer, all testable on the host — and a thin firmware crate that wires real peripherals into those ports.

## Hardware

### DS18B20 temperature probes (shared 1-Wire bus)

Both probes sit on one 1-Wire bus. A single 4.7 kΩ pull-up between `3V3` and the data line serves the whole bus — do not add one per probe.

```text
ESP32-S3                               Junction                      Both probes

3V3 ──────────────┬──────────────────  VCC  ───────────────────────  Red
                  │
              [4.7 kΩ]
                  │
GPIO4 ────────────┴──────────────────  DATA ───────────────────────  Yellow / White
(1-Wire data)
GND ─────────────────────────────────  GND  ───────────────────────  Black / Blue
```

The junction is a solder joint or a terminal block: each of the three lines is shared, and both probes hang off it in parallel — sump and main tank are wired identically.

Notes:
* Probes are powered normally (VCC wired), not parasitically. The bus driver still detects parasite-powered devices and reports them.
* Each DS18B20 has a unique 64-bit ROM address, so both probes share the bus and are addressed individually — see [Identifying the probes](#identifying-the-probes).
* Keep the bus below a few metres of cable; for longer runs use shielded/twisted cable and keep the pull-up at the ESP32 end.

### I2C bus

One I2C bus at 400 kHz carries two devices: the ADS1115 analog front-end (`0x48`) and the MCP23017 GPIO expander for the water-level sensors (`0x20`). The firmware opens the bus once, when `ph`, `tds` or `water_level` is not `Off`, and every transaction takes the bus lock for its duration, with a 100 ms timeout.

```text
ESP32-S3                               ADS1115 (0x48)        MCP23017 (0x20)

3V3 ─────────────────────────────────  VDD ───────────────── VDD
GND ─────────────────────────────────  GND ───────────────── VSS
GPIO8 (I2C SDA) ─────────────────────  SDA ───────────────── SDA
GPIO9 (I2C SCL) ─────────────────────  SCL ───────────────── SCL
```

Only one set of pull-ups is needed on `SDA`/`SCL` for the whole bus. Two modules that each carry 10 kΩ give 5 kΩ in parallel, which is still fine at 400 kHz.

### ADS1115 analog front-end

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
* The driver uses PGA ±4.096 V (125 µV per count), single-shot conversions at 128 SPS, and averages 16 samples per reading — about 150 ms per channel. After the nominal conversion time it polls the config register's `OS` bit until the conversion has finished, and gives up with `ADS1115 conversion did not complete in time` after about twice the nominal time. `A2` and `A3` are defined in the driver but unused.
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

### Heater SSRs (Fotek SSR-40 DA)

Each heater is switched by its own solid-state relay on the mains side. The ESP32 drives only the relay's DC input.

```text
ESP32-S3                               SSR input side                Mains side

GPIO17 ──────────────────────────────  SSR 1  3 (+)                  1-2 ── tank heater (via outlet)
GND    ──────────────────────────────         4 (−)
GPIO18 ──────────────────────────────  SSR 2  3 (+)                  1-2 ── sump heater (via outlet)
GND    ──────────────────────────────         4 (−)
```

Notes:
* The GPIO drives the SSR input directly, with no transistor. Fit a 10 kΩ pull-down from each SSR input `3 (+)` to GND: during reset and boot the GPIO floats until the firmware drives it LOW after initialisation, and the pull-down keeps the SSR off in that window instead of relying on the input's LED path.
* 3.3 V sits at the bottom of the SSR-40 DA's 3–32 V input range. Run `RunMode::SsrProbe` (no WiFi or HTTP): it switches one channel at a time — tank SSR on for 2 s, off for 2 s, then the sump SSR — and identifies each heater in the log. If an OFF command fails, the probe retries that channel every 2 s before it can energize the next one. Confirm the input LED lights brightly and a mains-side load actually switches. If a clone stays dim or misses, drive the input through an NPN or logic-level N-MOSFET from 5 V instead.
* An SSR usually fails shorted, which keeps a heater on regardless of firmware. Keep each heater's own thermostat set 1–2 °C above the target as the last line of defence.
* Mount the SSRs on a heatsink: the triac drops about 1–1.5 V at load current.
* Firmware safeguards: a reading at or above `cutoff_celsius` or a missing reading turns a heater off even under a manual ON override, and a heater switches off once its last temperature measurement is 6 s old, however often telemetry is republished. A timed override ends on time, without waiting for the next reading. A GPIO write the driver refuses is reported as `fault: output_error`; if turning a heater off fails, the status keeps showing it as possibly on and the firmware retries OFF on every tick. Targets, hysteresis and the cutoff live in NVS and are set over `PATCH /api/heaters/settings`; overrides live in RAM and do not survive a reboot.
* Servicing: unplug a heater from its outlet before emptying the chamber it sits in, and keep it unplugged until the chamber is refilled. A manual OFF from the app is not a lockout: it lives in RAM, so a restart or power cut brings the heater back under automatic control, and a dry heater with water still below its probe may switch on.
* With both temperature probes missing at boot the controller still starts (API, pumps): both heaters stay off with `sensor_fault` and the firmware retries the 1-Wire bus every 5 s, backing off to once a minute, and checks the probes' power mode before trusting their readings again.
* Enable control with `components.heaters` in `crates/firmware/src/settings.rs`.

### Pump relay module (4-channel, 5 V, optocoupled)

One relay module switches the return pump, the UV lamp, the makeup pump and the drain pump. The ESP32 drives only the module's `IN` pins.

```text
ESP32-S3                               Relay module                  Load side

5 V    ──────────────────────────────  VCC
GND    ──────────────────────────────  GND
GPIO5  ──────────────────────────────  IN1   NC contact ── return pump
GPIO6  ──────────────────────────────  IN2   NC contact ── UV lamp
GPIO7  ──────────────────────────────  IN3   NO contact ── makeup pump
GPIO16 ──────────────────────────────  IN4   NO contact ── drain pump
```

Notes:
* Set the trigger jumper to **High** and leave the `JD-VCC`–`VCC` jumper in place. The `IN` pins are driven directly from the GPIOs — no resistors.
* The return pump and the UV lamp use the **NC** contacts, so they keep running while the ESP32 is dead or resetting. The makeup and drain pumps use the **NO** contacts and stand still in that case. The firmware inverts the coil for NC channels (`Contact` in `crates/firmware/src/adapters/outbound/relay.rs`), so `on` always means the device runs.
* Run `RunMode::RelayProbe` (no WiFi or HTTP) to verify the wiring: it energizes and releases each coil in turn every 2 s and logs `relay channel=… coil=…`.
* Enable control with `components.pumps` in `crates/firmware/src/settings.rs`.
* The relay pins are driven low once the firmware initialises the relay module (`components.pumps` not `Off`). With pump control and flood monitoring off, the pins float unless NVS contains a flood lockout. A stored lockout makes the firmware drive all four loads off at startup.

### Barrel relays (module 2)

The second four-channel module shares the ESP32 ground and uses the same 5 V supply, VCC/JD-VCC jumper and High trigger setting as the pump module. Wire all four loads to normally open contacts. Both inlet valves are normally closed and the booster sits upstream of the RO membrane.

```text
supply ─┬─ RO valve (GPIO40) ─ booster (GPIO41) ─ RO membrane ─┐
        └─ tap valve (GPIO42) ────────────────────────────────┴─ barrel
                                                                 mixer (GPIO47), TDS (A0)
```

`RunMode::RelayProbe` exercises the pump module first. On module 2 it opens RO, starts the booster two seconds later, stops the booster, closes RO, then cycles the tap valve and mixer individually. Verify GPIO40/41/42/47 through reset and the bootloader before connecting the loads. GPIO1 and GPIO2 drive the [LED MOSFETs](#led-mosfet-outputs).

### LED MOSFET outputs

Two PWM outputs dim the lighting through a MOSFET module: one channel for the white LEDs, one for the RGB strip.

```text
ESP32-S3                               MOSFET module                 Load side

GPIO1  ──────────────────────────────  PWM in, channel 1             white LEDs
GPIO2  ──────────────────────────────  PWM in, channel 2             RGB LEDs
GND    ──────────────────────────────  GND
```

Notes:
* Both channels share LEDC `TIMER0` at 5 kHz (`lighting.pwm_freq_hz`) with 12-bit resolution; white uses `CHANNEL0`, RGB `CHANNEL1`. A percentage maps to a duty cycle rounded to the nearest step, values above 100 % are clamped.
* Both outputs are driven to 0 % as soon as the firmware initialises them (`components.lighting` not `Off`). Before that the GPIOs float; fit a pull-down on each gate input if the module does not carry one.
* With `components.lighting` set to `Running`, the lighting loop resolves the schedule and the active override every 30 s (`lighting.update_interval`), immediately after an override is set or cleared over the API, and right after a timed override expires. It writes the outputs only when the levels change; a refused write is logged and retried on the next pass. Without a synchronised clock the schedule resolves to `off`.
* The supplied profile leaves `components.lighting` `Off`.

### Floor flood probes

| Probe location | Isolated status contact | Alarm input |
|---|---|---|
| Under the makeup reservoir | Channel 1 | GPIO38 |
| Under the sump | Channel 2 | GPIO48 |

The detector is powered from its specified supply (24 V or 5 V). Its hardware interlock must interrupt the common +24 V feed to both normally closed solenoid valves whenever either zone trips, including while the ESP32 is powered down. Use a dedicated valve interlock contact, or a separate isolated pole per zone with the two valve poles in series. Size the switching contacts for the valves' DC inductive load.

The ESP32 needs **two independent dry status contacts**, one per probe. Each connects its input to ESP ground while the detector is healthy and the probe is dry. The internal pull-up reads an open circuit as an alarm, including a broken status wire. Select the module's operating mode and contact terminals for this behaviour and test loss of detector power. The 5 V or 24 V supply and the valve circuit must stay isolated from both GPIO inputs. A single changeover contact shares its common terminal and cannot serve both circuits.

In normal firmware set `components.flood` to `ComponentMode::Running` after commissioning. The supplied bench profile leaves it `Off`. Check the board revision before wiring: ESP32-S3-DevKitC-1 uses GPIO48 for the onboard RGB LED on v1.0 and GPIO38 on v1.1. Confirm both inputs reach valid high and low levels with the actual board. [Espressif board FAQ](https://docs.espressif.com/projects/esp-faq/en/latest/hardware-related/development-board.html)

The firmware samples the contacts every 100 ms. Either alarm stops return, UV, makeup and drain, even with barrel or pump automation disabled. A running fill or water transfer is interrupted. The dashboards show which zone is active. The pump lockout survives reboot when written to NVS. Once both probes report dry, acknowledge the alarm on the dashboard or send an authenticated `DELETE /api/flood/alarm`. If barrel automation recorded its own leak fault, also clear it via `DELETE /api/reservoir/fault` after the global lockout clears. With the ESP32 unpowered, the existing NC return and UV contacts revert to their energised-load state. Only the valve cutoff is independent of the controller.

### XKC-Y25 water-level sensors

Eight non-contact sensors read the water through the glass. Their outputs go to an MCP23017 GPIO expander on the [I2C bus](#i2c-bus); they share only the supply.

```text
MCP23017 (0x20)                        XKC-Y25 (×8)                  Mounted at

                     5 V ────────────  VCC (brown)
                     GND ────────────  GND (blue)
GPA0 ────────────────────────────────  OUT (yellow)                  sump mechanical chamber                     sump_mechanical
GPA1 ────────────────────────────────  OUT (yellow)                  sump biological chamber                     sump_biological
GPA2 ────────────────────────────────  OUT (yellow)                  sump chamber 5 (return pump), low           sump_pump
GPA3 ────────────────────────────────  OUT (yellow)                  main tank, overflow level                   main_tank
GPA4 ────────────────────────────────  OUT (yellow)                  makeup reservoir, bottom                    sump_reserve
GPA5 ────────────────────────────────  OUT (yellow)                  sump chamber 5, at the operating level      sump_return_level
GPA6 ────────────────────────────────  OUT (yellow)                  makeup reservoir, mid height                reservoir_ready
GPB0 ────────────────────────────────  OUT (yellow)                  makeup reservoir, top                       reservoir_full

A0, A1, A2 ── GND (address 0x20)
RESET ─────── 3V3
```

Notes:
* The firmware expects an open-collector output that pulls the line low over water (`level.polarity: ActiveLow` in `crates/firmware/src/settings.rs`). Every expander input has its internal 100 kΩ pull-up enabled, so no resistors are needed, and the 5 V supply never reaches the expander.
* Current MCP23017 silicon specifies `GPA7` and `GPB7` as output-only. The firmware drives both low and configures the other 14 pins as pulled-up inputs, so unused inputs never float. The sensor-to-pin map is `LEVEL_SENSOR_PINS` in `crates/firmware/src/board.rs`, checked at compile time against the input mask.
* `RESET` is active low and has no internal pull-up: it must be tied to `3V3` (most breakout modules do this on board). The firmware does not drive it.
* The sampler reads both ports in one transaction every 100 ms. A failed read publishes nothing and does not count as a sample: the last published levels age out after `level_stale`, and the pump and barrel controllers then treat the levels as unavailable. The first failure and the recovery are logged once each.
* `sump_return_level` sits in the return-pump chamber, the only chamber whose level moves with a top-off or a refill. It is the default refill-stop sensor. A sensor in a baffled chamber only gets wet once the whole sump has flooded above the baffles.
* `reservoir_ready` marks enough water for a water change, and `reservoir_full` a full reservoir. `sump_reserve` marks an empty reservoir and protects the makeup pump.
* Enable sampling with `components.water_level` in `crates/firmware/src/settings.rs`.

### Pin map

| Peripheral | Pin(s) | Mode |
|---|---|---|
| DS18B20 1-Wire bus (tank + sump) | `GPIO4` | open-drain input/output, external 4.7 kΩ pull-up |
| I2C bus (ADS1115, MCP23017) — data | `GPIO8` | I2C master at 400 kHz, pull-up on the module |
| I2C bus (ADS1115, MCP23017) — clock | `GPIO9` | I2C master at 400 kHz, pull-up on the module |
| pH electrode (analog front-end) | ADS1115 `A1` | single-ended, PGA ±4.096 V, single-shot 128 SPS |
| TDS probe (analog front-end) | ADS1115 `A0` | single-ended, PGA ±4.096 V, single-shot 128 SPS |
| XKC-Y25 — sump mechanical chamber | MCP23017 `GPA0` | input, expander pull-up, active-low |
| XKC-Y25 — sump biological chamber | MCP23017 `GPA1` | input, expander pull-up, active-low |
| XKC-Y25 — sump pump chamber | MCP23017 `GPA2` | input, expander pull-up, active-low |
| XKC-Y25 — main tank | MCP23017 `GPA3` | input, expander pull-up, active-low |
| XKC-Y25 — makeup reservoir, bottom (empty) | MCP23017 `GPA4` | input, expander pull-up, active-low |
| XKC-Y25 — sump return chamber, operating level | MCP23017 `GPA5` | input, expander pull-up, active-low |
| XKC-Y25 — makeup reservoir, mid height (ready for a water change) | MCP23017 `GPA6` | input, expander pull-up, active-low |
| XKC-Y25 — makeup reservoir, top (full) | MCP23017 `GPB0` | input, expander pull-up, active-low |
| LED MOSFET — white channel | `GPIO1` | LEDC PWM, `TIMER0`/`CHANNEL0`, 5 kHz, 12-bit, 0 % once initialised |
| LED MOSFET — RGB channel | `GPIO2` | LEDC PWM, `TIMER0`/`CHANNEL1`, 5 kHz, 12-bit, 0 % once initialised |
| Floor probe — under the reservoir | `GPIO38` | dry contact to ESP ground, internal pull-up, open = alarm |
| Floor probe — under the sump | `GPIO48` | dry contact to ESP ground, internal pull-up, open = alarm |
| Heater SSR — main tank | `GPIO17` | push-pull output, low at boot, drives the SSR input directly |
| Heater SSR — sump | `GPIO18` | push-pull output, low at boot, drives the SSR input directly |
| Pump relay IN1 — return pump (NC) | `GPIO5` | push-pull output, driven low once the relay module is initialised, drives the relay module input directly |
| Pump relay IN2 — UV lamp (NC) | `GPIO6` | push-pull output, driven low once the relay module is initialised, drives the relay module input directly |
| Pump relay IN3 — makeup pump (NO) | `GPIO7` | push-pull output, driven low once the relay module is initialised, drives the relay module input directly |
| Pump relay IN4 — drain pump (NO) | `GPIO16` | push-pull output, driven low once the relay module is initialised, drives the relay module input directly |
| Barrel relay IN1 — RO valve | `GPIO40` | active-high output, NO contact |
| Barrel relay IN2 — booster | `GPIO41` | active-high output, NO contact |
| Barrel relay IN3 — tap valve | `GPIO42` | active-high output, NO contact |
| Barrel relay IN4 — mixer | `GPIO47` | active-high output, NO contact |

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
* **Water level with fault detection:** eight XKC-Y25 inputs read through an MCP23017 expander and sampled at 10 Hz, debounced over a sample window, with flap detection that marks a chattering sensor faulty instead of acting on it.
* **pH and TDS on one external ADC:** an ADS1115 over I2C serves both probes; 16 averaged samples per reading at 16-bit resolution. pH goes through a two-point calibration with a nominal Nernst slope and sump-temperature compensation; TDS through DFRobot's polynomial with a hand-entered compensation temperature and a correction factor.
* **Calibration over the API, persisted in NVS:** buffer points, the top-up tank temperature and the TDS factor are set from the apps and survive a reboot. The write goes to flash before the value takes effect, so a calibration that cannot be stored is refused rather than silently lost at the next power cycle.
* **HTTP API + SSE:** a bearer-token-guarded `picoserve` router serving telemetry snapshots, a live `text/event-stream` feed, a health probe, lighting override control, and pump control.
* **Lighting:** a seasonal schedule and overrides — a profile (`off`, `sunrise`, `daylight`, `sunset`, `moonlight`) or explicit channel percentages, optionally time-boxed — drive two PWM channels (white, RGB) through LED MOSFETs. With the lighting loop running, an override reaches the outputs and telemetry immediately rather than waiting for the next sample tick.
* **Makeup barrel.** The second relay module fills the barrel with RO or mixes RO and tap water to a target TDS. Telemetry includes the barrel state. InfluxDB receives snapshots in `aquarium_reservoir` and fill records in `aquarium_reservoir_fill`.
* **Pumps, UV, top-off and water changes:** four relay channels (return pump, UV lamp, makeup pump, drain pump) with water-level interlocks, cron-scheduled or on-demand top-offs and water changes, OFF overrides, runtime counters, and checkpoints in NVS so an interrupted operation resumes after a restart.
* **InfluxDB 2.x export:** optional, compile-time gated on the `INFLUX_*` variables being present. Snapshots are buffered and batched, with bounded retry backoff, on a dedicated worker.
* **Component modes:** every subsystem (WiFi, HTTP, telemetry, temperature, water level, pH, TDS, Influx, heaters, pumps, flood, reservoir, lighting, health) is independently `Off` / `Initialized` / `Running`, so peripherals can be brought up without being wired into the runtime — useful when bisecting a hardware fault.
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
* `make erase-nvs` — wipe only the NVS partition; the service step when the firmware reports the partition as unusable
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

`Settings.timezone` (a POSIX TZ string, default `CET-1CEST,M3.5.0,M10.5.0/3`) sets the local time used by the lighting schedule and the pump cron schedules.

Calibration is the exception: `sensors.calibration` holds only the **defaults used when NVS is empty**. Once a calibration has been written over the API it comes from flash, and editing the constants no longer changes what the device uses. `make erase` wipes NVS — and with it both the calibration and the stored WiFi credentials.

The firmware never erases NVS on its own. If the partition cannot be opened (for example `ESP_ERR_NVS_NO_FREE_PAGES` or `ESP_ERR_NVS_NEW_VERSION_FOUND` after a firmware change), it leaves the data untouched, logs the error code on the serial console every 30 s, and boots in a degraded mode: heaters regulate on the `settings.rs` defaults, calibration uses its defaults, settings writes fail (`503` for the barrel), and pump and barrel automation stay stopped with their relays released. To recover, read out anything worth keeping, then run `make erase-nvs` (or `espflash erase-parts --partition-table crates/firmware/partitions.csv nvs`) and reflash or restart; settings, calibration, pump schedules and WiFi data start from scratch.

Barrel control is opt-in. Set `components.reservoir`, `water_level`, `tds` and `telemetry` to `Running` and keep NVS available. In `Initialized` the barrel relays stay LOW. A running barrel controller also requires active watchdog supervision to energize an output.

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
| `GET` | `/api/heaters` | Status of both heaters (`tank`, `sump`) |
| `GET` | `/api/heaters/settings` | Stored thermostat settings: per-heater target and hysteresis, shared cutoff |
| `PATCH` | `/api/heaters/settings` | Partial update, merged and validated as a whole; `400` on values the device rejects, `500` when it cannot write them to NVS |
| `PUT` | `/api/heaters/{tank\|sump}/override` | Force a heater `on` or `off`; `on` needs `duration_s` of 1–3600, `off` without one lasts until cleared; `400` otherwise, `404` on an unknown heater |
| `DELETE` | `/api/heaters/{tank\|sump}/override` | Return the heater to the thermostat (`204`) |
| `GET` | `/api/pumps` | Status of the four channels, the current operation, the last top-off and water change, and the last skipped window |
| `GET` | `/api/pumps/settings` | Stored pump settings: top-off and water-change cron windows, flow rates, resume window, refill-stop sensor, minimum gap between water changes, usable reservoir volume |
| `PATCH` | `/api/pumps/settings` | Partial update, merged and validated as a whole; `400` on values the device rejects, `500` when it cannot write them to NVS |
| `PUT` | `/api/pumps/{return\|uv}/override` | Force a channel OFF; `duration_s` is required for `return` (60–7200 s) and optional for `uv` (60–2592000 s; without it the override lasts until cleared); `400` otherwise, `404` on `makeup` or `drain` |
| `DELETE` | `/api/pumps/{return\|uv}/override` | Clear the override (`204`); `404` on `makeup` or `drain` |
| `POST` | `/api/pumps/{id}/runtime/reset` | Zero the channel's runtime counter (`204`) |
| `POST` | `/api/pumps/top-off` | Start a top-off of `duration_s` seconds (`202`); `400` on an invalid duration, `409` while another operation runs, `423` when blocked |
| `POST` | `/api/pumps/top-off/resume` | Resume a suspended top-off (`202`); `409` when none is suspended, `423` when blocked |
| `DELETE` | `/api/pumps/top-off` | Cancel the top-off (`204`) |
| `POST` | `/api/water-change` | Start a water change from `drain_s`, `settle_s` and `refill_s` (`202`); `400` on an invalid plan or a drain larger than `reservoir_ready_l`, `409` while another operation runs, `423` when blocked or water levels are unavailable |
| `POST` | `/api/water-change/resume` | Resume a suspended water change (`202`); `409` when none is suspended, `423` when blocked |
| `DELETE` | `/api/water-change` | Cancel the water change (`204`) |
| `GET` | `/api/reservoir` | Channel states, interlocks, runtime, active fill, last result, fault and restart holds |
| `GET` / `PATCH` | `/api/reservoir/settings` | Read or update settings in NVS. Set `target_tds_ppm` to `null` for RO only |
| `PUT` / `DELETE` | `/api/reservoir/{ro\|tap\|mixer}/override` | Set an override with `{"on":true,"duration_s":60}` or clear it (`204`). An OFF override without a duration lasts until cleared or rebooted |
| `POST` | `/api/reservoir/{channel}/runtime/reset` | Zero the counter for `ro_valve`, `booster`, `tap_valve` or `mixer` (`204`) |
| `POST` / `DELETE` | `/api/reservoir/fill` | Start a manual fill (`202`) or cancel it (`204`) |
| `DELETE` | `/api/flood/alarm` | Acknowledge the pump lockout after both zones report dry (204, 423 while wet or unavailable, 503 on a persistence error) |
| `DELETE` | `/api/reservoir/fault` | Clear the persistent lockout (`204`) |

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

curl -X PUT http://192.168.1.50/api/heaters/tank/override \
  -H "Authorization: Bearer $API_TOKEN" -H 'Content-Type: application/json' \
  -d '{"state":"on","duration_s":600}'

curl -X PATCH http://192.168.1.50/api/heaters/settings \
  -H "Authorization: Bearer $API_TOKEN" -H 'Content-Type: application/json' \
  -d '{"sump":{"target_celsius":26.5,"hysteresis_celsius":0.3}}'

curl -X POST http://192.168.1.50/api/water-change \
  -H "Authorization: Bearer $API_TOKEN" -H "Content-Type: application/json" \
  -d '{"drain_s":300,"settle_s":30,"refill_s":420}'

curl -X PATCH http://192.168.1.50/api/pumps/settings \
  -H "Authorization: Bearer $API_TOKEN" -H "Content-Type: application/json" \
  -d '{"water_change":[{"cron":"0 10 * * sun","drain_s":300,"refill_s":420}],"flow_lph":{"makeup":300,"drain":600},"reservoir_ready_l":60}'
```

A `PATCH` carries only the fields being changed. The three pH points are one unit: send all of them or none, because the device validates them together and will not accept half a calibration. The same holds for a heater zone in `/api/heaters/settings`: `target_celsius` and `hysteresis_celsius` travel together.

Limits: targets 15–32 °C, hysteresis 0.1–2.0 °C, and a cutoff of at most 35 °C that must sit above both targets. A manual `on` never beats the safeguards — at or above the cutoff, or without a temperature reading, the heater stays off and its status says why. Overrides live in RAM; with `components.heaters` not running the API still accepts them, but nothing acts on them and the status reports `unavailable`.

Pump safeguards: a dry `sump_pump` chamber stops the return pump and the UV lamp, which restart once it has been wet for 30 s. Missing water-level data stops the makeup and drain pumps. A wet `main_tank` sensor ends refilling with the result `overflow`, which counts as a success.

* **Water-change sequence:** the return pump and the UV lamp stay off while draining and settling (`blocked_by: water_change` on the return pump, `return_off` on the UV), so the drain lowers the tank by `drain_s` × drain flow instead of emptying the return chamber. Draining ends after `drain_s` of drain-pump time; a dry `sump_pump` sensor no longer ends it. While refilling, the makeup pump fills the sump and the return pump moves the water to the tank in batches: it stops when `sump_pump` goes dry and restarts after 30 s wet, until the tank is back at the overflow weir. A resumed water change that was draining or settling keeps the return pump off. Starting either water mover requires a successful OFF command to the other mover in the same control cycle, and the drain also requires a successful OFF command to the return pump.
* **Refill-stop sensor:** `refill_stop_sensor` (`sump_return_level`, `sump_mechanical`, `sump_biological` or `null`, default `sump_return_level`) names the sump sensor that marks a full sump. While it is wet, or faulty, the makeup pump is blocked (`sump_full`), a refill ends with the result `sump_full` (a success, like `overflow`), and a top-off does not start (`423`). A water-change refill ends on `sump_full` only after the makeup pump has run in that phase: the sump starts the refill flooded by the water that drained back while the return pump was off, so the refill waits until the return pump has lowered the return chamber, and the next `sump_full` ends it. In a `PATCH`, leaving the field out keeps it and `null` clears it. Settings stored without the field load with the default. A stored `null` stays cleared.
* **Reservoir ready:** a water change, scheduled or over the API, starts only while `reservoir_ready` is wet and not faulty (`423`, `blocked_by: reservoir_low` on a skipped window). A top-off needs only `sump_reserve`, and resuming an interrupted water change does not check `reservoir_ready`. `reservoir_full` has no control effect and is reported in telemetry only.
* **Reservoir volume:** `reservoir_ready_l` (1–10000 L or `null`, default `null`) is the water the reservoir holds above the `sump_reserve` empty sensor when `reservoir_ready` is wet. With both `reservoir_ready_l` and `flow_lph.drain` set, a water change whose planned drain `drain_s` × `flow_lph.drain` / 3600 exceeds `reservoir_ready_l` is refused: over the API with `400` ("the planned drain exceeds the water the reservoir holds"), and as a schedule window with `400` on the `PATCH` that saves it. A `PATCH` that changes the flow rate or the volume so that a stored window no longer fits is refused the same way. With either value unset there is no check, so a drain larger than the reservoir ends the refill with `reservoir_empty` and leaves the tank short. Setting both is strongly recommended. In a `PATCH`, leaving the field out keeps it and `null` clears it.
* **Refill timeout:** a refill phase (a top-off, or the refill of a water change) ends with the result `refill_timeout`, a failure, once it has lasted longer than `max(2 × refill_s, refill_s + 300)` seconds of controller time, even if the makeup pump never ran. This catches a refill-stop sensor stuck wet, which would otherwise keep the makeup pump blocked, the refill waiting forever and every other operation refused with `409`. The budget counts time the refill was stalled for any reason, including a return pump override, but not time the controller was off: the elapsed refill time is stored in the checkpoint, and a resumed refill continues from it. The result shows up in `last_top_off` / `last_water_change` and in the Influx `aquarium_pump_operation` point as `result=refill_timeout`.
* **Drain timeout:** a water change whose drain has not reached `drain_s` of drain-pump time within `max(2 × drain_s, drain_s + 300)` seconds of controller time ends with the result `drain_timeout`, a failure. This catches a drain that never starts, for example while a failed OFF command on the makeup pump keeps the interlock closed. The return pump and the UV lamp restart once the levels allow it, and a drain that never ran does not count towards `min_water_change_gap_h`.
* **Level outage after a drain:** losing the level readings during a water change, or within 30 min after one that did not finish as `completed` or `sump_full`, holds the return pump and the UV lamp off (`levels_unavailable_after_drain`) until safe readings return. The hold is stored in NVS (`pump_hold`) and survives a reboot, however long the outage. Before SNTP synchronises the clock, such an unfinished last water change counts as recent during the first 30 min of uptime.
* **Estimated litres:** `drained_l` and `refilled_l` (in the operation status and the Influx `aquarium_pump_operation` point) are estimates, not measurements: run time × the rated `flow_lph`. They appear only when the matching flow rate is set. A drain that kept running after the tank fell to the drain intake still counts that dry time, so `drained_l` then overstates the water actually removed; `refilled_l` likewise assumes the makeup pump delivered its rated flow the whole time.
* **Override duration:** a timed override set while the clock is synchronised lasts its `duration_s` of controller time, even if SNTP later steps the clock; the stored epoch expiry is used only after a restart.
* **Return pump override:** while the return pump has an OFF override, the makeup pump is blocked (`return_off`) and top-offs and water changes do not start (`423`). A running refill pauses until the override ends. The 30 s restart lockout after a dry sump does not block the makeup pump, since only a refill can wet the sump again.
* **Water-change gap:** `min_water_change_gap_h` (0–168, default 12, 0 disables it) refuses a water change, scheduled or over the API, that would start sooner than this after the start of the previous water change that removed water (`423`, `blocked_by: recent_water_change` on a skipped window). A water change cancelled or interrupted before its drain pump ran does not count. Without a synchronised clock or a recorded start time the limit does not apply.
* **Incomplete refill:** a water change whose refill ended as `completed` or `sump_full` is recorded as `incomplete` (a failure) when the refill estimate stayed below 70 % of the drain estimate (both run time × `flow_lph`, only with both flows set), or when `sump_pump` still read dry (or faulty) as the refill ended. A drain that ran past the drain intake overstates `drained_l` and is graded `incomplete` too. Top-offs are not graded.
* **Starved circulation:** `circulation_starved` (in `/api/pumps`, in telemetry `pumps` and in the Influx `aquarium_pumps` point) turns on when, outside a running top-off or water change, `sump_pump` goes dry three times within 10 min or stays dry for 10 min, and turns off once it has stayed wet for 10 min. It reports only; it changes no output.
* **Sensor plausibility:** `sump_return_level` wet over a dry `sump_pump`, `reservoir_full` wet over a dry `reservoir_ready`, and `reservoir_ready` wet over a dry `sump_reserve` contradict the mounting order, so both sensors of the pair are reported faulty and the pumps treat them as dangerous.
* **Boot:** the water-level sampler publishes nothing until every sensor has given `debounce_samples` agreeing readings (about 1 s), so the pump loop sees no levels at first and the return pump keeps running instead of reacting to the filters' starting value.
* **Restarts:** an operation interrupted by a restart resumes if the checkpoint is younger than `resume_window_s`, and is suspended otherwise. A wet or faulty tank overflow or sump flood sensor blocks both automatic and manual drain resumption. An operation resumes automatically at most once. After a second interruption it stays suspended until resumed over the API. While the checkpoint cannot be stored, the drain pump stays off (`storage_unavailable`) and continues once a checkpoint write succeeds, so a restart repeats at most one checkpoint interval of draining.
* **Cancellation:** the controller replaces the live checkpoint with a finished record that includes the drain history, then archives it. If storage cannot record the cancellation, the operation stops locally and repeated `DELETE` requests for it return `503` until cancellation is durable. A reboot can recover the 12-hour gap from the finished record even while the history key is unwritable.
* **Watchdog:** with `components.pumps` running, the firmware reconfigures the ESP-IDF task watchdog to a 10 s timeout that panics and reboots. The main task is subscribed and fed every 2 s, but only while the heartbeats of all running control loops keep advancing, so a stalled pump loop or a blocked main task resets the board and leaves the relays to their fail-safe contacts. While supervision is unavailable, new top-offs and water changes are refused and the makeup pump stays off (`supervision_unavailable`), also in a running operation, so a water change that reaches its refill ends with `refill_timeout`.

Cron schedules are evaluated in local time (`Settings.timezone`), and need an SNTP-synchronised clock to fire. SNTP starts only with WiFi (`components.wifi` not `Off`), so without WiFi the schedules never fire. A window fires at most once per local minute (date, hour and minute) within 24 h, so the repeated hour when daylight saving time ends, or SNTP stepping the clock back, does not fire it again. A restart within the minute in which a window already started its operation does not fire it again either. A clock step forward skips the windows in between without a `last_skipped` record.

> **Avoid pump windows between 02:00 and 02:59 local time.** With the default `CET-1CEST` zone that hour does not exist on the last Sunday of March, so a window there (for example `30 2 * * *` or `30 2 * * sun`) silently does not run that day, with no `last_skipped` record.

A snapshot covers `system_mode`, `system_health` (uptime, RSSI, free heap), `temperature` (tank and sump readings, sensor health), `heaters` (per heater: `active`, `mode` — `auto`, `manual_on` or `manual_off` — `fault` — `sensor_fault`, `over_temperature`, `unavailable` or `null` — `override_expires_in_s` and `target_celsius`), `filtration`, `co2` (including the current pH and electrode health), `tds` (parts per million and conductivity), `ato`, `pumps` (per channel on/off state plus the running operation's kind and state), `lighting`, `water_level` (wet/faulty flags for `sump_mechanical`, `sump_biological`, `sump_pump`, `main_tank`, `sump_reserve`, `sump_return_level`, `reservoir_ready` and `reservoir_full`, plus `sensors_healthy`, exported to InfluxDB as `aquarium_water_level,sensor=<name>`) and `leak_detected` (at least one current floor alarm). The `flood` section adds `probes.reservoir`, `probes.sump` and the persistent `latched` lockout. Before sampling, `probes` is null. Clearing a restored lockout requires running flood monitoring and a dry reading from both probes. InfluxDB records these as `flood_reservoir`, `flood_sump` and `flood_latched` on `aquarium_health`. Raw probe voltages are deliberately **not** part of the snapshot — they are diagnostics for the calibration screen, served by `/api/probe`, and are not exported to InfluxDB.

The same contract is available to TypeScript clients as `packages/api-client`,
generated from these Rust types — see [The HTTP contract](#the-http-contract).

### Barrel fill operation and API responses

* `auto_fill` defaults to false. Once enabled, it starts a fill after the full sensor has been continuously dry for `refill_debounce_s` (default 10 min). Wet, faulty or unavailable readings reset the timer.
* With a target TDS, RO primes an empty barrel until the mixer is submerged. After mixing, a fresh TDS reading below target selects a tap pulse. At or above target the controller selects RO. At full level it stops the inlets and checks the mixed water against the tolerance.
* During a water change both inlets and the booster stop. The mixer can keep running and the fill can continue afterwards. Each inlet defaults to 90 min of cumulative use followed by at least 15 min of rest.
* Measure `empty_fill_s` with RO alone. If combined valve-open time exceeds 120% of that value while the full sensor is dry, all outputs stop and NVS retains an `overtime` fault. A detected leak also locks all outputs. Inspect the cause before clearing a fault.
* An interrupted fill sets `held_after_reboot` until an accepted manual fill. An automatic fill ending in `timeout`, `blocked`, `tds_unavailable` or `aborted` sets `auto_latched`, which survives restarts. A faulty full sensor during a fill closes the inlets and ends the fill as `blocked`. An accepted manual fill or a healthy wet full sensor clears that latch.

For each request, the controller returns the first applicable refusal in order.

| Manual fill request | HTTP | Response body |
|---|---|---|
| Fill already running | 409 | `a barrel fill is already running` |
| Stored fault | 423 | `the barrel fill is locked by a fault; clear it with DELETE /api/reservoir/fault` |
| Wet leak sensor | 423 | `the leak sensor is wet` |
| Watchdog unavailable | 423 | `the controller watchdog is not supervising the barrel` |
| Levels unavailable | 423 | `barrel level readings are unavailable` |
| Uncertain inlet output | 423 | `the other inlet valve may still be open` |
| Full or faulty full sensor | 423 | `the barrel is full` |
| Water change running | 423 | `a water change is running; the barrel fills after it` |
| TDS target set and no fresh reading in 30 s | 423 | `no fresh TDS reading for the target` |
| Start marker write failed | 503 | `the fill could not be recorded` |
| Accepted | 202 | Empty |

| Override request | HTTP | Response body |
|---|---|---|
| Unknown target | 404 | |
| ON without a duration | 400 | `an ON override needs duration_s` |
| Duration outside 1–600 s | 400 | `an override lasts 1 to 600 s` |
| ON during a fill | 409 | `a barrel fill is running; cancel it before overriding` |
| ON with a stored fault, wet leak sensor, missing supervision or unavailable levels | 423 | Same reason as for a manual fill |
| Inlet ON with uncertain output or the other inlet forced on | 423 | `the other inlet valve may still be open` |
| Inlet ON when full | 423 | `the barrel is full` |
| Inlet ON during a water change | 423 | `a water change is running; the barrel fills after it` |
| Inlet ON during its rest period | 423 | `the inlet is resting after a long run` |
| Mixer ON when empty | 423 | `the barrel is empty, the mixer would run dry` |
| Accepted ON or OFF | 200 | Updated barrel status |

Cancelling an idle or active fill returns `204`. If NVS cannot retire the marker it returns `503` with `cancelled, but not recorded; the controller keeps retrying`. Clearing a fault returns `204`, including when already clear, `423 the leak sensor is wet` during a leak, or `503 the fault could not be cleared in storage` on a write failure.

## Project Structure

```text
├── crates/
│   ├── core/                    # aquarium-core — hardware-free, host-testable
│   │   ├── src/
│   │   │   ├── domain/          # lighting, water, heater, pump, reservoir, flood, cron, metrics, calibration
│   │   │   ├── protocol/        # ds18b20, xkc_y25, ads1115, ph, tds, influx line protocol, settings_record
│   │   │   ├── ports/           # inbound (telemetry, lighting, calibration, probe, heater, pump, reservoir) / outbound
│   │   │   ├── application/     # collector, water_level sampler, calibration, heater, pump, reservoir, lighting loop, metrics_export
│   │   │   ├── adapters/
│   │   │   │   ├── inbound/http/  # picoserve router, auth layer, SSE, handlers
│   │   │   │   └── outbound/    # in-memory stores
│   │   │   └── secret.rs        # redacting Secret<T> newtype
│   │   └── tests/               # http_* route tests, e2e_* scenario tests on the simulator
│   └── firmware/                # aquarium-controller — ESP32-S3 firmware
│       ├── .cargo/config.toml   # xtensa target, linker, ESP-IDF env
│       ├── src/
│       │   ├── adapters/outbound/  # i2c/ (shared bus, ads1115, mcp23017), ds18b20, xkc_y25, ssr, relay, flood, lighting, influx, nvs_*
│       │   ├── infrastructure/  # wifi, clock (SNTP), system_health
│       │   ├── startup/         # task graph: sensors, control loops, network, monitoring
│       │   ├── board.rs         # peripheral and pin assignment
│       │   ├── diagnostics.rs   # ROM discovery, ADC, SSR and relay probe modes
│       │   ├── settings.rs      # compile-time configuration
│       │   └── main.rs          # entry point, run-mode dispatch
│       ├── partitions.csv       # 8 MiB layout (3 MiB factory app + SPIFFS)
│       └── sdkconfig.defaults   # CPU/PSRAM/stack/socket tuning
├── packages/
│   ├── api-client/              # typed HTTP + SSE client, generated from the Rust DTOs
│   │   └── src/generated/       # ts-rs output — not committed, run `make types`
│   ├── view-model/              # client-side view models shared by the apps
│   ├── i18n/                    # translations
│   └── theme/                   # design tokens
├── apps/
│   ├── mobile/                  # Expo client (expo-router, TanStack Query, react-hook-form)
│   │   └── src/
│   │       ├── app/             # routes only, one screen each
│   │       ├── components/      # atoms / molecules / organisms / pages / utility
│   │       └── device/          # config schema, secure storage, fetch seam, SSE hook
│   └── web/                     # web client, served by Caddy (see docker/README.md)
├── docker/                      # InfluxDB + Caddy stack
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
