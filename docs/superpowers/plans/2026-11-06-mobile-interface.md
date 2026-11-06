# Mobile interface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **NO COMMITS.** The user owns this repository's git history. Every task ends in a
> **Checkpoint** that verifies by running commands — never `git add`, never `git commit`.

**Goal:** Replace the scaffold telemetry screen and two-entry Stack with a standard
mobile app: bottom tabs, a card-based telemetry dashboard, and a lighting control
screen wired to the override hooks that already exist but have no UI.

**Architecture:** Pure selectors turn one `TankTelemetryDto` into per-card data
(`status` + `Metric[]`), so every card's dot and rows come from one source and are
unit-testable without a renderer. Screens compose existing atoms; all new style
definitions go into `@aquarium/theme` so no colour or metric lives in a component.
The data layer is untouched — the same TanStack Query hooks and SSE stream.

**Tech Stack:** Expo SDK 58, expo-router Tabs, `@expo/vector-icons`,
`@react-native-community/slider` 5.2.2, TanStack Query 5, Vitest 5.

**Spec:** `docs/superpowers/specs/2026-11-06-mobile-interface-design.md`

---

## File Structure

**`packages/theme/src/index.ts`** — extended with `Chip`, `Slider`, `StatusDot` and
card style definitions. Still the single place any colour or dimension is written.

**`apps/mobile/src/components/utility/`**

| File | Responsibility |
| --- | --- |
| `format.ts` | value → display string, including the em dash for `null` |
| `cards.ts` | `CardStatus`, `CardData`, and one selector per card |
| `water-level.ts` | the five `WaterLevelEntryDto` → chip states |
| `lighting-request.ts` | picker state → `SetOverrideRequest` |
| `use-countdown.ts` | ticking countdown for `expires_in_s` |

**`apps/mobile/src/components/atoms/`** — `Chip/`, `Slider/`, `StatusDot/` added to
the existing `Alert/ Badge/ Button/ Card/ Input/ Spinner/ TextDisplay/`.

**`apps/mobile/src/components/molecules/`** — `MetricCard/`, `WaterLevelGrid/`,
`ProfilePicker/`, `DurationPicker/`, `ChannelSliders/` added to the existing
`FormField/ MetricList/ ScreenMessage/`.

**`apps/mobile/src/components/organisms/`** — `TelemetryDashboard/` and
`LightingControl/` added; `TelemetryPanel/` is deleted, its `stream-badge.ts`
moving to `utility/` because the tab header now uses it too.

**`apps/mobile/src/components/pages/`** — `TelemetryPage/` rebuilt, `LightingPage/`
new, `DeviceSettingsPage/` unchanged.

**`apps/mobile/src/app/`** — routes move under `(tabs)/`.

---

## Task 1: Slider dependency and theme additions

**Files:**
- Modify: `apps/mobile/package.json`, `packages/theme/src/index.ts`

- [ ] **Step 1: Install the slider**

React Native has shipped no slider since 0.60. `expo install` picks the version
that matches SDK 58 rather than the newest on npm.

```bash
cd apps/mobile && yarn expo install @react-native-community/slider
```

Expected: the dependency appears in `apps/mobile/package.json` and yarn links it.

- [ ] **Step 2: Add the new style definitions to the theme**

Append to `packages/theme/src/index.ts`, after the `TextDisplay` section and
before `Screen-level styles`:

```ts
// ============================================================================
// Chip
// ============================================================================

export type ChipVariant = "default" | "wet" | "dry" | "faulty";

const CHIP_VARIANTS: Record<ChipVariant, { container: ViewStyle; label: TextStyle }> = {
  default: {
    container: { backgroundColor: colors.bgInput, borderColor: colors.border },
    label: { color: colors.textMain },
  },
  wet: {
    container: { backgroundColor: alpha.primary15, borderColor: colors.primary },
    label: { color: colors.primary },
  },
  dry: {
    container: { backgroundColor: "transparent", borderColor: colors.border },
    label: { color: colors.textMuted },
  },
  faulty: {
    container: { backgroundColor: alpha.danger15, borderColor: colors.danger },
    label: { color: colors.danger },
  },
};

export function chipContainerStyle(variant: ChipVariant, selected: boolean): ViewStyle {
  return {
    alignItems: "center",
    justifyContent: "center",
    minHeight: 34,
    paddingHorizontal: spacing.sm,
    borderRadius: radius.mdL,
    borderWidth: selected ? 2 : 1,
    ...CHIP_VARIANTS[variant].container,
    ...(selected
      ? { backgroundColor: alpha.primary15, borderColor: colors.primary }
      : {}),
  };
}

export function chipLabelStyle(variant: ChipVariant, selected: boolean): TextStyle {
  return {
    fontSize: typography.size.sm,
    fontWeight: selected ? typography.weight.bold : typography.weight.medium,
    ...CHIP_VARIANTS[variant].label,
    ...(selected ? { color: colors.primary } : {}),
  };
}

// ============================================================================
// StatusDot
// ============================================================================

export type StatusTone = "ok" | "degraded" | "off";

const STATUS_COLOR: Record<StatusTone, string> = {
  ok: colors.success,
  degraded: colors.warning,
  off: colors.textMuted,
};

export function statusDotStyle(tone: StatusTone): ViewStyle {
  return {
    width: 8,
    height: 8,
    borderRadius: 4,
    backgroundColor: STATUS_COLOR[tone],
  };
}

export function statusLabelStyle(tone: StatusTone): TextStyle {
  return { color: STATUS_COLOR[tone], fontSize: typography.size.sm };
}

// ============================================================================
// Slider
// ============================================================================

export const sliderStyles = {
  track: { width: "100%", height: 40 } satisfies ViewStyle,
  minimumTrackTintColor: colors.primary,
  maximumTrackTintColor: colors.border,
  thumbTintColor: colors.primary,
  label: {
    flexDirection: "row",
    justifyContent: "space-between",
  } satisfies ViewStyle,
} as const;

// ============================================================================
// Cards and hero
// ============================================================================

export const cardStyles = {
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    marginBottom: spacing.xs,
  } satisfies ViewStyle,
  title: {
    color: colors.textMain,
    fontSize: typography.size.md,
    fontWeight: typography.weight.bold,
  } satisfies TextStyle,
  status: {
    flexDirection: "row",
    alignItems: "center",
    gap: spacing.xxs * 2,
  } satisfies ViewStyle,
  heroValue: {
    color: colors.textMain,
    fontSize: 44,
    fontWeight: typography.weight.bold,
  } satisfies TextStyle,
  heroUnit: {
    color: colors.textMuted,
    fontSize: typography.size.lg,
  } satisfies TextStyle,
  heroRow: {
    flexDirection: "row",
    alignItems: "baseline",
    gap: spacing.xs,
  } satisfies ViewStyle,
  chipRow: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: spacing.xs,
  } satisfies ViewStyle,
  sectionLabel: {
    color: colors.textMuted,
    fontSize: typography.size.sm,
    fontWeight: typography.weight.medium,
  } satisfies TextStyle,
} as const;
```

- [ ] **Step 3: Checkpoint**

```bash
cd packages/theme && yarn typecheck && yarn lint
grep -c "chipContainerStyle\|statusDotStyle\|sliderStyles\|cardStyles" src/index.ts
```

Expected: typecheck and lint clean, count ≥ 4.

---

## Task 2: Formatters

**Files:**
- Modify: `apps/mobile/src/components/utility/format.ts`
- Create: `apps/mobile/src/components/utility/format.test.ts`

- [ ] **Step 1: Write the failing test**

`apps/mobile/src/components/utility/format.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  EM_DASH,
  formatCelsius,
  formatDuration,
  formatHeap,
  formatLitresPerHour,
  formatPercent,
  formatPercentPair,
  formatPh,
  formatRssi,
} from "./format";

describe("formatCelsius", () => {
  it("renders two decimals with a unit", () => {
    expect(formatCelsius(25.5)).toBe("25.50 °C");
  });

  it("renders null as an em dash, never as zero", () => {
    expect(formatCelsius(null)).toBe(EM_DASH);
  });
});

describe("formatPh", () => {
  it("renders two decimals", () => {
    expect(formatPh(6.8)).toBe("6.80");
  });

  it("renders null as an em dash", () => {
    expect(formatPh(null)).toBe(EM_DASH);
  });
});

describe("formatLitresPerHour", () => {
  it("renders a rounded rate with a unit", () => {
    expect(formatLitresPerHour(640.4)).toBe("640 L/h");
  });

  it("renders null as an em dash", () => {
    expect(formatLitresPerHour(null)).toBe(EM_DASH);
  });
});

describe("formatPercent", () => {
  it("appends a percent sign", () => {
    expect(formatPercent(60)).toBe("60%");
  });

  it("renders null as an em dash", () => {
    expect(formatPercent(null)).toBe(EM_DASH);
  });
});

describe("formatPercentPair", () => {
  it("joins two channels", () => {
    expect(formatPercentPair(60, 40)).toBe("60% / 40%");
  });
});

describe("formatDuration", () => {
  it("renders seconds under a minute", () => {
    expect(formatDuration(42)).toBe("42s");
  });

  it("renders minutes and seconds", () => {
    expect(formatDuration(125)).toBe("2m 05s");
  });

  it("renders hours and minutes", () => {
    expect(formatDuration(3700)).toBe("1h 01m");
  });

  it("renders zero", () => {
    expect(formatDuration(0)).toBe("0s");
  });
});

describe("formatHeap", () => {
  it("renders kilobytes below a megabyte", () => {
    expect(formatHeap(512)).toBe("512 kB");
  });

  it("switches to megabytes above a thousand", () => {
    expect(formatHeap(8302)).toBe("8.3 MB");
  });
});

describe("formatRssi", () => {
  it("renders dBm", () => {
    expect(formatRssi(-54)).toBe("-54 dBm");
  });
});
```

`formatHeap` earns its own case because the live device reports `free_heap_kb:
8302` with PSRAM active, and "8302 kB" is harder to read than "8.3 MB".

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd apps/mobile && yarn vitest run src/components/utility/format.test.ts 2>&1 | tail -6
```

Expected: FAIL — `EM_DASH` and the new formatters are not exported.

- [ ] **Step 3: Rewrite `apps/mobile/src/components/utility/format.ts`**

```ts
export const EM_DASH = "—";

export function formatCelsius(value: number | null): string {
  return value === null ? EM_DASH : `${value.toFixed(2)} °C`;
}

export function formatPh(value: number | null): string {
  return value === null ? EM_DASH : value.toFixed(2);
}

export function formatLitresPerHour(value: number | null): string {
  return value === null ? EM_DASH : `${Math.round(value)} L/h`;
}

export function formatPercent(value: number | null): string {
  return value === null ? EM_DASH : `${value}%`;
}

export function formatPercentPair(white: number, rgb: number): string {
  return `${white}% / ${rgb}%`;
}

export function formatDuration(totalSeconds: number): string {
  if (totalSeconds < 60) return `${totalSeconds}s`;

  if (totalSeconds < 3600) {
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
  }

  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  return `${hours}h ${String(minutes).padStart(2, "0")}m`;
}

export function formatHeap(kilobytes: number): string {
  return kilobytes < 1000
    ? `${kilobytes} kB`
    : `${(kilobytes / 1024).toFixed(1)} MB`;
}

export function formatRssi(dbm: number): string {
  return `${dbm} dBm`;
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd apps/mobile && yarn vitest run src/components/utility/format.test.ts 2>&1 | tail -6
```

Expected: PASS, 16 tests.

- [ ] **Step 5: Checkpoint**

```bash
cd apps/mobile && yarn vitest run 2>&1 | grep -E "Test Files|Tests "
```

Expected: 2 files, 24 tests (16 new + the 8 device-form tests).

---

## Task 3: Card selectors

**Files:**
- Create: `apps/mobile/src/components/utility/cards.ts`, `apps/mobile/src/components/utility/cards.test.ts`
- Delete: `apps/mobile/src/components/utility/telemetry-metrics.ts`

- [ ] **Step 1: Write the failing test**

`apps/mobile/src/components/utility/cards.test.ts`. The fixture is a full
`TankTelemetryDto`, so the selectors are exercised against the real contract
shape rather than a partial object.

```ts
import type { TankTelemetryDto } from "@aquarium/api-client";
import { describe, expect, it } from "vitest";
import {
  atoCard,
  co2Card,
  filtrationCard,
  lightingCard,
  systemCard,
  temperatureCard,
} from "./cards";

const snapshot: TankTelemetryDto = {
  timestamp_epoch_s: 1,
  system_mode: "normal",
  system_health: { uptime_seconds: 3700, wifi_rssi_dbm: -54, free_heap_kb: 8302 },
  temperature: {
    tank_celsius: 25.5,
    sump_celsius: 25.1,
    target_celsius: 27,
    heater_active: true,
    sensors_healthy: true,
  },
  filtration: { flow_rate_lph: 640, flow_ok: true, filter_relay_active: true },
  co2: {
    solenoid_open: true,
    current_ph: 6.8,
    drop_checker_ph_target: 6.8,
    ph_electrode_health: 0.93,
  },
  ato: {
    sump_water_low: false,
    reservoir_empty: false,
    pump_active: false,
    last_fill_duration_ms: 4200,
  },
  lighting: {
    profile: "daylight",
    channel_white_pct: 60,
    channel_rgb_pct: 40,
    override_active: false,
  },
  water_level: {
    sump_mechanical: { wet: true, faulty: false },
    sump_biological: { wet: true, faulty: false },
    sump_pump: { wet: true, faulty: false },
    main_tank: { wet: true, faulty: false },
    sump_reserve: { wet: false, faulty: false },
    sensors_healthy: true,
  },
  leak_detected: false,
};

describe("temperatureCard", () => {
  it("puts the tank reading in the hero slot", () => {
    const card = temperatureCard(snapshot);

    expect(card.heroValue).toBe("25.50");
    expect(card.heroUnit).toBe("°C");
    expect(card.status).toBe("ok");
  });

  it("reads as off when the sensors are disabled, not as zero", () => {
    const card = temperatureCard({
      ...snapshot,
      temperature: {
        ...snapshot.temperature,
        tank_celsius: null,
        sump_celsius: null,
        sensors_healthy: false,
      },
    });

    expect(card.heroValue).toBe("—");
    expect(card.status).toBe("off");
    expect(card.statusLabel).toBe("sensors off");
  });

  it("lists target, heater and sump as metrics", () => {
    const labels = temperatureCard(snapshot).metrics.map((m) => m.label);

    expect(labels).toEqual(["target", "heater", "sump"]);
  });
});

describe("filtrationCard", () => {
  it("is ok when flow is ok", () => {
    expect(filtrationCard(snapshot).status).toBe("ok");
  });

  it("is degraded when flow is not ok but the pump relay is on", () => {
    const card = filtrationCard({
      ...snapshot,
      filtration: { flow_rate_lph: 0, flow_ok: false, filter_relay_active: true },
    });

    expect(card.status).toBe("degraded");
    expect(card.statusLabel).toBe("no flow");
  });

  it("is off when there is no reading at all", () => {
    const card = filtrationCard({
      ...snapshot,
      filtration: { flow_rate_lph: null, flow_ok: false, filter_relay_active: false },
    });

    expect(card.status).toBe("off");
  });
});

describe("co2Card", () => {
  it("shows pH against the drop checker target", () => {
    const card = co2Card(snapshot);
    const ph = card.metrics.find((m) => m.label === "pH");

    expect(ph?.value).toBe("6.80");
    expect(card.metrics.find((m) => m.label === "target")?.value).toBe("6.80");
  });

  it("is off with no pH reading", () => {
    const card = co2Card({
      ...snapshot,
      co2: { ...snapshot.co2, current_ph: null, ph_electrode_health: null },
    });

    expect(card.status).toBe("off");
  });
});

describe("atoCard", () => {
  it("is degraded when the reservoir is empty", () => {
    const card = atoCard({
      ...snapshot,
      ato: { ...snapshot.ato, reservoir_empty: true },
    });

    expect(card.status).toBe("degraded");
    expect(card.statusLabel).toBe("reservoir empty");
  });

  it("formats the last fill from milliseconds", () => {
    const value = atoCard(snapshot).metrics.find((m) => m.label === "last fill")?.value;

    expect(value).toBe("4s");
  });
});

describe("lightingCard", () => {
  it("reports the profile and both channels", () => {
    const card = lightingCard(snapshot);

    expect(card.statusLabel).toBe("daylight");
    expect(card.metrics.find((m) => m.label === "white / rgb")?.value).toBe("60% / 40%");
  });

  it("is degraded while an override is active, so it reads as not on schedule", () => {
    const card = lightingCard({
      ...snapshot,
      lighting: { ...snapshot.lighting, override_active: true },
    });

    expect(card.status).toBe("degraded");
    expect(card.statusLabel).toBe("override");
  });
});

describe("systemCard", () => {
  it("formats heap in megabytes once PSRAM is live", () => {
    const value = systemCard(snapshot).metrics.find((m) => m.label === "free heap")?.value;

    expect(value).toBe("8.3 MB");
  });

  it("is degraded outside normal mode", () => {
    expect(systemCard({ ...snapshot, system_mode: "maintenance" }).status).toBe(
      "degraded",
    );
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd apps/mobile && yarn vitest run src/components/utility/cards.test.ts 2>&1 | tail -6
```

Expected: FAIL — `Cannot find module './cards'`.

- [ ] **Step 3: Write `apps/mobile/src/components/utility/cards.ts`**

```ts
import type { TankTelemetryDto } from "@aquarium/api-client";
import type { StatusTone } from "@aquarium/theme";
import type { Metric } from "@/components/molecules";
import {
  EM_DASH,
  formatCelsius,
  formatDuration,
  formatHeap,
  formatLitresPerHour,
  formatPercent,
  formatPercentPair,
  formatPh,
  formatRssi,
} from "./format";

export type CardData = {
  title: string;
  status: StatusTone;
  statusLabel: string;
  metrics: Metric[];
};

export type HeroCardData = CardData & {
  heroValue: string;
  heroUnit: string;
};

const onOff = (on: boolean) => (on ? "on" : "off");

export function temperatureCard(s: TankTelemetryDto): HeroCardData {
  const { tank_celsius, sump_celsius, target_celsius, heater_active, sensors_healthy } =
    s.temperature;

  const status: StatusTone = !sensors_healthy ? "off" : "ok";

  return {
    title: "temperature",
    status,
    statusLabel: sensors_healthy ? "healthy" : "sensors off",
    heroValue: tank_celsius === null ? EM_DASH : tank_celsius.toFixed(2),
    heroUnit: "°C",
    metrics: [
      { label: "target", value: formatCelsius(target_celsius) },
      { label: "heater", value: onOff(heater_active) },
      { label: "sump", value: formatCelsius(sump_celsius) },
    ],
  };
}

export function filtrationCard(s: TankTelemetryDto): CardData {
  const { flow_rate_lph, flow_ok, filter_relay_active } = s.filtration;

  const status: StatusTone =
    flow_rate_lph === null ? "off" : flow_ok ? "ok" : "degraded";

  return {
    title: "filtration",
    status,
    statusLabel: flow_rate_lph === null ? "off" : flow_ok ? "flowing" : "no flow",
    metrics: [
      { label: "flow", value: formatLitresPerHour(flow_rate_lph) },
      { label: "relay", value: onOff(filter_relay_active) },
    ],
  };
}

export function co2Card(s: TankTelemetryDto): CardData {
  const { solenoid_open, current_ph, drop_checker_ph_target, ph_electrode_health } = s.co2;

  const status: StatusTone = current_ph === null ? "off" : "ok";

  return {
    title: "co2",
    status,
    statusLabel: current_ph === null ? "off" : solenoid_open ? "dosing" : "idle",
    metrics: [
      { label: "pH", value: formatPh(current_ph) },
      { label: "target", value: formatPh(drop_checker_ph_target) },
      { label: "solenoid", value: onOff(solenoid_open) },
      {
        label: "electrode",
        value:
          ph_electrode_health === null
            ? EM_DASH
            : formatPercent(Math.round(ph_electrode_health * 100)),
      },
    ],
  };
}

export function atoCard(s: TankTelemetryDto): CardData {
  const { sump_water_low, reservoir_empty, pump_active, last_fill_duration_ms } = s.ato;

  const status: StatusTone = reservoir_empty || sump_water_low ? "degraded" : "ok";
  const statusLabel = reservoir_empty
    ? "reservoir empty"
    : sump_water_low
      ? "sump low"
      : pump_active
        ? "filling"
        : "idle";

  return {
    title: "ato",
    status,
    statusLabel,
    metrics: [
      { label: "pump", value: onOff(pump_active) },
      { label: "sump low", value: sump_water_low ? "yes" : "no" },
      { label: "reservoir", value: reservoir_empty ? "empty" : "ok" },
      {
        label: "last fill",
        value: formatDuration(Math.round(last_fill_duration_ms / 1000)),
      },
    ],
  };
}

export function lightingCard(s: TankTelemetryDto): CardData {
  const { profile, channel_white_pct, channel_rgb_pct, override_active } = s.lighting;

  return {
    title: "lighting",
    status: override_active ? "degraded" : "ok",
    statusLabel: override_active ? "override" : profile,
    metrics: [
      { label: "profile", value: profile },
      {
        label: "white / rgb",
        value: formatPercentPair(channel_white_pct, channel_rgb_pct),
      },
    ],
  };
}

export function systemCard(s: TankTelemetryDto): CardData {
  const { uptime_seconds, wifi_rssi_dbm, free_heap_kb } = s.system_health;

  return {
    title: "system",
    status: s.system_mode === "normal" ? "ok" : "degraded",
    statusLabel: s.system_mode,
    metrics: [
      { label: "wifi", value: formatRssi(wifi_rssi_dbm) },
      { label: "free heap", value: formatHeap(free_heap_kb) },
      { label: "uptime", value: formatDuration(uptime_seconds) },
    ],
  };
}
```

`lightingCard` deliberately reports an active override as `degraded`: nothing is
wrong, but the tank is not following its schedule, and that is worth a coloured
dot rather than a silent green one.

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd apps/mobile && yarn vitest run src/components/utility/cards.test.ts 2>&1 | tail -6
```

Expected: PASS, 14 tests.

- [ ] **Step 5: Delete the superseded module**

`telemetry-metrics.ts` produced one flat `Metric[]` for the whole snapshot. The
card selectors replace it entirely.

```bash
rm apps/mobile/src/components/utility/telemetry-metrics.ts
grep -rn "telemetry-metrics" apps/mobile/src || echo "no references remain"
```

Expected: the `no references remain` line. If anything still imports it, that
file is rewritten in Task 8.

- [ ] **Step 6: Checkpoint**

```bash
cd apps/mobile && yarn vitest run 2>&1 | grep -E "Test Files|Tests "
```

Expected: 3 files, 38 tests.

---

## Task 4: Water level mapping

**Files:**
- Create: `apps/mobile/src/components/utility/water-level.ts`, `apps/mobile/src/components/utility/water-level.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
import type { WaterLevelMetricsDto } from "@aquarium/api-client";
import { describe, expect, it } from "vitest";
import { waterLevelCard } from "./water-level";

const healthy: WaterLevelMetricsDto = {
  sump_mechanical: { wet: true, faulty: false },
  sump_biological: { wet: true, faulty: false },
  sump_pump: { wet: true, faulty: false },
  main_tank: { wet: true, faulty: false },
  sump_reserve: { wet: false, faulty: false },
  sensors_healthy: true,
};

describe("waterLevelCard", () => {
  it("returns the five sensors in a stable order", () => {
    expect(waterLevelCard(healthy).chips.map((c) => c.label)).toEqual([
      "main tank",
      "mechanical",
      "biological",
      "pump",
      "reserve",
    ]);
  });

  it("maps wet and dry to distinct chip variants", () => {
    const chips = waterLevelCard(healthy).chips;

    expect(chips.find((c) => c.label === "main tank")?.variant).toBe("wet");
    expect(chips.find((c) => c.label === "reserve")?.variant).toBe("dry");
  });

  it("marks a faulty sensor regardless of its wet state", () => {
    const card = waterLevelCard({
      ...healthy,
      sump_pump: { wet: true, faulty: true },
    });

    expect(card.chips.find((c) => c.label === "pump")?.variant).toBe("faulty");
    expect(card.status).toBe("degraded");
    expect(card.statusLabel).toBe("1 sensor faulty");
  });

  it("pluralises the faulty count", () => {
    const card = waterLevelCard({
      ...healthy,
      sump_pump: { wet: true, faulty: true },
      main_tank: { wet: true, faulty: true },
    });

    expect(card.statusLabel).toBe("2 sensors faulty");
  });

  it("is off when the firmware reports the sensors unhealthy with none faulty", () => {
    const card = waterLevelCard({ ...healthy, sensors_healthy: false });

    expect(card.status).toBe("off");
    expect(card.statusLabel).toBe("sensors off");
  });
});
```

The faulty case asserts that `faulty` wins over `wet`: a sensor that reports wet
while flagged faulty is reporting nothing trustworthy, and showing it as a normal
wet chip would hide the fault.

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd apps/mobile && yarn vitest run src/components/utility/water-level.test.ts 2>&1 | tail -6
```

Expected: FAIL — `Cannot find module './water-level'`.

- [ ] **Step 3: Write `apps/mobile/src/components/utility/water-level.ts`**

```ts
import type { WaterLevelEntryDto, WaterLevelMetricsDto } from "@aquarium/api-client";
import type { ChipVariant, StatusTone } from "@aquarium/theme";

export type WaterLevelChip = {
  label: string;
  variant: ChipVariant;
};

export type WaterLevelCardData = {
  title: string;
  status: StatusTone;
  statusLabel: string;
  chips: WaterLevelChip[];
};

const SENSORS: [string, keyof WaterLevelMetricsDto][] = [
  ["main tank", "main_tank"],
  ["mechanical", "sump_mechanical"],
  ["biological", "sump_biological"],
  ["pump", "sump_pump"],
  ["reserve", "sump_reserve"],
];

function variantOf(entry: WaterLevelEntryDto): ChipVariant {
  if (entry.faulty) return "faulty";
  return entry.wet ? "wet" : "dry";
}

export function waterLevelCard(levels: WaterLevelMetricsDto): WaterLevelCardData {
  const chips = SENSORS.map(([label, key]) => ({
    label,
    variant: variantOf(levels[key] as WaterLevelEntryDto),
  }));

  const faulty = chips.filter((chip) => chip.variant === "faulty").length;

  if (faulty > 0) {
    return {
      title: "water level",
      status: "degraded",
      statusLabel: `${faulty} ${faulty === 1 ? "sensor" : "sensors"} faulty`,
      chips,
    };
  }

  return {
    title: "water level",
    status: levels.sensors_healthy ? "ok" : "off",
    statusLabel: levels.sensors_healthy ? "healthy" : "sensors off",
    chips,
  };
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd apps/mobile && yarn vitest run src/components/utility/water-level.test.ts 2>&1 | tail -6
```

Expected: PASS, 5 tests.

- [ ] **Step 5: Checkpoint**

```bash
cd apps/mobile && yarn vitest run 2>&1 | grep -E "Test Files|Tests "
```

Expected: 4 files, 43 tests.

---

## Task 5: Countdown hook and lighting request mapping

**Files:**
- Create: `apps/mobile/src/components/utility/use-countdown.ts`, `apps/mobile/src/components/utility/lighting-request.ts`, `apps/mobile/src/components/utility/lighting-request.test.ts`

- [ ] **Step 1: Write the failing test for the request mapping**

`apps/mobile/src/components/utility/lighting-request.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  DURATION_CHOICES,
  channelRequest,
  profileRequest,
} from "./lighting-request";

describe("profileRequest", () => {
  it("wraps a profile in the externally tagged target", () => {
    expect(profileRequest("moonlight", 300)).toEqual({
      target: { profile: "moonlight" },
      duration_s: 300,
    });
  });

  it("sends null for no limit, never zero", () => {
    expect(profileRequest("daylight", null)).toEqual({
      target: { profile: "daylight" },
      duration_s: null,
    });
  });
});

describe("channelRequest", () => {
  it("wraps channel levels in the externally tagged target", () => {
    expect(channelRequest({ white_pct: 20, rgb_pct: 10 }, 3600)).toEqual({
      target: { channels: { white_pct: 20, rgb_pct: 10 } },
      duration_s: 3600,
    });
  });

  it("clamps levels into the range the domain accepts", () => {
    expect(channelRequest({ white_pct: 140, rgb_pct: -5 }, null).target).toEqual({
      channels: { white_pct: 100, rgb_pct: 0 },
    });
  });

  it("rounds fractional slider values to integers", () => {
    expect(channelRequest({ white_pct: 59.6, rgb_pct: 40.2 }, null).target).toEqual({
      channels: { white_pct: 60, rgb_pct: 40 },
    });
  });
});

describe("DURATION_CHOICES", () => {
  it("offers no limit as null rather than zero", () => {
    const noLimit = DURATION_CHOICES.find((c) => c.label === "no limit");

    expect(noLimit?.seconds).toBeNull();
  });

  it("never offers a zero duration, which the firmware rejects", () => {
    expect(DURATION_CHOICES.every((c) => c.seconds === null || c.seconds > 0)).toBe(true);
  });
});
```

The clamping and rounding tests matter because the slider is a float input and
`ChannelLevels::validate` rejects anything above 100 — the mapping is where a
float becomes a value the firmware will accept.

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd apps/mobile && yarn vitest run src/components/utility/lighting-request.test.ts 2>&1 | tail -6
```

Expected: FAIL — `Cannot find module './lighting-request'`.

- [ ] **Step 3: Write `apps/mobile/src/components/utility/lighting-request.ts`**

```ts
import type {
  ChannelLevels,
  LightingProfile,
  SetOverrideRequest,
} from "@aquarium/api-client";

export const LIGHTING_PROFILES: LightingProfile[] = [
  "off",
  "sunrise",
  "daylight",
  "sunset",
  "moonlight",
];

export type DurationChoice = {
  label: string;
  seconds: number | null;
};

// `null` means "until something else changes it". It must never be 0 —
// SetOverrideRequest::validate rejects Some(0) with "duration must be positive".
export const DURATION_CHOICES: DurationChoice[] = [
  { label: "5 min", seconds: 300 },
  { label: "15 min", seconds: 900 },
  { label: "1 h", seconds: 3600 },
  { label: "no limit", seconds: null },
];

export function profileRequest(
  profile: LightingProfile,
  duration: number | null,
): SetOverrideRequest {
  return { target: { profile }, duration_s: duration };
}

function clampPct(value: number): number {
  return Math.min(100, Math.max(0, Math.round(value)));
}

export function channelRequest(
  levels: ChannelLevels,
  duration: number | null,
): SetOverrideRequest {
  return {
    target: {
      channels: {
        white_pct: clampPct(levels.white_pct),
        rgb_pct: clampPct(levels.rgb_pct),
      },
    },
    duration_s: duration,
  };
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd apps/mobile && yarn vitest run src/components/utility/lighting-request.test.ts 2>&1 | tail -6
```

Expected: PASS, 7 tests.

- [ ] **Step 5: Write the failing test for the countdown arithmetic**

The spec calls for testing the countdown's ticking, its clamp at zero, and its
restart when a new `expires_in_s` arrives. A `setInterval` inside a hook cannot
be tested without a renderer, so the arithmetic is extracted into a pure
function and the hook becomes the wiring around it.

Append to `apps/mobile/src/components/utility/lighting-request.test.ts`:

```ts
describe("tickCountdown", () => {
  it("counts a second down", () => {
    expect(tickCountdown(42)).toEqual({ next: 41, elapsed: false });
  });

  it("clamps at zero and reports elapsed", () => {
    expect(tickCountdown(1)).toEqual({ next: 0, elapsed: true });
  });

  it("stays at zero without reporting elapsed twice", () => {
    expect(tickCountdown(0)).toEqual({ next: 0, elapsed: false });
  });

  it("passes null through, since no override means no countdown", () => {
    expect(tickCountdown(null)).toEqual({ next: null, elapsed: false });
  });
});
```

Add `tickCountdown` to that file's import list from `./lighting-request`.

The "stays at zero" case is what stops the hook refetching once a second forever
after an override expires.

- [ ] **Step 6: Add `tickCountdown` to `lighting-request.ts`**

```ts
export function tickCountdown(current: number | null): {
  next: number | null;
  elapsed: boolean;
} {
  if (current === null) return { next: null, elapsed: false };
  if (current <= 0) return { next: 0, elapsed: false };

  const next = current - 1;
  return { next, elapsed: next <= 0 };
}
```

- [ ] **Step 7: Run the test to verify it passes**

```bash
cd apps/mobile && yarn vitest run src/components/utility/lighting-request.test.ts 2>&1 | tail -6
```

Expected: PASS, 11 tests.

- [ ] **Step 8: Write the countdown hook over it**

`apps/mobile/src/components/utility/use-countdown.ts`:

```ts
import { useEffect, useRef, useState } from "react";
import { tickCountdown } from "./lighting-request";

export function useCountdown(
  seconds: number | null,
  onElapsed?: () => void,
): number | null {
  const [remaining, setRemaining] = useState<number | null>(seconds);
  const elapsedCallback = useRef(onElapsed);
  elapsedCallback.current = onElapsed;

  useEffect(() => {
    setRemaining(seconds);
  }, [seconds]);

  const idle = remaining === null || remaining <= 0;

  useEffect(() => {
    if (idle) return;

    const timer = setInterval(() => {
      setRemaining((current) => {
        const { next, elapsed } = tickCountdown(current);
        if (elapsed) elapsedCallback.current?.();
        return next;
      });
    }, 1000);

    return () => clearInterval(timer);
  }, [idle]);

  return remaining;
}
```

The effect depends on `idle`, not on `remaining`, so the interval is created once
per countdown rather than torn down and rebuilt every tick. `onElapsed` lives in
a ref so a caller passing an inline arrow does not restart the interval on every
render.

- [ ] **Step 9: Checkpoint**

```bash
cd apps/mobile && yarn vitest run 2>&1 | grep -E "Test Files|Tests "
cd apps/mobile && yarn typecheck
```

Expected: 5 files, 54 tests; typecheck exits 0.

---

## Task 6: Chip, StatusDot and Slider atoms

**Files:**
- Create: `apps/mobile/src/components/atoms/Chip/{types.ts,Chip.tsx}`, `apps/mobile/src/components/atoms/StatusDot/{types.ts,StatusDot.tsx}`, `apps/mobile/src/components/atoms/Slider/{types.ts,Slider.tsx}`
- Modify: `apps/mobile/src/components/atoms/index.ts`

These are presentational wrappers over theme functions written in Task 1; they
carry no logic to test.

- [ ] **Step 1: Write `Chip`**

`apps/mobile/src/components/atoms/Chip/types.ts`:

```ts
import type { StyleProp, ViewStyle } from "react-native";
import type { ChipVariant } from "@aquarium/theme";

export type { ChipVariant };

export interface ChipProps {
  label: string;
  variant?: ChipVariant;
  selected?: boolean;
  onPress?: () => void;
  style?: StyleProp<ViewStyle>;
}
```

`apps/mobile/src/components/atoms/Chip/Chip.tsx`:

```tsx
import { chipContainerStyle, chipLabelStyle } from "@aquarium/theme";
import { Pressable, Text, View } from "react-native";
import type { ChipProps } from "./types";

export type { ChipProps, ChipVariant } from "./types";

export function Chip({
  label,
  variant = "default",
  selected = false,
  onPress,
  style,
}: ChipProps) {
  const container = chipContainerStyle(variant, selected);
  const text = <Text style={chipLabelStyle(variant, selected)}>{label}</Text>;

  if (!onPress) {
    return <View style={[container, style]}>{text}</View>;
  }

  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ selected }}
      onPress={onPress}
      style={({ pressed }) => [container, pressed ? { opacity: 0.85 } : null, style]}
    >
      {text}
    </Pressable>
  );
}
```

A chip without `onPress` renders as a `View`, so the water-level chips are not
announced as buttons to a screen reader when they cannot be pressed.

- [ ] **Step 2: Write `StatusDot`**

`apps/mobile/src/components/atoms/StatusDot/types.ts`:

```ts
import type { StatusTone } from "@aquarium/theme";

export type { StatusTone };

export interface StatusDotProps {
  tone: StatusTone;
  label?: string;
}
```

`apps/mobile/src/components/atoms/StatusDot/StatusDot.tsx`:

```tsx
import { cardStyles, statusDotStyle, statusLabelStyle } from "@aquarium/theme";
import { Text, View } from "react-native";
import type { StatusDotProps } from "./types";

export type { StatusDotProps, StatusTone } from "./types";

export function StatusDot({ tone, label }: StatusDotProps) {
  return (
    <View style={cardStyles.status}>
      <View style={statusDotStyle(tone)} />
      {label ? <Text style={statusLabelStyle(tone)}>{label}</Text> : null}
    </View>
  );
}
```

- [ ] **Step 3: Write `Slider`**

`apps/mobile/src/components/atoms/Slider/types.ts`:

```ts
export interface SliderProps {
  label: string;
  value: number;
  onCommit: (value: number) => void;
  minimumValue?: number;
  maximumValue?: number;
  step?: number;
  disabled?: boolean;
}
```

`apps/mobile/src/components/atoms/Slider/Slider.tsx`:

```tsx
import CommunitySlider from "@react-native-community/slider";
import { cardStyles, sliderStyles } from "@aquarium/theme";
import { useEffect, useState } from "react";
import { Text, View } from "react-native";
import { formatPercent } from "@/components/utility/format";
import type { SliderProps } from "./types";

export type { SliderProps } from "./types";

export function Slider({
  label,
  value,
  onCommit,
  minimumValue = 0,
  maximumValue = 100,
  step = 1,
  disabled = false,
}: SliderProps) {
  const [dragging, setDragging] = useState(value);

  useEffect(() => {
    setDragging(value);
  }, [value]);

  return (
    <View>
      <View style={sliderStyles.label}>
        <Text style={cardStyles.sectionLabel}>{label}</Text>
        <Text style={cardStyles.sectionLabel}>{formatPercent(Math.round(dragging))}</Text>
      </View>
      <CommunitySlider
        style={sliderStyles.track}
        value={value}
        minimumValue={minimumValue}
        maximumValue={maximumValue}
        step={step}
        disabled={disabled}
        minimumTrackTintColor={sliderStyles.minimumTrackTintColor}
        maximumTrackTintColor={sliderStyles.maximumTrackTintColor}
        thumbTintColor={sliderStyles.thumbTintColor}
        onValueChange={setDragging}
        onSlidingComplete={onCommit}
      />
    </View>
  );
}
```

The local `dragging` state drives the readout while the thumb moves; only
`onSlidingComplete` calls `onCommit`, so dragging does not fire a `PUT` per pixel.

- [ ] **Step 4: Extend the atoms barrel**

Append to `apps/mobile/src/components/atoms/index.ts`:

```ts
export { Chip } from "./Chip/Chip";
export type { ChipProps, ChipVariant } from "./Chip/types";
export { Slider } from "./Slider/Slider";
export type { SliderProps } from "./Slider/types";
export { StatusDot } from "./StatusDot/StatusDot";
export type { StatusDotProps, StatusTone } from "./StatusDot/types";
```

- [ ] **Step 5: Checkpoint**

```bash
cd apps/mobile && yarn typecheck && yarn eslint .
```

Expected: both exit 0.

---

## Task 7: Molecules

**Files:**
- Create: `apps/mobile/src/components/molecules/MetricCard/{types.ts,MetricCard.tsx}`, `.../WaterLevelGrid/WaterLevelGrid.tsx`, `.../ProfilePicker/ProfilePicker.tsx`, `.../DurationPicker/DurationPicker.tsx`, `.../ChannelSliders/ChannelSliders.tsx`
- Modify: `apps/mobile/src/components/molecules/index.ts`

- [ ] **Step 1: Write `MetricCard`**

`apps/mobile/src/components/molecules/MetricCard/types.ts`:

```ts
import type { ReactNode } from "react";
import type { StatusTone } from "@aquarium/theme";
import type { Metric } from "@/components/molecules/MetricList/types";

export interface MetricCardProps {
  title: string;
  status: StatusTone;
  statusLabel: string;
  metrics?: Metric[];
  children?: ReactNode;
}
```

`apps/mobile/src/components/molecules/MetricCard/MetricCard.tsx`:

```tsx
import { cardStyles } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Card, StatusDot } from "@/components/atoms";
import { MetricList } from "@/components/molecules/MetricList/MetricList";
import type { MetricCardProps } from "./types";

export type { MetricCardProps } from "./types";

export function MetricCard({
  title,
  status,
  statusLabel,
  metrics,
  children,
}: MetricCardProps) {
  return (
    <Card variant="elevated" padding="md" width="full">
      <View style={cardStyles.header}>
        <Text style={cardStyles.title}>{title}</Text>
        <StatusDot tone={status} label={statusLabel} />
      </View>
      {metrics ? <MetricList metrics={metrics} /> : null}
      {children}
    </Card>
  );
}
```

It imports `MetricList` by path rather than through the molecules barrel, because
the barrel also exports `MetricCard` and importing a barrel from inside it is a
cycle.

- [ ] **Step 2: Write `WaterLevelGrid`**

`apps/mobile/src/components/molecules/WaterLevelGrid/WaterLevelGrid.tsx`:

```tsx
import { cardStyles } from "@aquarium/theme";
import { View } from "react-native";
import { Chip } from "@/components/atoms";
import type { WaterLevelChip } from "@/components/utility/water-level";

export interface WaterLevelGridProps {
  chips: WaterLevelChip[];
}

export function WaterLevelGrid({ chips }: WaterLevelGridProps) {
  return (
    <View style={cardStyles.chipRow}>
      {chips.map((chip) => (
        <Chip key={chip.label} label={chip.label} variant={chip.variant} />
      ))}
    </View>
  );
}
```

- [ ] **Step 3: Write `ProfilePicker`**

`apps/mobile/src/components/molecules/ProfilePicker/ProfilePicker.tsx`:

```tsx
import type { LightingProfile } from "@aquarium/api-client";
import { cardStyles, spacing } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Chip } from "@/components/atoms";
import { LIGHTING_PROFILES } from "@/components/utility/lighting-request";

export interface ProfilePickerProps {
  selected: LightingProfile | null;
  onSelect: (profile: LightingProfile) => void;
  disabled?: boolean;
}

export function ProfilePicker({ selected, onSelect, disabled }: ProfilePickerProps) {
  return (
    <View style={{ gap: spacing.xs }}>
      <Text style={cardStyles.sectionLabel}>profile</Text>
      <View style={cardStyles.chipRow}>
        {LIGHTING_PROFILES.map((profile) => (
          <Chip
            key={profile}
            label={profile}
            selected={profile === selected}
            {...(disabled ? {} : { onPress: () => onSelect(profile) })}
          />
        ))}
      </View>
    </View>
  );
}
```

- [ ] **Step 4: Write `DurationPicker`**

`apps/mobile/src/components/molecules/DurationPicker/DurationPicker.tsx`:

```tsx
import { cardStyles, spacing } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Chip } from "@/components/atoms";
import { DURATION_CHOICES } from "@/components/utility/lighting-request";

export interface DurationPickerProps {
  selected: number | null;
  onSelect: (seconds: number | null) => void;
}

export function DurationPicker({ selected, onSelect }: DurationPickerProps) {
  return (
    <View style={{ gap: spacing.xs }}>
      <Text style={cardStyles.sectionLabel}>duration</Text>
      <View style={cardStyles.chipRow}>
        {DURATION_CHOICES.map((choice) => (
          <Chip
            key={choice.label}
            label={choice.label}
            selected={choice.seconds === selected}
            onPress={() => onSelect(choice.seconds)}
          />
        ))}
      </View>
    </View>
  );
}
```

- [ ] **Step 5: Write `ChannelSliders`**

`apps/mobile/src/components/molecules/ChannelSliders/ChannelSliders.tsx`:

```tsx
import type { ChannelLevels } from "@aquarium/api-client";
import { cardStyles, spacing } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Slider } from "@/components/atoms";

export interface ChannelSlidersProps {
  levels: ChannelLevels;
  onCommit: (levels: ChannelLevels) => void;
  disabled?: boolean;
}

export function ChannelSliders({ levels, onCommit, disabled }: ChannelSlidersProps) {
  return (
    <View style={{ gap: spacing.xs }}>
      <Text style={cardStyles.sectionLabel}>manual channels</Text>
      <Slider
        label="white"
        value={levels.white_pct}
        disabled={disabled ?? false}
        onCommit={(white_pct) => onCommit({ ...levels, white_pct })}
      />
      <Slider
        label="rgb"
        value={levels.rgb_pct}
        disabled={disabled ?? false}
        onCommit={(rgb_pct) => onCommit({ ...levels, rgb_pct })}
      />
    </View>
  );
}
```

- [ ] **Step 6: Extend the molecules barrel**

Append to `apps/mobile/src/components/molecules/index.ts`:

```ts
export { ChannelSliders } from "./ChannelSliders/ChannelSliders";
export type { ChannelSlidersProps } from "./ChannelSliders/ChannelSliders";
export { DurationPicker } from "./DurationPicker/DurationPicker";
export type { DurationPickerProps } from "./DurationPicker/DurationPicker";
export { MetricCard } from "./MetricCard/MetricCard";
export type { MetricCardProps } from "./MetricCard/types";
export { ProfilePicker } from "./ProfilePicker/ProfilePicker";
export type { ProfilePickerProps } from "./ProfilePicker/ProfilePicker";
export { WaterLevelGrid } from "./WaterLevelGrid/WaterLevelGrid";
export type { WaterLevelGridProps } from "./WaterLevelGrid/WaterLevelGrid";
```

- [ ] **Step 7: Checkpoint**

```bash
cd apps/mobile && yarn typecheck && yarn eslint .
```

Expected: both exit 0.

---

## Task 8: Telemetry dashboard

**Files:**
- Create: `apps/mobile/src/components/organisms/TelemetryDashboard/TelemetryDashboard.tsx`, `apps/mobile/src/components/organisms/TelemetryDashboard/HeroCard.tsx`
- Modify: `apps/mobile/src/components/organisms/index.ts`, `apps/mobile/src/components/pages/TelemetryPage/TelemetryPage.tsx`
- Move: `apps/mobile/src/components/organisms/TelemetryPanel/stream-badge.ts` → `apps/mobile/src/components/utility/stream-badge.ts`
- Delete: `apps/mobile/src/components/organisms/TelemetryPanel/`

- [ ] **Step 1: Move the stream badge into utility**

The tab header needs it in Task 10, and an organism is the wrong place for a
pure mapping.

```bash
cd apps/mobile
git mv src/components/organisms/TelemetryPanel/stream-badge.ts src/components/utility/stream-badge.ts
rm -rf src/components/organisms/TelemetryPanel
```

- [ ] **Step 2: Write the hero card**

`apps/mobile/src/components/organisms/TelemetryDashboard/HeroCard.tsx`:

```tsx
import { cardStyles } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Card, StatusDot } from "@/components/atoms";
import { MetricList } from "@/components/molecules";
import type { HeroCardData } from "@/components/utility/cards";

export function HeroCard({ card }: { card: HeroCardData }) {
  return (
    <Card variant="levitating" padding="md" width="full">
      <View style={cardStyles.header}>
        <Text style={cardStyles.title}>{card.title}</Text>
        <StatusDot tone={card.status} label={card.statusLabel} />
      </View>

      <View style={cardStyles.heroRow}>
        <Text style={cardStyles.heroValue}>{card.heroValue}</Text>
        <Text style={cardStyles.heroUnit}>{card.heroUnit}</Text>
      </View>

      <MetricList metrics={card.metrics} />
    </Card>
  );
}
```

- [ ] **Step 3: Write the dashboard**

`apps/mobile/src/components/organisms/TelemetryDashboard/TelemetryDashboard.tsx`:

```tsx
import type { TankTelemetryDto } from "@aquarium/api-client";
import { spacing } from "@aquarium/theme";
import { View } from "react-native";
import { Alert } from "@/components/atoms";
import { MetricCard, WaterLevelGrid } from "@/components/molecules";
import {
  atoCard,
  co2Card,
  filtrationCard,
  lightingCard,
  systemCard,
  temperatureCard,
} from "@/components/utility/cards";
import { waterLevelCard } from "@/components/utility/water-level";
import { HeroCard } from "./HeroCard";

export function TelemetryDashboard({ snapshot }: { snapshot: TankTelemetryDto }) {
  const levels = waterLevelCard(snapshot.water_level);
  const cards = [
    filtrationCard(snapshot),
    co2Card(snapshot),
    atoCard(snapshot),
    lightingCard(snapshot),
    systemCard(snapshot),
  ];

  return (
    <View style={{ gap: spacing.sm }}>
      {snapshot.leak_detected ? (
        <Alert variant="danger" title="leak detected">
          water outside the tank — check the sump and the floor
        </Alert>
      ) : null}

      <HeroCard card={temperatureCard(snapshot)} />

      <MetricCard
        title={levels.title}
        status={levels.status}
        statusLabel={levels.statusLabel}
      >
        <WaterLevelGrid chips={levels.chips} />
      </MetricCard>

      {cards.map((card) => (
        <MetricCard
          key={card.title}
          title={card.title}
          status={card.status}
          statusLabel={card.statusLabel}
          metrics={card.metrics}
        />
      ))}
    </View>
  );
}
```

The leak alert is rendered first and unconditionally above the cards, so it is
never something a user has to scroll to reach.

- [ ] **Step 4: Rewrite the telemetry page**

`apps/mobile/src/components/pages/TelemetryPage/TelemetryPage.tsx`. The page owns
fetching, refresh and error presentation; the dashboard only renders a snapshot.

```tsx
import { ApiError, useTelemetry } from "@aquarium/api-client";
import { colors, screenStyles, spacing } from "@aquarium/theme";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { RefreshControl, ScrollView } from "react-native";
import { Alert } from "@/components/atoms";
import { ScreenMessage } from "@/components/molecules";
import { TelemetryDashboard } from "@/components/organisms";
import { useDevice } from "@/components/utility/app-providers";
import { useTelemetryStream } from "@/device/use-telemetry-stream";

export function TelemetryPage() {
  const { transport } = useDevice();
  const queryClient = useQueryClient();
  const [refreshing, setRefreshing] = useState(false);

  useTelemetryStream(transport);

  const query = useTelemetry(
    transport ?? { device: { host: "", port: 80, token: "" }, fetchImpl: fetch },
  );

  const onRefresh = useCallback(async () => {
    setRefreshing(true);
    await queryClient.invalidateQueries();
    setRefreshing(false);
  }, [queryClient]);

  if (!transport) return <ScreenMessage>no device configured</ScreenMessage>;
  if (query.isPending) return <ScreenMessage loading>fetching snapshot…</ScreenMessage>;

  return (
    <ScrollView
      style={screenStyles.page}
      contentContainerStyle={{ padding: spacing.sm, gap: spacing.sm }}
      refreshControl={
        <RefreshControl
          refreshing={refreshing}
          onRefresh={() => void onRefresh()}
          tintColor={colors.primary}
        />
      }
    >
      {query.error ? <TelemetryError error={query.error} /> : null}
      {query.data ? <TelemetryDashboard snapshot={query.data} /> : null}
    </ScrollView>
  );
}

function TelemetryError({ error }: { error: Error }) {
  if (!(error instanceof ApiError)) {
    return (
      <Alert variant="danger" title="telemetry failed">
        {error.message}
      </Alert>
    );
  }

  if (error.kind === "unauthorized") {
    return (
      <Alert variant="danger" title="token rejected">
        the device refused the api token — open the Device tab and re-enter it
      </Alert>
    );
  }

  if (error.kind === "transport") {
    return (
      <Alert variant="warning" title="device unreachable">
        no answer from the controller — it may be off, or on another network
      </Alert>
    );
  }

  if (error.kind === "malformed") {
    return (
      <Alert variant="danger" title="unexpected telemetry">
        {error.detail ?? error.message}
      </Alert>
    );
  }

  return (
    <Alert variant="danger" title="device returned an error">
      {error.detail ?? error.message}
    </Alert>
  );
}
```

`useTelemetry` is called with a placeholder transport when none is configured
because hooks cannot be called conditionally; the `!transport` branch returns
before anything renders from it, and Task 10's redirect means this state is only
reachable for a frame.

- [ ] **Step 5: Update the organisms barrel**

Replace the `TelemetryPanel` entries in
`apps/mobile/src/components/organisms/index.ts`:

```ts
export { DeviceForm } from "./DeviceForm/DeviceForm";
export type { DeviceFormProps } from "./DeviceForm/DeviceForm";
export { TelemetryDashboard } from "./TelemetryDashboard/TelemetryDashboard";
```

- [ ] **Step 6: Checkpoint**

```bash
cd apps/mobile && yarn typecheck && yarn eslint . && yarn vitest run 2>&1 | grep -E "Tests "
grep -rn "TelemetryPanel" src || echo "no references to the deleted organism"
```

Expected: all clean, 54 tests, and the `no references` line.

---

## Task 9: Lighting screen

**Files:**
- Create: `apps/mobile/src/components/organisms/LightingControl/LightingControl.tsx`, `apps/mobile/src/components/pages/LightingPage/LightingPage.tsx`
- Modify: `apps/mobile/src/components/organisms/index.ts`, `apps/mobile/src/components/pages/index.ts`

- [ ] **Step 1: Write the control organism**

`apps/mobile/src/components/organisms/LightingControl/LightingControl.tsx`:

```tsx
import {
  useClearLightingOverride,
  useLightingOverride,
  useSetLightingOverride,
  type ChannelLevels,
  type LightingMetricsDto,
  type LightingProfile,
  type Transport,
} from "@aquarium/api-client";
import { cardStyles, spacing } from "@aquarium/theme";
import { useState } from "react";
import { Text, View } from "react-native";
import { Alert, Button } from "@/components/atoms";
import {
  ChannelSliders,
  DurationPicker,
  MetricCard,
  ProfilePicker,
} from "@/components/molecules";
import { formatDuration, formatPercentPair } from "@/components/utility/format";
import { channelRequest, profileRequest } from "@/components/utility/lighting-request";
import { useCountdown } from "@/components/utility/use-countdown";

export function LightingControl({
  transport,
  lighting,
}: {
  transport: Transport;
  lighting: LightingMetricsDto;
}) {
  const override = useLightingOverride(transport);
  const setOverride = useSetLightingOverride(transport);
  const clearOverride = useClearLightingOverride(transport);

  const [duration, setDuration] = useState<number | null>(900);
  const [levels, setLevels] = useState<ChannelLevels>({
    white_pct: lighting.channel_white_pct,
    rgb_pct: lighting.channel_rgb_pct,
  });

  const remaining = useCountdown(override.data?.expires_in_s ?? null, () => {
    void override.refetch();
  });

  const busy = setOverride.isPending || clearOverride.isPending;
  const active = lighting.override_active;

  const selectedProfile: LightingProfile | null =
    override.data && "profile" in override.data.target
      ? override.data.target.profile
      : null;

  const failure = setOverride.error ?? clearOverride.error;

  return (
    <View style={{ gap: spacing.sm }}>
      <MetricCard
        title="now"
        status={active ? "degraded" : "ok"}
        statusLabel={active ? "override" : "on schedule"}
        metrics={[
          { label: "profile", value: lighting.profile },
          {
            label: "white / rgb",
            value: formatPercentPair(
              lighting.channel_white_pct,
              lighting.channel_rgb_pct,
            ),
          },
          ...(remaining === null
            ? []
            : [{ label: "expires in", value: formatDuration(remaining) }]),
        ]}
      />

      {failure ? (
        <Alert variant="danger" title="the device refused that">
          {failure.message}
        </Alert>
      ) : null}

      <MetricCard title="override" status="ok" statusLabel={busy ? "sending…" : "ready"}>
        <View style={{ gap: spacing.sm }}>
          <DurationPicker selected={duration} onSelect={setDuration} />

          <ProfilePicker
            selected={selectedProfile}
            disabled={busy}
            onSelect={(profile) => setOverride.mutate(profileRequest(profile, duration))}
          />

          <ChannelSliders
            levels={levels}
            disabled={busy}
            onCommit={(next) => {
              setLevels(next);
              setOverride.mutate(channelRequest(next, duration));
            }}
          />

          <Text style={cardStyles.sectionLabel}>
            {active
              ? "an override is active — the schedule is paused"
              : "the schedule is running"}
          </Text>

          <Button
            variant="outline"
            fullWidth
            isLoading={clearOverride.isPending}
            disabled={!active}
            onPress={() => clearOverride.mutate()}
          >
            restore schedule
          </Button>
        </View>
      </MetricCard>
    </View>
  );
}
```

The duration chips come before the profile and channel controls because
selecting a duration changes what the next tap sends; putting them after would
mean choosing a lifetime for an override that has already been applied.

- [ ] **Step 2: Write the page**

`apps/mobile/src/components/pages/LightingPage/LightingPage.tsx`:

```tsx
import { useTelemetry } from "@aquarium/api-client";
import { screenStyles, spacing } from "@aquarium/theme";
import { ScrollView } from "react-native";
import { ScreenMessage } from "@/components/molecules";
import { LightingControl } from "@/components/organisms";
import { useDevice } from "@/components/utility/app-providers";

export function LightingPage() {
  const { transport } = useDevice();

  const query = useTelemetry(
    transport ?? { device: { host: "", port: 80, token: "" }, fetchImpl: fetch },
  );

  if (!transport) return <ScreenMessage>no device configured</ScreenMessage>;
  if (query.isPending) return <ScreenMessage loading>reading lighting…</ScreenMessage>;
  if (!query.data) return <ScreenMessage>no data</ScreenMessage>;

  return (
    <ScrollView
      style={screenStyles.page}
      contentContainerStyle={{ padding: spacing.sm, gap: spacing.sm }}
    >
      <LightingControl transport={transport} lighting={query.data.lighting} />
    </ScrollView>
  );
}
```

- [ ] **Step 3: Update both barrels**

Append to `apps/mobile/src/components/organisms/index.ts`:

```ts
export { LightingControl } from "./LightingControl/LightingControl";
```

Replace `apps/mobile/src/components/pages/index.ts`:

```ts
export { DeviceSettingsPage } from "./DeviceSettingsPage/DeviceSettingsPage";
export { LightingPage } from "./LightingPage/LightingPage";
export { TelemetryPage } from "./TelemetryPage/TelemetryPage";
```

- [ ] **Step 4: Checkpoint**

```bash
cd apps/mobile && yarn typecheck && yarn eslint .
```

Expected: both exit 0.

---

## Task 10: Bottom tabs

**Files:**
- Create: `apps/mobile/src/app/(tabs)/_layout.tsx`, `apps/mobile/src/app/(tabs)/index.tsx`, `apps/mobile/src/app/(tabs)/lighting.tsx`, `apps/mobile/src/app/(tabs)/settings.tsx`
- Modify: `apps/mobile/src/app/_layout.tsx`
- Delete: `apps/mobile/src/app/index.tsx`, `apps/mobile/src/app/settings.tsx`

- [ ] **Step 1: Reduce the root layout to providers plus a Slot**

`apps/mobile/src/app/_layout.tsx`. The tab group owns its own headers, so the
root no longer needs a `Stack`:

```tsx
import { Slot } from "expo-router";
import { AppProviders } from "@/components/utility/app-providers";

export default function RootLayout() {
  return (
    <AppProviders>
      <Slot />
    </AppProviders>
  );
}
```

- [ ] **Step 2: Write the tab layout**

`apps/mobile/src/app/(tabs)/_layout.tsx`:

```tsx
import { colors, screenStyles, typography } from "@aquarium/theme";
import Ionicons from "@expo/vector-icons/Ionicons";
import { Redirect, Tabs } from "expo-router";
import { Badge } from "@/components/atoms";
import { ScreenMessage } from "@/components/molecules";
import { useDevice } from "@/components/utility/app-providers";
import { streamBadge } from "@/components/utility/stream-badge";
import { useTelemetryStream } from "@/device/use-telemetry-stream";

export default function TabsLayout() {
  const { device, transport, loading } = useDevice();
  const streamState = useTelemetryStream(transport);
  const badge = streamBadge(streamState);

  if (loading) return <ScreenMessage loading>loading…</ScreenMessage>;
  if (!device) return <Redirect href="/settings" />;

  return (
    <Tabs
      screenOptions={{
        headerStyle: screenStyles.navigation.header,
        headerTintColor: colors.textMain,
        headerTitleStyle: { fontWeight: typography.weight.bold },
        headerRight: () => (
          <Badge variant={badge.variant} style={{ marginRight: 12 }}>
            {badge.label}
          </Badge>
        ),
        sceneStyle: screenStyles.page,
        tabBarStyle: screenStyles.navigation.header,
        tabBarActiveTintColor: colors.primary,
        tabBarInactiveTintColor: colors.textMuted,
      }}
    >
      <Tabs.Screen
        name="index"
        options={{
          title: "Telemetry",
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="pulse" color={color} size={size} />
          ),
        }}
      />
      <Tabs.Screen
        name="lighting"
        options={{
          title: "Lighting",
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="sunny" color={color} size={size} />
          ),
        }}
      />
      <Tabs.Screen
        name="settings"
        options={{
          title: "Device",
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="hardware-chip" color={color} size={size} />
          ),
        }}
      />
    </Tabs>
  );
}
```

The stream subscription lives here, not in the telemetry page, for two reasons:
the badge stays accurate while the user is on another tab, and there is exactly
one SSE connection rather than one per screen that wants live data. The firmware
allows four concurrent connections (`max_connections: 4` in `settings.rs`), so
duplicating the subscription is wasteful rather than harmless.

Task 8's `TelemetryPage` therefore loses its own subscription. Delete from
`apps/mobile/src/components/pages/TelemetryPage/TelemetryPage.tsx`:

- the line `useTelemetryStream(transport);`
- the import of `useTelemetryStream` from `@/device/use-telemetry-stream`

The page keeps reading `useTelemetry`, which reads the cache this layout's
subscription keeps fresh.

- [ ] **Step 3: Write the three route files**

`apps/mobile/src/app/(tabs)/index.tsx`:

```tsx
import { TelemetryPage } from "@/components/pages";

export default function Index() {
  return <TelemetryPage />;
}
```

`apps/mobile/src/app/(tabs)/lighting.tsx`:

```tsx
import { LightingPage } from "@/components/pages";

export default function Lighting() {
  return <LightingPage />;
}
```

`apps/mobile/src/app/(tabs)/settings.tsx`:

```tsx
import { DeviceSettingsPage } from "@/components/pages";

export default function Settings() {
  return <DeviceSettingsPage />;
}
```

- [ ] **Step 4: Remove the old routes**

```bash
cd apps/mobile && rm src/app/index.tsx src/app/settings.tsx
ls src/app src/app/\(tabs\)
```

Expected: `src/app` holds `_layout.tsx` and `(tabs)`; the group holds
`_layout.tsx`, `index.tsx`, `lighting.tsx`, `settings.tsx`.

- [ ] **Step 5: Fix the settings redirect target**

`DeviceSettingsPage` calls `router.replace("/")` after a successful save. With
the tab group that still resolves to the telemetry tab, so no change is needed —
verify rather than assume:

```bash
grep -n 'router.replace' src/components/pages/DeviceSettingsPage/DeviceSettingsPage.tsx
```

Expected: `router.replace("/")`. Leave it.

- [ ] **Step 6: Checkpoint**

```bash
cd apps/mobile && yarn typecheck && yarn eslint . && yarn vitest run 2>&1 | grep -E "Tests "
```

Expected: all clean, 54 tests.

---

## Task 11: Verify against the running device

**Files:** none — verification only.

- [ ] **Step 1: Full gate**

```bash
cd $(git rev-parse --show-toplevel)
. $HOME/export-esp.sh >/dev/null 2>&1
make ci 2>&1 | tail -6
```

Expected: `CI passed`.

- [ ] **Step 2: Bundle the app**

This is what proves Metro resolves the new slider dependency and the `(tabs)`
group.

```bash
cd apps/mobile
rm -rf /tmp/aquarium-export
CI=1 yarn expo export --platform web --output-dir /tmp/aquarium-export 2>&1 | tail -6
```

Expected: `Exported`, with no `Unable to resolve` lines.

- [ ] **Step 3: Confirm the screens reach the bundle**

```bash
D=/tmp/aquarium-export/client/_expo/static/js/web
for s in "restore schedule" "leak detected" "no limit" "water level"; do
  printf '%-20s ' "$s"; grep -rql "$s" "$D" && echo present || echo MISSING
done
```

Expected: all four `present`.

- [ ] **Step 4: Check the routes render**

The dev server must be running (`yarn expo start --port 8081`).

```bash
for r in "" settings lighting; do
  printf '%-10s ' "/$r"
  curl -s -o /dev/null -m 180 -w 'http %{http_code}\n' "http://localhost:8081/$r"
done
```

Expected: `http 200` for all three.

- [ ] **Step 5: Exercise it against the real controller**

With the firmware flashed and reachable (it was at `192.168.1.25`), open the app,
enter the host, port 80 and the `API_TOKEN` from `.env`, then check:

- the telemetry tab shows the hero card reading `—` with status `sensors off`,
  because the device is flashed with `temperature: Off`
- the water level card shows five chips
- the system card shows free heap in megabytes, which confirms PSRAM
- the header badge reads `live`
- the lighting tab: tapping `moonlight` with `15 min` sets an override, the
  "expires in" row counts down, the telemetry tab's lighting card flips to
  `override`, and `restore schedule` clears it

That last sequence is the one that cannot be checked any other way — it crosses
the app, the HTTP contract and the firmware's override state in one motion.

- [ ] **Step 6: Checkpoint**

```bash
cd $(git rev-parse --show-toplevel) && git status --short | head -40
```

Review the working tree with the user. The work is complete and uncommitted,
which is intended.

---

## Execution notes

**`free_heap_kb` is KiB, so the display unit is MiB.** The plan's test expected
`formatHeap(8302)` to read `8.3 MB`, which assumed a decimal kB. The firmware
computes `esp_get_free_heap_size() / 1024`
(`crates/firmware/src/infrastructure/system_health.rs`), so the field is
kibibytes and the right rendering is `8.1 MiB`. Implementation kept, test
expectation and unit labels corrected — `make size` already prints MiB, so this
also matches the project's own convention.

**`@expo/vector-icons` does not ship with Expo SDK 58.** The spec said it needed
no install; it did (`expo install @expo/vector-icons`, resolved to 15.0.2). Its
`tabBarIcon` render prop hands back RN's `ColorValue`, which admits `null`,
`number` and opaque platform colours, none of which Ionicons accepts — hence the
`iconColor` narrowing helper in the tab layout rather than a cast.

**The slider resolved to 5.2.1, not the 5.2.2 on npm.** `expo install` picks the
version matched to the SDK, which is why it was used instead of `yarn add`.

**The placeholder-transport trick in the plan was wrong and was replaced.**
Passing a dummy transport into `useTelemetry` so the hook could be called
unconditionally would have fired a real request at `http://:80`. Both
`TelemetryPage` and `LightingPage` instead check for a transport and render a
child component that owns the hook, so it only ever mounts with a real device.

**`react-hooks/set-state-in-effect` rejected the prop-to-state sync** in both
`Slider` and `useCountdown`. Both now adjust state during render against a
remembered previous value, which is React's documented pattern for this, and
`useCountdown` writes its callback ref inside an effect rather than during
render.

**The repo's formatter strips banner comments.** The `// ====` section headers
written into `packages/theme/src/index.ts` were removed, consistent with the
project's lean-comment convention, so the new sections were added without them.

**Hardware verification.** `make ci` passes; the web export contains every new
screen; all three tab routes answer 200. The lighting flow was then exercised
against the controller at 192.168.1.25: `DELETE` then `GET` gives 204, a
moonlight override for 900 s reads back `expires_in_s: 899`, that value fell to
896 over three seconds — confirming the countdown is live and not an echo —
manual channels 20/10 were accepted, telemetry reported
`override_active: true` with those channels, and `DELETE` restored the schedule.

What remains unverified is the rendering itself: nobody has yet tapped through
the tabs on a device or simulator.
