# Monorepo reorganization and mobile client — design

## Problem

The repository holds one Cargo workspace whose root is simultaneously the
workspace root and the firmware package. Adding a React Native client needs a
second workspace (yarn) alongside it, and the current layout has no place to put
it:

1. `Cargo.toml` mixes `[workspace]` with `[package]`, `[[bin]]` and firmware
   dependencies. Firmware build inputs (`build.rs`, `sdkconfig.defaults`,
   `partitions.csv`, `espflash.toml`, `src/`) sit loose in the root next to
   workspace-level concerns.
2. `.cargo/config.toml` in the root sets `[build] target =
   "xtensa-esp32s3-espidf"`. Cargo discovers config from the invocation
   directory upward, so that target applies to every crate in the tree. Host
   work has to opt out explicitly — `make test`, `make lint-core` and
   `test-watch` all carry `--target $(HOST_TARGET)`, and any new host crate
   inherits the same tax.
3. There is no shared place for an HTTP contract. The firmware already serves
   `/api/telemetry`, `/api/health` and `/api/lighting/override` plus an SSE
   stream at `/api/events`, with DTOs defined in `aquarium-core`. A TypeScript
   client re-typing those by hand would drift silently.

## Approach

Split the tree by toolchain: Rust crates under `crates/`, JavaScript packages
under `apps/` and `packages/`. The root becomes a pure workspace root for both
ecosystems. The firmware's build target moves into the firmware crate, so host
crates build natively with no override.

The HTTP contract is generated from the Rust DTOs with `ts-rs`. The generated
directory is gitignored and rebuilt by `make types`, so there is exactly one
source of truth — the Rust types — and no committed copy that can quietly fall
behind.

Scope of this spec: the reorganization, the yarn and Expo scaffold, the
`api-client` package, and one vertical slice proving the stack end to end
(settings screen + live telemetry screen). Remaining screens are a separate
spec.

## Target layout

```
/
├─ Cargo.toml                 # [workspace] + [patch.crates-io] + profiles only
├─ package.json               # private, yarn workspaces: apps/*, packages/*
├─ .yarnrc.yml                # nodeLinker: node-modules
├─ yarn.lock
├─ Makefile                   # single entrypoint, delegates to both worlds
│                             # note: root .cargo/config.toml is deleted —
│                             # everything in it was target-specific
├─ crates/
│  ├─ firmware/
│  │  ├─ .cargo/config.toml   # xtensa target, linker, runner, rustflags, env
│  │  ├─ Cargo.toml
│  │  ├─ build.rs
│  │  ├─ sdkconfig.defaults
│  │  ├─ partitions.csv
│  │  ├─ espflash.toml
│  │  └─ src/
│  └─ core/                   # aquarium-core, internals unchanged
├─ apps/
│  └─ mobile/                 # Expo + expo-router
└─ packages/
   └─ api-client/             # generated types + HTTP/SSE client + query hooks
```

Rust files move with `git mv` to keep blame history. `crates/core/` is a pure
path move: no edits inside except the `ts-export` feature described below.

## Cargo workspace split

Root `Cargo.toml` keeps `[workspace]` (`members = ["crates/*"]`, `resolver =
"2"`), `[patch.crates-io]` and the `release`/`dev` profiles. Profiles and
`[patch]` are only honoured at the workspace root, so they cannot move into the
firmware crate.

`crates/firmware/Cargo.toml` takes the `[package]` block (name
`aquarium-controller`, authors, edition 2021, rust-version 1.82), `[[bin]]`
(`harness = false`), all runtime dependencies and `[build-dependencies]`. The
core dependency becomes `aquarium-core = { path = "../core" }`.

## Build target relocation

This is the part that can break the firmware build, so it is specified
precisely.

`crates/firmware/.cargo/config.toml` receives everything from today's root
config: `[build] target`, the `cfg(target_os = "espidf")` linker / runner /
rustflags block, `[unstable] build-std`, and `[env]` (`MCU`, `ESP_IDF_VERSION`,
`ESP_IDF_TOOLS_INSTALL_DIR`). Nothing in that file is ecosystem-wide, so the
root copy is deleted rather than trimmed.

`rust-toolchain.toml` stays at the root, pinning the `esp` channel for the whole
tree. The esp toolchain builds host targets fine, which is how `make test` works
today; moving it would be unrequested churn.

Consequences, all of which the Makefile encodes so no one has to remember them:

- Firmware commands run from `crates/firmware/`. Cargo merges that config with
  the root one and still walks up to the workspace root for `target/` and
  profiles.
- Host commands run from the root. `cargo test -p aquarium-core` and
  `cargo clippy -p aquarium-core` build for the native host with no `--target`.
  `HOST_TARGET` disappears from `test`, `test-watch` and `lint-core`.
- `cargo clippy --all-targets --workspace` from the root would try to build the
  firmware crate for the host and fail. Workspace-wide clippy is replaced by two
  per-crate invocations, matching the existing `lint-core` / `lint-firmware`
  split.

Acceptance for this step is a flash to hardware, not a passing type-check. That
matters more than it sounds: the move breaks three path references that every
compile-time check passes straight through.

**`CARGO_WORKSPACE_DIR` is mandatory, not a fallback.** `esp-idf-sys` resolves
`sdkconfig.defaults` relative to embuild's workspace directory. Before the move
the repo root was simultaneously the workspace root, the package directory and
the home of `sdkconfig.defaults`, so it was found with no configuration. After
the move those roles split and nothing is found — and the build still *succeeds*,
silently, with stock ESP-IDF defaults. The commented-out
`CARGO_WORKSPACE_DIR = { value = "", relative = true }` must be uncommented in
`crates/firmware/.cargo/config.toml`, which sets the project directory to
`crates/firmware` and restores resolution.

What a stock sdkconfig means in practice, measured: `CONFIG_ESP_MAIN_TASK_STACK_SIZE`
drops from 131072 to 3584 — 36× smaller — and the firmware panics with
`LoadProhibited` immediately after the first log in `main`, registers full of
`0xa5a5a5a5` (FreeRTOS's stack fill pattern). PSRAM, the 240 MHz clock, the 16
lwIP sockets, the end-of-stack watchpoint and the 8 MB flash size are all lost at
the same time. `sdkconfig.defaults` documents that one picoserve connection costs
~78 KiB of stack on the main task, so 3584 bytes cannot survive boot.

**The partition CSV path inside `sdkconfig.defaults` must move too.**
`CONFIG_PARTITION_TABLE_CUSTOM_FILENAME` is relative to `esp-idf-sys`'s `OUT_DIR`,
where six `../` reach the workspace root — verified from the ninja error naming
the absolute path it looked for. Since `partitions.csv` now lives in the firmware
crate, the value becomes
`"../../../../../../crates/firmware/partitions.csv"`.

**`espflash` must run from `crates/firmware`.** It reads `espflash.toml` from its
working directory, and that file is what points it at `partitions.csv`. Run from
the repo root it finds no config and derives a partition layout from flash size
instead — observable as `App/part. size: …/16,384,000 bytes` where the real
factory partition is 3,145,728. The Makefile's `flash`, `flash-debug`, `monitor`
and `erase` recipes therefore `cd $(FW_DIR)` and reach back to the artifacts with
`$(FW_TO_ROOT)`.

All three are invisible to `cargo check`, `cargo clippy` and `cargo build`. Only
a boot proves them.

## Contract generation

`aquarium-core` gains an optional feature `ts-export` pulling `ts-rs` as an
optional dependency. The already-clean DTO layer is the entire export surface:

- `domain/metrics.rs` — `TankTelemetryDto`, `TemperatureMetricsDto`,
  `FiltrationMetricsDto`, `Co2MetricsDto`, `AtoMetricsDto`, `SystemHealthDto`,
  `WaterLevelEntryDto`, `WaterLevelMetricsDto`, `LightingMetricsDto`,
  `SystemMode`, `LightingProfile`
- `domain/light.rs` — `ChannelLevels`, `LightingTarget`, `LightingOverride`
- `adapters/inbound/http/handlers/lighting.rs` — `SetOverrideRequest`, the
  `PUT` body. It lives in the handler rather than the domain, and its field is
  `duration_s` while the response's is `expires_in_s`. These differ in meaning,
  not just in name: `duration_s` is the lifetime being requested, while
  `expires_in_s` is the countdown remaining, already decremented by the time the
  response is written. Asking for 600 reads back 599. A client must not assert
  equality between them.

`domain/water.rs` is deliberately not in the surface: `WaterLevelState` and
`WaterLevelRole` never reach the wire, since `WaterLevelMetricsDto` flattens
them into `WaterLevelEntryDto { wet, faulty }`.

Each gets `#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]` plus
`#[cfg_attr(feature = "ts-export", ts(export))]`. Existing `#[serde(rename_all
= "snake_case")]` attributes are mirrored with the matching `ts(rename_all)` so
the generated names match the wire format rather than the Rust identifiers.

`ts-rs` emits during a test run, and `TS_RS_EXPORT_DIR` is resolved relative to
the crate's own manifest directory, not the invocation directory — so `make
types` runs the export from `crates/core` with `../../packages/api-client/src/generated`.
The barrel over the per-type files is generated in the same step, which makes the
whole directory a build product rather than a mix of generated and hand-written
files.

The directory is gitignored. That trades one guarantee for another: a checkout
cannot type-check the app until `make types` has run, but there is no committed
copy of the contract that can drift from the Rust types. The drift check changes
shape accordingly — instead of `git diff --exit-code` against a committed copy,
`make types-check` regenerates and then type-checks the client against it. That
is the stronger check: a diff only reports that the contract moved, while a
type-check reports that the move broke something.

Generation runs on stable Rust (`cargo +stable`), even though `rust-toolchain.toml`
pins the `esp` channel for the tree: `aquarium-core` is a plain host crate, so CI
can generate the contract without installing the xtensa toolchain.

Two fields need an explicit override. `timestamp_epoch_s` and `uptime_seconds`
are `u64`, which ts-rs maps to `bigint` — but `serde_json` writes them as JSON
numbers, so `JSON.parse` yields a `number` and the generated type would lie.
Both carry seconds, comfortably inside the JS safe-integer range, so they are
pinned with `#[ts(type = "number")]`.

ts-rs 12 enables `serde-compat` by default and therefore already honours
`#[serde(rename_all)]`; mirroring it with `ts(rename_all)` would be a second
source of truth for the same renaming, so the derives stay bare.

The generated types describe the firmware this repository builds, not necessarily
the firmware flashed on the device in front of you. A thin `zod` schema at the
parse boundary checks that the fields the app reads are present, without
restating the whole tree.

## `packages/api-client`

A source-only workspace: `exports` points at `src/index.ts`, there is no build
step. Metro transpiles the TypeScript directly and `tsc --noEmit` guards types.
This avoids a stale `dist/` and a build ordering problem in the Metro pipeline.

```
packages/api-client/src/
├─ generated/      # ts-rs output, committed, never hand-edited
├─ transport.ts    # injected fetch implementation
├─ config.ts       # baseUrl + token, no ambient state
├─ client.ts       # typed calls to the four REST endpoints
├─ events.ts       # SSE stream parsing
├─ queries.ts      # queryKeys, hooks, mutations
└─ index.ts
```

`transport.ts` exists so the package tests on Node without React Native in the
loop: the fetch implementation is a constructor argument, defaulting to
`expo/fetch` in the app and to the platform `fetch` in tests.

`client.ts` covers exactly what the firmware serves today: `GET /api/telemetry`,
`GET /api/health`, and `GET` / `PUT` / `DELETE` on `/api/lighting/override`.
Authentication is `Authorization: Bearer <token>`. A non-2xx response maps to a
typed error distinguishing unauthorized (the firmware answers 401 with
`unauthorized\n`) from transport failure and from malformed payload, because the
app shows three different things for those three cases.

`events.ts` streams `/api/events`. `EventSource` cannot set headers, which is
why the firmware's `ApiTokenLayer` accepts a token in the query string on that
one path; the client uses that door. Streaming goes through `expo/fetch`, whose
WinterCG-compatible response exposes a `ReadableStream`. React Native's built-in
`fetch` is XHR-backed and cannot stream, so this was the deciding factor for
Expo over bare RN. The exact Expo SDK version providing `expo/fetch` is
confirmed during scaffold; if it is unavailable, the fallback is a `react-native-sse`
style XHR reader behind the same `transport.ts` seam, with no change above it.

`queries.ts` exposes `queryKeys` plus `useTelemetry`, `useHealth`,
`useLightingOverride`, `useSetLightingOverride`, `useClearLightingOverride`. The
SSE stream pushes fresh snapshots into the cache via
`queryClient.setQueryData` on the telemetry key, so polling stays only as a
fallback for a dropped stream rather than as the primary refresh path.

## `apps/mobile`

Expo with `expo-router` and TypeScript. `metro.config.js` extends
`expo/metro-config` with `watchFolders` pointing at the repo root and
`nodeModulesPaths` covering both the app and the root, so `packages/api-client`
resolves from source.

The firmware does not advertise mDNS, so the device address is entered by hand;
discovery is explicitly out of scope and left as a separate spec. The settings
screen is `react-hook-form` with a `zod` resolver validating host and port, and
it probes `GET /api/health` before saving so a typo fails at entry instead of on
the next screen. The token is stored in `expo-secure-store`, never in
`AsyncStorage`. `expo-secure-store` is native-only — its web entry point is an
empty object, so calling it under `expo start --web` throws
`ExpoSecureStore.default.getValueWithKeyAsync is not a function`. The web build
therefore resolves `src/device/store.web.ts`, which keeps the same three-function
API over `localStorage`; web is a layout preview, and that storage is readable by
any script on the origin, so the secure-store guarantee holds on device only.
Web also cannot reach the controller at all until the firmware sends CORS
headers, which it does not.

The form schema transforms `port` from the string the keyboard produces into a
number, so the form is typed with react-hook-form's three generics
(`useForm<DeviceFormValues, unknown, ParsedDeviceForm>`). Without the third, the
resolver's output type and the field type disagree and the resolver will not
type-check.

The telemetry screen renders a `TankTelemetryDto` snapshot and subscribes to the
SSE stream, showing connection state explicitly: connecting, live, reconnecting,
or unauthorized. `idle` is derived from the absence of a configured device rather
than stored, because setting it inside the effect body is a cascading render that
`react-hooks/set-state-in-effect` correctly rejects.

### Component architecture

Components follow Atomic Design, mirroring the structure and file conventions of
the existing portfolio design system
(`typescript/portfolio/client/src/components`): `atoms/`, `molecules/`,
`organisms/`, `pages/`, `utility/`, each component a PascalCase directory of
`types.ts`, `styles.ts`, and the component itself. `src/app/` holds only route
files that mount a page, so routing carries no layout.

The portfolio's components could not be copied: they are web components built on
DOM elements, `className`/BEM and imported `.scss`, none of which React Native
has. What transfers is the API and the look, so the atoms are reimplemented on
React Native primitives keeping the same prop vocabulary — `Button`'s eight
variants and three sizes, `Card`'s eight variants, `Input`'s label/error/
helperText/icons, `Alert`, `Badge`, `Spinner`, `TextDisplay`.

`styles/palette.scss` becomes `components/utility/theme.ts`: the Tokyo Night
palette verbatim, `rem` spacing resolved to px at 16, and the Sass colour
functions evaluated to literals (`color.adjust($tn-bg-night, -2%)` → `#161720`,
`color.adjust($tn-blue, 5%)` → `#92b3f8`). Shadow tokens become React Native
`boxShadow` arrays, which RN 0.76+ supports including `inset`.

Gradient variants use `expo-linear-gradient` with `start`/`end` set to the
diagonal that CSS `135deg` describes. Worth noting while reading the port: the
source's `btn--primary` gradient has both stops resolve to the same `#92b3f8`,
so it renders flat; the port keeps both stops rather than "fixing" it, so the
two stay in sync if the palette changes.

Only the atoms this client needs were ported. The portfolio's remaining atoms
(`Select`, `Switch`, `Tag`, `CheckBox`, `TextArea`, `CodeBlock`, `ToolTip`,
`Icon`, `Arrow`, `ListItem`, `LoadingBar`) and its molecules (`EntityTable`,
`Timeline`, `CalendarDropdown`, `MenuPortal`, …) are web-shaped and unused here;
porting them now would be speculative.

## Testing

`api-client` gets Vitest unit tests — pure TypeScript, no React Native in the
loop:

- URL construction, including the SSE token query parameter
- error mapping: 401, network failure, malformed body
- SSE frame parsing: multi-line `data:`, keepalive comments, partial chunks
  split across reads, stream end
- `queryKeys` stability

The mobile app gets `tsc --noEmit`, ESLint, and Vitest over its pure-logic
modules only — the device form schema, which has real branching worth asserting.
Components get no tests: `jest-expo` arrives when there is component behaviour
worth asserting, and adding it now would mean testing a scaffold.

Rust testing is unchanged apart from losing the `--target` override.

## CI

`rust_ci.yml` gains path filters so a change under `apps/` or `packages/` does
not spend the xtensa toolchain's build time, and its build step runs from
`crates/firmware`. Its workspace-wide clippy matrix entry splits into the
per-crate form described above.

A new `js_ci.yml` runs on changes to `apps/`, `packages/` and the root manifests:
`yarn install --immutable`, ESLint, `tsc --noEmit` across workspaces, and Vitest.

Because the contract is no longer committed, `js_ci.yml` has to generate it
before it can type-check. It installs stable Rust for that one step rather than
the xtensa toolchain, which keeps the JavaScript job independent of the firmware
job.

## Makefile

The Makefile stays the single entrypoint. Existing firmware targets keep their
names and gain the `crates/firmware` working directory; `.env`, partition and
image-size paths follow the moved files. Host targets drop `HOST_TARGET`.

New targets:

- `types` — regenerate the contract into `packages/api-client/src/generated`
- `types-check` — run `types`, then fail if the generated directory is dirty
- `app-install` — `yarn install --immutable`
- `app-start` — Expo dev server
- `js-lint` — ESLint plus `tsc --noEmit` across the yarn workspaces
- `js-test` — Vitest on `api-client`

`ci` becomes `fmt-check lint test types-check js-lint js-test`, so one command
still gates everything.

## Editor and container config

`.vscode/tasks.json` needs no change: every task already shells out to `make`,
so the moved paths are absorbed by the Makefile.

`.devcontainer/Dockerfile` needs Node, since the repo now builds in two
toolchains. Debian bookworm ships Node 18, which Expo 58 does not support, so it
comes from NodeSource — and it must be installed in the root stage, before the
image switches to the `esp` user. The Rust half of the image is untouched.

## Repository hygiene

`.gitignore` gains `node_modules/`, `.expo/`, `dist/`, and `.yarn/*` with
negations for `!.yarn/releases` and `!.yarn/patches`.

`packages/api-client/src/generated/` is ignored: it is a build product of
`make types`.

`Cargo.lock` stays ignored, as it is today. `yarn.lock` is committed — the app is
a deployed artifact rather than a library, and `yarn install --immutable` in CI
requires it.

An empty `yarn.lock` is also what marks this repository as a yarn project root:
there is a `package.json` in the user's home directory, and without the lockfile
yarn 4 treats this repo as a stray subdirectory of that one and refuses to
install.

## Out of scope

- Screens beyond settings and telemetry
- mDNS or any device discovery
- Cloud or backend services; the app talks to the ESP32 directly over LAN
- `jest-expo` component tests
- EAS build configuration and store submission
