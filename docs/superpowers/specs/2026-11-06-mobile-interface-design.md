# Mobile interface — design

## Problem

The mobile client proves the stack but does not present it. The telemetry screen
renders twelve undifferentiated `label / value` rows in one card — a data dump
that gives a tank temperature the same weight as free heap, and buries
`leak_detected` nine rows down. Of the four endpoints the firmware serves, the
app exercises three; `PUT`/`DELETE` on `/api/lighting/override` have typed hooks
in `api-client` (`useSetLightingOverride`, `useClearLightingOverride`) and no UI
at all.

Navigation is a two-entry `Stack` with `Link`s between screens, which is the
shape a scaffold has, not the shape a mobile app has.

## Approach

Standard mobile conventions throughout: bottom tab navigation, one screen per
concern, content in cards, pull-to-refresh. No novel interaction patterns — the
value here is legibility, not invention.

Three tabs: **Telemetry**, **Lighting**, **Device**. The existing design system
(`@aquarium/theme`, Tokyo Night) and the existing data layer (TanStack Query
hooks plus the SSE stream feeding the cache) are unchanged; this spec adds
components and screens on top of them.

HTTP contract changes are explicitly a separate spec. This one builds on the
contract as it is, including its two known quirks.

## Navigation

`src/app/(tabs)/_layout.tsx` holds an expo-router `Tabs`:

| Route | Tab | Page component |
| --- | --- | --- |
| `(tabs)/index` | Telemetry | `TelemetryPage` |
| `(tabs)/lighting` | Lighting | `LightingPage` |
| `(tabs)/settings` | Device | `DeviceSettingsPage` |

Icons come from `@expo/vector-icons`, which ships with Expo and renders on both
platforms. The template's `expo-symbols` is SF Symbols and therefore iOS-only,
so it is not used for navigation chrome.

The stream status badge moves into the tab header as a `headerRight`, so
connection state is visible from every tab rather than only from the telemetry
screen. It renders the existing `Badge` driven by `streamBadge(state)`, which
already maps every `StreamState` to a label and variant.

With no device stored, every tab but Device is useless, so the app lands on
Device: `(tabs)/index.tsx` returns expo-router's `<Redirect href="/settings" />`
while `useDevice().device` is null. The redirect lives in the index route rather
than in `(tabs)/_layout.tsx`, because `/settings` is itself a child of that
layout: a layout that returns a `Redirect` instead of its navigator can never
mount the route it redirects to, so every render re-issues the redirect. On web
that loop calls `window.history.replaceState` thousands of times a second until
Firefox's History API limiter throws `SecurityError: The operation is insecure.`
The layout still gates on `loading`, otherwise the first frame after launch
bounces to settings before the device store has answered. Telemetry and Lighting
need no redirect of their own — both pages already render `no device configured`
when `transport` is null.

## Telemetry screen

A `leak_detected` alert renders above everything else, in the `danger` variant.
It is the one reading a user must not have to scroll to find, so it does not
live in a card with its peers.

Then a hero card and six subsystem cards, in this order:

| Card | Primary | Secondary |
| --- | --- | --- |
| Temperature (hero) | `tank_celsius`, large | `target_celsius`, `heater_active`, `sump_celsius` |
| Filtration | `flow_rate_lph` | `flow_ok`, `filter_relay_active` |
| CO2 | `current_ph` against `drop_checker_ph_target` | `solenoid_open`, `ph_electrode_health` |
| ATO | `pump_active` | `sump_water_low`, `reservoir_empty`, `last_fill_duration_ms` |
| Water level | the five `WaterLevelEntryDto` as wet/dry chips | `sensors_healthy`, per-sensor `faulty` |
| Lighting | `profile`, `channel_white_pct` / `channel_rgb_pct` | `override_active`, link to the Lighting tab |
| System | `system_mode` | `wifi_rssi_dbm`, `free_heap_kb`, `uptime_seconds` |

Every card carries a status line: a `StatusDot` plus a word, driven by the
card's own health flag (`sensors_healthy`, `flow_ok`, …). A card whose sensors
are off therefore reads as "off" rather than as a column of zeroes. Each card
selector returns that status alongside its metrics, so the dot and the rows
cannot disagree.

`Option<f32>` fields arrive as `null` and render as an em dash. Never `0.00` —
the firmware sends `null` when a DS18B20 read fails validation or the component
is disabled, and a displayed zero would read as a real measurement. This matters
in practice right now: the device is flashed with `temperature: Off`, so
`tank_celsius` and `sump_celsius` are null on every snapshot.

Pull-to-refresh invalidates the telemetry query. The SSE stream keeps pushing
into the same cache key regardless, so the gesture is a manual override of a
feed that is already live, not the only way to get fresh data.

## Lighting screen

The screen reads `useLightingOverride()` for the override and the `lighting`
block of the telemetry snapshot for what is actually on.

**Current state**, at the top: resolved profile, both channel percentages, and —
when an override is active — a countdown.

**Profile** — five chips (`off`, `sunrise`, `daylight`, `sunset`, `moonlight`),
sending `PUT { target: { profile }, duration_s }`.

**Manual channels** — two sliders, white and RGB, 0–100 in steps of 1, sending
`PUT { target: { channels: { white_pct, rgb_pct } }, duration_s }`. The domain
rejects anything above 100 (`ChannelLevels::validate`), so the slider range is
the validation: the UI cannot construct a request the firmware will refuse.
Sliders commit on release rather than on every value change, so dragging does
not fire a request per pixel.

**Duration** — chips for 5 min, 15 min, 1 h, and no limit. "No limit" sends
`duration_s: null`. It must not send `0`: `SetOverrideRequest::validate` rejects
`Some(0)` with "duration must be positive", and a UI that offers an option the
API refuses is a bug.

**Restore schedule** — `DELETE`, clearing the override.

The mutations already write their response into the cache in `onSuccess`, so no
optimistic update is needed and none is added.

### The countdown

`expires_in_s` is the remaining lifetime, not an echo of the `duration_s` that
was requested — the firmware has already begun expiring it by the time it
answers, so asking for 600 reads back 599. The screen therefore treats it as a
countdown: it ticks locally once a second from the value last received, and
refetches the override when it reaches zero rather than trusting the local clock
to have stayed in step. A local tick alone would drift; a poll alone would make
the number jump.

## Components

Following the existing Atomic Design layout, each a PascalCase directory of
`types.ts` plus the component, with style definitions added to
`packages/theme`:

**Atoms** — `Chip` (selectable, with a selected state), `Slider` (wraps
`@react-native-community/slider` with theme colours), `StatusDot` (a coloured
dot for healthy / degraded / off).

**Molecules** — `MetricCard` (title, status line, `MetricList`),
`WaterLevelGrid` (the five sensors), `ProfilePicker`, `DurationPicker`,
`ChannelSliders`.

**Organisms** — `TelemetryDashboard` (the alert, hero and card sequence),
`LightingControl` (current state, pickers, sliders, clear action).

**Pages** — `TelemetryPage` rebuilt on `TelemetryDashboard`, `LightingPage` new,
`DeviceSettingsPage` unchanged.

**Utility** — `telemetry-metrics.ts` splits into one selector per card, each
taking a `TankTelemetryDto` and returning `Metric[]` plus a status; a
`useCountdown` hook for the override.

## New dependency

`@react-native-community/slider` 5.2.2, installed with `expo install` so the
version matches SDK 58. React Native has had no built-in slider since 0.60.
`@expo/vector-icons` needs no install — it ships with Expo.

## Error and empty states

| Condition | Presentation |
| --- | --- |
| No device stored | Redirect to the Device tab |
| `ApiError` kind `unauthorized` | `danger` alert naming the token, link to Device; the stream already stops retrying |
| `ApiError` kind `transport` | `warning` alert, device unreachable; the stream keeps retrying |
| `ApiError` kind `malformed` | `danger` alert listing the fields `assertTelemetry` rejected |
| `ApiError` kind `status` | `danger` alert showing the firmware's own message (e.g. "channel level out of range") |
| A metric is `null` | em dash |
| Override absent | Lighting screen shows the schedule as authoritative, no countdown |

## Testing

Vitest over the pure logic, as in the previous spec — no component tests, since
`jest-expo` would still be testing a tree rather than behaviour:

- each card's selector: field mapping, null handling, the status flag it derives
- `useCountdown` arithmetic: ticking down, clamping at zero, restarting when a
  new `expires_in_s` arrives
- profile and channel inputs mapping to `SetOverrideRequest`, including that
  "no limit" produces `duration_s: null` and never `0`
- the water-level grid reducing five `{wet, faulty}` entries to chip states

## Out of scope

- History, charts, any persisted time series
- Push notifications and alerting
- mDNS or device discovery
- HTTP contract changes, including the `timestamp_epoch_s` / `uptime_seconds`
  defect — a separate spec, per "UI first"
- Component-level tests
