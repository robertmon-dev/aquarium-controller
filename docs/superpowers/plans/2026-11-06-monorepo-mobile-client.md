# Monorepo reorganization and mobile client — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **NO COMMITS.** The user owns this repository's git history. Every task ends in a
> **Checkpoint** step that verifies the work by running commands — never `git commit`,
> never `git add`. `git mv` is used in Task 1 only to preserve blame on moved files;
> it stages moves but must not be followed by a commit.

**Goal:** Split the repository into a Rust workspace under `crates/` and a yarn
workspace under `apps/`+`packages/`, generate the HTTP contract from the Rust DTOs,
and ship an Expo app with a settings screen and a live telemetry screen.

**Architecture:** The firmware package moves out of the repository root into
`crates/firmware/`, taking its `.cargo/config.toml` with it so the xtensa build
target stops applying to the whole tree. `aquarium-core` gains an optional
`ts-export` feature that emits TypeScript for its wire DTOs into
`packages/api-client/src/generated`, committed and drift-checked in CI. The app
consumes `api-client` as a source-only yarn workspace, with a `fetch`
implementation injected at the boundary so the package tests on Node.

**Tech Stack:** Rust (esp toolchain, ESP-IDF 5.5.5, picoserve), ts-rs 12, yarn 4
(node-modules linker), Expo SDK 58 / React Native 0.88, expo-router 58, TanStack
Query 5, react-hook-form 7 + zod 4, Vitest 5.

**Spec:** `docs/superpowers/specs/2026-11-06-monorepo-mobile-client-design.md`

---

## File Structure

**Rust — moved, not rewritten:**

| Path | Responsibility |
| --- | --- |
| `Cargo.toml` | Workspace root only: members, resolver, `[patch.crates-io]`, profiles |
| `crates/firmware/Cargo.toml` | The `aquarium-controller` binary package |
| `crates/firmware/.cargo/config.toml` | xtensa target, linker, runner, rustflags, ESP-IDF env |
| `crates/firmware/{src,build.rs,sdkconfig.defaults,partitions.csv,espflash.toml}` | Unchanged content, new location |
| `crates/core/` | `aquarium-core`, unchanged except the `ts-export` feature |

**Rust — edited:**

| Path | Change |
| --- | --- |
| `crates/core/Cargo.toml` | Optional `ts-rs` dependency, `ts-export` feature |
| `crates/core/src/domain/metrics.rs` | `cfg_attr` TS derives on 9 DTOs + 2 enums |
| `crates/core/src/domain/light.rs` | `cfg_attr` TS derives on 3 types |
| `crates/core/src/adapters/inbound/http/handlers/lighting.rs` | `cfg_attr` TS derive on `SetOverrideRequest` |

**JavaScript — new:**

| Path | Responsibility |
| --- | --- |
| `package.json`, `.yarnrc.yml` | yarn 4 workspace root, no code |
| `packages/api-client/src/generated/` | ts-rs output, committed, never hand-edited |
| `packages/api-client/src/config.ts` | `DeviceConfig` type + URL building |
| `packages/api-client/src/transport.ts` | Injected fetch + typed `ApiError` |
| `packages/api-client/src/client.ts` | The four REST endpoints |
| `packages/api-client/src/events.ts` | SSE frame parser + telemetry stream |
| `packages/api-client/src/schema.ts` | zod guard at the parse boundary |
| `packages/api-client/src/queries.ts` | `queryKeys`, hooks, mutations |
| `apps/mobile/` | Expo app: settings screen, telemetry screen |

**Infrastructure — edited:**

`Makefile`, `.github/workflows/rust_ci.yml`, `.github/workflows/js_ci.yml` (new),
`.gitignore`, `.vscode/tasks.json`, `.devcontainer/Dockerfile`, `README.md`.

---

## Task 1: Move the Rust crates into `crates/`

**Files:**
- Create: `crates/firmware/.cargo/config.toml`, `crates/firmware/Cargo.toml`
- Move: `src/` → `crates/firmware/src/`, `build.rs`, `sdkconfig.defaults`, `partitions.csv`, `espflash.toml`, `core/` → `crates/core/`
- Modify: `Cargo.toml`
- Delete: `.cargo/config.toml`

Nothing here is test-driven: it is a file move whose verification is that the
firmware still type-checks and the host tests still run.

- [ ] **Step 1: Record the current baseline**

Before touching anything, capture what must still work afterwards:

```bash
cargo test -p aquarium-core --target x86_64-unknown-linux-gnu 2>&1 | tail -5
```

Expected: a passing test summary, e.g. `test result: ok. N passed; 0 failed`.
Write down N — Task 1 must end with the same N.

- [ ] **Step 2: Move the files with git mv**

```bash
mkdir -p crates/firmware
git mv core crates/core
git mv src crates/firmware/src
git mv build.rs crates/firmware/build.rs
git mv sdkconfig.defaults crates/firmware/sdkconfig.defaults
git mv partitions.csv crates/firmware/partitions.csv
git mv espflash.toml crates/firmware/espflash.toml
git mv .cargo crates/firmware/.cargo
```

`.cargo` moves wholesale: every setting in it is xtensa-specific, so there is no
root copy to keep. Do **not** commit — `git mv` only stages the rename so blame
survives when the user commits later.

- [ ] **Step 3: Write the new workspace root `Cargo.toml`**

Replace the whole file. `[patch.crates-io]` and the profiles are only honoured at
a workspace root, so they stay here rather than moving with the package.

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[profile.release]
opt-level = "s"

[profile.dev]
debug = true
opt-level = "z"

[patch.crates-io]
esp-idf-sys = { git = "https://github.com/esp-rs/esp-idf.git" }
esp-idf-hal = { git = "https://github.com/esp-rs/esp-idf.git" }
esp-idf-svc = { git = "https://github.com/esp-rs/esp-idf.git" }
```

- [ ] **Step 4: Write `crates/firmware/Cargo.toml`**

This is the `[package]` half of the old root manifest, with the core dependency
repointed from `core` to `../core`.

```toml
[package]
name = "aquarium-controller"
version = "0.1.0"
authors = ["Robert Moń <m0ni3v@gmail.com>"]
edition = "2021"
rust-version = "1.82"

[[bin]]
name = "aquarium-controller"
harness = false

[dependencies]
aquarium-core = { path = "../core" }

log = "0.4"
tracing = { version = "0.1", default-features = false, features = ["log"] }
tracing-subscriber = { version = "0.3", default-features = false, features = ["fmt", "ansi"] }

esp-idf-svc = { version = "0.53", features = ["critical-section", "embassy-time-driver", "embassy-sync"] }
esp-idf-hal = "0.47"
esp-idf-sys = { version = "0.38", default-features = false }

tokio = { version = "1.40", default-features = false, features = ["rt", "macros", "sync", "time", "net"] }
embassy-time = { version = "0.5", features = ["generic-queue-8"] }

picoserve = { version = "0.20.1", features = ["alloc", "tokio", "json"] }
serde = { version = "1.0", default-features = false, features = ["derive", "alloc"] }
serde_json = { version = "1.0", default-features = false, features = ["alloc"] }
anyhow = "1.0"
thiserror = "2.0.21"

[build-dependencies]
embuild = "0.33.5"
```

- [ ] **Step 5: Verify host tests now run with no target override**

The whole point of the move. From the repository root:

```bash
cargo test -p aquarium-core 2>&1 | tail -5
```

Expected: the same `test result: ok. N passed` as Step 1, with no `--target`
flag. If this instead tries to build for `xtensa-esp32s3-espidf`, a stray
`.cargo/config.toml` is still in the root or in `$HOME/.cargo/config.toml` —
check with `cargo config get build.target` before continuing.

- [ ] **Step 6: Verify the firmware type-checks from its own directory**

```bash
cd crates/firmware && . $HOME/export-esp.sh && cargo check --release 2>&1 | tail -20
```

Expected: `Finished` with no errors. The first run reconfigures ESP-IDF and takes
several minutes.

If it fails with an error mentioning the build or target directory, uncomment the
`CARGO_WORKSPACE_DIR` line in `crates/firmware/.cargo/config.toml`:

```toml
[env]
CARGO_WORKSPACE_DIR = { value = "", relative = true }
```

- [ ] **Step 7: Checkpoint**

```bash
cd $(git rev-parse --show-toplevel)
test -f crates/firmware/.cargo/config.toml || echo "FAIL: cargo config did not move"
test ! -e .cargo || echo "FAIL: root .cargo still exists"
grep -q '^\[package\]' Cargo.toml && echo "FAIL: root manifest still a package"
cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(sorted(p["name"] for p in json.load(sys.stdin)["packages"]))'
```

Expected: no `FAIL` lines, and `['aquarium-controller', 'aquarium-core']`.

---

## Task 2: Repoint the Makefile at the new layout

**Files:**
- Modify: `Makefile`

- [ ] **Step 1: Add a firmware directory variable**

After the `PORT` line in the variables block at the top, add:

```make
FW_DIR        := crates/firmware
```

Then change the artifact paths so they keep pointing at the workspace-level
`target/`, which has not moved:

```make
ELF_RELEASE   := target/$(TARGET)/release/$(BIN)
```

These four lines (`ELF_RELEASE`, `BL_RELEASE`, `BL_DEBUG`, `ELF_DEBUG`) need no
change — `target/` is still at the root. Verify that by reading them, then move
on.

- [ ] **Step 2: Run firmware cargo commands from the firmware directory**

Every firmware-side cargo invocation gains `cd $(FW_DIR) &&`. The four to change
are `build`, `build-debug`, `check` and `lint-firmware`:

```make
build: check-env ## Build firmware (release)
	@$(MSG) "Building firmware (release)"
	@$(LOAD_ENV) $(ESP_ENV) cd $(FW_DIR) && cargo build --release

build-debug: check-env ## Build firmware (debug)
	@$(MSG) "Building firmware (debug)"
	@$(LOAD_ENV) $(ESP_ENV) cd $(FW_DIR) && cargo build

check: check-env ## Type-check firmware without linking
	@$(MSG) "Checking firmware"
	@$(LOAD_ENV) $(ESP_ENV) cd $(FW_DIR) && cargo check --release

lint-firmware: check-env ## Run clippy on the firmware crate
	@$(MSG) "Clippy: firmware"
	@$(LOAD_ENV) $(ESP_ENV) cd $(FW_DIR) && cargo clippy --release -- -D warnings
```

Keep each recipe's existing surrounding lines (the `@$(MSG)` banner and any
trailing `$(OK)`); only the cargo line changes. `cd` inside a recipe affects only
that line's shell, so the `.env` path in `LOAD_ENV` still resolves from the root —
which is what we want, `.env` stays at the root.

- [ ] **Step 3: Drop the host-target overrides**

`test`, `test-watch` and `lint-core` no longer need `--target`, because the root
no longer forces one:

```make
test: ## Run host tests for aquarium-core
	@$(MSG) "Host tests ($(CORE_CRATE))"
	@cargo test -p $(CORE_CRATE)

test-watch: ## Re-run host tests on file changes (needs cargo-watch)
	@command -v cargo-watch >/dev/null 2>&1 || { $(ERR) "cargo-watch is missing: cargo install cargo-watch"; exit 1; }
	@cargo watch -x 'test -p $(CORE_CRATE)'

lint-core: ## Run clippy on aquarium-core (host target)
	@$(MSG) "Clippy: $(CORE_CRATE)"
	@cargo clippy -p $(CORE_CRATE) --all-targets -- -D warnings
```

Then delete the now-unused `HOST_TARGET ?= x86_64-unknown-linux-gnu` line from the
variables block.

- [ ] **Step 4: Fix the partition table path**

In the `size` target, the awk call reads `partitions.csv` from the root. Point it
at the moved file:

```make
	part=$$(awk -F',' '/^factory/ { gsub(/ /, "", $$5); print strtonum($$5) }' $(FW_DIR)/partitions.csv); \
```

In the `partitions` target, update the message that names the file:

```make
	@$(SUB) "espflash writes the table from $(FW_DIR)/partitions.csv - these two must agree"
```

- [ ] **Step 5: Verify**

```bash
make test 2>&1 | tail -5
make lint-core 2>&1 | tail -5
make size 2>&1 | tail -10
```

Expected: tests pass with the same count as Task 1, clippy is clean, and `size`
prints a byte figure against the factory partition rather than an awk error.

- [ ] **Step 6: Checkpoint**

```bash
grep -n 'HOST_TARGET' Makefile && echo "FAIL: HOST_TARGET still referenced"
grep -c 'cd $(FW_DIR) && cargo' Makefile
```

Expected: no `FAIL` line, and the count is `4`.

---

## Task 3: Generate the TypeScript contract from the Rust DTOs

**Files:**
- Modify: `crates/core/Cargo.toml`, `crates/core/src/domain/metrics.rs`, `crates/core/src/domain/light.rs`, `crates/core/src/adapters/inbound/http/handlers/lighting.rs`
- Create: `packages/api-client/src/generated/` (generated output)

- [ ] **Step 1: Add the optional ts-rs dependency and feature**

In `crates/core/Cargo.toml`, add to `[dependencies]`:

```toml
ts-rs = { version = "12.0.1", optional = true }
```

and a new section after `[dependencies]`:

```toml
[features]
ts-export = ["dep:ts-rs"]
```

- [ ] **Step 2: Derive TS on the metrics DTOs**

In `crates/core/src/domain/metrics.rs`, add a `cfg_attr` derive to each wire type.
There are eleven. The pattern, applied to `TankTelemetryDto`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct TankTelemetryDto {
```

Apply the identical two `cfg_attr` lines to: `TemperatureMetricsDto`,
`FiltrationMetricsDto`, `Co2MetricsDto`, `AtoMetricsDto`, `SystemHealthDto`,
`WaterLevelEntryDto`, `WaterLevelMetricsDto`, `LightingMetricsDto`.

The two enums already carry `#[serde(rename_all = "snake_case")]`, and ts-rs does
not read serde attributes by default, so they need the rename mirrored or the
generated union will say `"Off"` where the wire says `"off"`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "snake_case"))]
pub enum LightingProfile {
```

Apply the same to `SystemMode`.

Do **not** touch `WaterTemperature` or `Ph` — they are newtypes with private
fields that never reach the wire. Do not touch `domain/water.rs` at all;
`WaterLevelMetricsDto` flattens those types into `WaterLevelEntryDto` before
serialization.

- [ ] **Step 3: Derive TS on the lighting types**

In `crates/core/src/domain/light.rs`, add the plain pair to `ChannelLevels` and
`LightingOverride`:

```rust
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
```

`LightingTarget` is a serde-externally-tagged enum with snake_case renaming, so it
needs the rename too:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "snake_case"))]
pub enum LightingTarget {
    Profile(LightingProfile),
    Channels(ChannelLevels),
}
```

This must generate `{ profile: LightingProfile } | { channels: ChannelLevels }`.
Step 6 asserts exactly that.

- [ ] **Step 4: Derive TS on the request body**

`SetOverrideRequest` is the `PUT /api/lighting/override` body and lives in the
handler, not the domain. In
`crates/core/src/adapters/inbound/http/handlers/lighting.rs`:

```rust
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct SetOverrideRequest {
    pub target: LightingTarget,
    pub duration_s: Option<u32>,
}
```

Note for later tasks: this struct's field is `duration_s`, while the
`LightingOverride` that comes back has `expires_in_s`. They are different names
for related values and the client must not conflate them.

- [ ] **Step 5: Generate**

```bash
mkdir -p packages/api-client/src/generated
TS_RS_EXPORT_DIR=packages/api-client/src/generated \
  cargo test -p aquarium-core --features ts-export 2>&1 | tail -5
ls packages/api-client/src/generated
```

Expected: tests pass, and the directory lists exactly these 15 `.ts` files, one
per derived type:

`AtoMetricsDto.ts`, `ChannelLevels.ts`, `Co2MetricsDto.ts`,
`FiltrationMetricsDto.ts`, `LightingMetricsDto.ts`, `LightingOverride.ts`,
`LightingProfile.ts`, `LightingTarget.ts`, `SetOverrideRequest.ts`,
`SystemHealthDto.ts`, `SystemMode.ts`, `TankTelemetryDto.ts`,
`TemperatureMetricsDto.ts`, `WaterLevelEntryDto.ts`, `WaterLevelMetricsDto.ts`.

Assert on the names rather than only the count, so a missing derive is visible.

- [ ] **Step 6: Verify the generated output matches the wire format**

Guessing wrong about serde-to-ts-rs translation is the main risk in this task, so
check the three cases where they could diverge:

```bash
cat packages/api-client/src/generated/LightingProfile.ts
cat packages/api-client/src/generated/LightingTarget.ts
grep -n 'tank_celsius' packages/api-client/src/generated/TemperatureMetricsDto.ts
```

Expected:
- `LightingProfile` is `"off" | "sunrise" | "daylight" | "sunset" | "moonlight"` —
  lowercase. If it is `"Off" | ...`, the `ts(rename_all)` from Step 2 is missing.
- `LightingTarget` is `{ "profile": LightingProfile } | { "channels": ChannelLevels }`.
- `tank_celsius` is `number | null`, not `number | undefined` — `Option<f32>`
  serializes to `null` through serde_json.

- [ ] **Step 7: Write an index barrel**

ts-rs emits one file per type and no barrel. Create
`packages/api-client/src/generated/index.ts` by hand — it is the one file in this
directory that is not generated, so mark it:

```ts
// Hand-written barrel over the ts-rs output. Everything else here is generated.
export type { AtoMetricsDto } from "./AtoMetricsDto";
export type { ChannelLevels } from "./ChannelLevels";
export type { Co2MetricsDto } from "./Co2MetricsDto";
export type { FiltrationMetricsDto } from "./FiltrationMetricsDto";
export type { LightingMetricsDto } from "./LightingMetricsDto";
export type { LightingOverride } from "./LightingOverride";
export type { LightingProfile } from "./LightingProfile";
export type { LightingTarget } from "./LightingTarget";
export type { SetOverrideRequest } from "./SetOverrideRequest";
export type { SystemHealthDto } from "./SystemHealthDto";
export type { SystemMode } from "./SystemMode";
export type { TankTelemetryDto } from "./TankTelemetryDto";
export type { TemperatureMetricsDto } from "./TemperatureMetricsDto";
export type { WaterLevelEntryDto } from "./WaterLevelEntryDto";
export type { WaterLevelMetricsDto } from "./WaterLevelMetricsDto";
```

If Step 5 produced a name not in this list, add it. The barrel must be exhaustive —
Task 11 imports from it.

- [ ] **Step 8: Verify the default build is unaffected**

The feature must be genuinely optional: firmware never enables it.

```bash
cargo test -p aquarium-core 2>&1 | tail -3
cargo tree -p aquarium-core 2>&1 | grep -c ts-rs
```

Expected: tests pass, and the `grep -c` prints `0` — ts-rs is absent from the
default dependency graph.

- [ ] **Step 9: Checkpoint**

```bash
grep -rn 'ts(export' crates/core/src | wc -l
ls packages/api-client/src/generated/*.ts | wc -l
```

Expected: 15 derive sites and 16 files (15 generated + the barrel).

---

## Task 4: Stand up the yarn 4 workspace root

**Files:**
- Create: `package.json`, `.yarnrc.yml`
- Modify: `.gitignore`

- [ ] **Step 1: Enable yarn 4 via corepack**

The machine has yarn 1.22 on `PATH`; corepack pins a modern yarn per-repository
instead of replacing it globally.

```bash
corepack enable
corepack use yarn@4.18.1
```

Expected: a `packageManager` field appears in `package.json` (corepack creates the
file if missing) and `.yarn/releases/yarn-4.18.1.cjs` is written. Confirm:

```bash
yarn --version
```

Expected: `4.18.1`. If corepack resolves a newer 4.x, use that version
consistently in the next step rather than downgrading.

- [ ] **Step 2: Write the root `package.json`**

Overwrite whatever corepack generated, keeping its `packageManager` value — read
the file first and substitute the version corepack actually wrote if it differs
from `4.18.1`:

```json
{
  "name": "aquarium-controller-monorepo",
  "private": true,
  "packageManager": "yarn@4.18.1",
  "workspaces": [
    "apps/*",
    "packages/*"
  ],
  "scripts": {
    "lint": "yarn workspaces foreach --all --parallel run lint",
    "typecheck": "yarn workspaces foreach --all --parallel run typecheck",
    "test": "yarn workspaces foreach --all run test"
  }
}
```

- [ ] **Step 3: Write `.yarnrc.yml`**

```yaml
nodeLinker: node-modules
enableGlobalCache: true
```

`node-modules` is not the yarn 4 default and is not optional here: Metro cannot
resolve through Plug'n'Play.

- [ ] **Step 4: Extend `.gitignore`**

Append to the existing file, keeping the current Rust entries:

```gitignore
node_modules/
dist/
.expo/

.yarn/*
!.yarn/releases
!.yarn/patches
```

- [ ] **Step 5: Verify**

```bash
yarn install
git status --short .yarn | head
```

Expected: install succeeds reporting zero workspace dependencies so far, and
`git status` shows `.yarn/releases/yarn-4.*.cjs` as the only untracked `.yarn`
entry — the cache and install state are ignored.

- [ ] **Step 6: Checkpoint**

```bash
test -f yarn.lock || echo "FAIL: no lockfile"
grep -q 'nodeLinker: node-modules' .yarnrc.yml || echo "FAIL: PnP still active"
git check-ignore -q node_modules && echo "node_modules ignored"
```

Expected: no `FAIL` lines, and the `node_modules ignored` confirmation.

---

## Task 5: `api-client` — package skeleton and config

**Files:**
- Create: `packages/api-client/package.json`, `packages/api-client/tsconfig.json`, `packages/api-client/vitest.config.ts`, `packages/api-client/src/config.ts`, `packages/api-client/src/config.test.ts`

- [ ] **Step 1: Write `packages/api-client/package.json`**

Source-only: `exports` points at TypeScript, there is no build step and no
`dist/`. Metro transpiles it directly, which avoids a stale build artifact and a
build-ordering problem in the Metro pipeline.

```json
{
  "name": "@aquarium/api-client",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "main": "./src/index.ts",
  "exports": {
    ".": "./src/index.ts"
  },
  "scripts": {
    "lint": "eslint src",
    "typecheck": "tsc --noEmit",
    "test": "vitest run"
  },
  "dependencies": {
    "zod": "4.6.7"
  },
  "peerDependencies": {
    "@tanstack/react-query": "^5.106.1"
  },
  "devDependencies": {
    "@tanstack/react-query": "5.106.1",
    "typescript": "5.9.3",
    "vitest": "5.1.0"
  }
}
```

- [ ] **Step 2: Write `packages/api-client/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "jsx": "react-jsx",
    "noEmit": true,
    "skipLibCheck": true,
    "types": ["vitest/globals"]
  },
  "include": ["src"]
}
```

`DOM` is in `lib` for `ReadableStream` and `AbortSignal`, which the SSE reader
uses. `noUncheckedIndexedAccess` matters in Task 7, where the parser indexes into
split arrays.

- [ ] **Step 3: Write `packages/api-client/vitest.config.ts`**

```ts
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    globals: true,
    environment: "node",
  },
});
```

- [ ] **Step 4: Install**

```bash
yarn install
```

Expected: resolves `zod`, `vitest`, `typescript` and `@tanstack/react-query` into
the root `node_modules`.

- [ ] **Step 5: Write the failing test for config**

`packages/api-client/src/config.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { baseUrl, endpointUrl, eventsUrl } from "./config";

const device = { host: "192.168.1.50", port: 80, token: "s3cret" };

describe("baseUrl", () => {
  it("builds an http origin from host and port", () => {
    expect(baseUrl(device)).toBe("http://192.168.1.50:80");
  });
});

describe("endpointUrl", () => {
  it("joins the origin with an api path", () => {
    expect(endpointUrl(device, "/api/telemetry")).toBe(
      "http://192.168.1.50:80/api/telemetry",
    );
  });
});

describe("eventsUrl", () => {
  it("carries the token in the query string", () => {
    expect(eventsUrl(device)).toBe(
      "http://192.168.1.50:80/api/events?token=s3cret",
    );
  });

  it("percent-encodes a token containing url-unsafe characters", () => {
    expect(eventsUrl({ ...device, token: "a+b/c=" })).toBe(
      "http://192.168.1.50:80/api/events?token=a%2Bb%2Fc%3D",
    );
  });
});
```

The token in the query string is not a shortcut: `/api/events` is the one path
where the firmware's `ApiTokenLayer` accepts it, because `EventSource` and
streaming readers cannot always set an `Authorization` header. The encoding test
exists because `make token` mints base64-derived tokens.

- [ ] **Step 6: Run the test to verify it fails**

```bash
cd packages/api-client && yarn test 2>&1 | tail -10
```

Expected: FAIL — `Failed to resolve import "./config"`.

- [ ] **Step 7: Write `packages/api-client/src/config.ts`**

```ts
export type DeviceConfig = {
  host: string;
  port: number;
  token: string;
};

export type ApiPath =
  | "/api/telemetry"
  | "/api/health"
  | "/api/lighting/override";

export const EVENTS_PATH = "/api/events";

export function baseUrl(device: DeviceConfig): string {
  return `http://${device.host}:${device.port}`;
}

export function endpointUrl(device: DeviceConfig, path: ApiPath): string {
  return `${baseUrl(device)}${path}`;
}

export function eventsUrl(device: DeviceConfig): string {
  const token = encodeURIComponent(device.token);
  return `${baseUrl(device)}${EVENTS_PATH}?token=${token}`;
}
```

The firmware serves plain HTTP with no TLS, so the scheme is hardcoded rather
than configurable — a configurable scheme would imply support that does not
exist.

- [ ] **Step 8: Run the test to verify it passes**

```bash
cd packages/api-client && yarn test 2>&1 | tail -10
```

Expected: PASS, 4 tests.

- [ ] **Step 9: Checkpoint**

```bash
cd packages/api-client && yarn typecheck && yarn test 2>&1 | tail -3
```

Expected: no type errors, 4 passing tests.

---

## Task 6: `api-client` — transport and REST client

**Files:**
- Create: `packages/api-client/src/transport.ts`, `packages/api-client/src/transport.test.ts`, `packages/api-client/src/client.ts`, `packages/api-client/src/client.test.ts`

- [ ] **Step 1: Write the failing test for the error taxonomy**

`packages/api-client/src/transport.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { ApiError, request } from "./transport";

const device = { host: "10.0.0.2", port: 80, token: "tok" };

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("request", () => {
  it("sends the token as a bearer header", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(jsonResponse({ ok: true }));

    await request({ device, fetchImpl }, "/api/health");

    const [, init] = fetchImpl.mock.calls[0]!;
    expect(init.headers.Authorization).toBe("Bearer tok");
  });

  it("returns the parsed body on success", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(jsonResponse({ uptime_seconds: 7 }));

    await expect(request({ device, fetchImpl }, "/api/health")).resolves.toEqual({
      uptime_seconds: 7,
    });
  });

  it("maps 401 to an unauthorized ApiError", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(new Response("unauthorized\n", { status: 401 }));

    const error = await request({ device, fetchImpl }, "/api/health").catch((e) => e);

    expect(error).toBeInstanceOf(ApiError);
    expect(error.kind).toBe("unauthorized");
  });

  it("maps a thrown fetch to a transport ApiError", async () => {
    const fetchImpl = vi.fn().mockRejectedValue(new TypeError("Network request failed"));

    const error = await request({ device, fetchImpl }, "/api/health").catch((e) => e);

    expect(error).toBeInstanceOf(ApiError);
    expect(error.kind).toBe("transport");
  });

  it("maps a non-json body to a malformed ApiError", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(new Response("<html>nope", { status: 200 }));

    const error = await request({ device, fetchImpl }, "/api/health").catch((e) => e);

    expect(error).toBeInstanceOf(ApiError);
    expect(error.kind).toBe("malformed");
  });

  it("maps other error statuses to a status ApiError carrying the body", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(new Response("channel level out of range", { status: 400 }));

    const error = await request({ device, fetchImpl }, "/api/lighting/override").catch(
      (e) => e,
    );

    expect(error.kind).toBe("status");
    expect(error.status).toBe(400);
    expect(error.detail).toBe("channel level out of range");
  });

  it("treats 204 as an empty body", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));

    await expect(request({ device, fetchImpl }, "/api/lighting/override")).resolves.toBeUndefined();
  });
});
```

Four kinds, not one, because the app shows four different things: re-enter your
token, device unreachable, firmware too old, and a validated rejection with a
reason the firmware already phrases well (`channel level out of range`,
`duration must be positive`).

`204` needs its own case: the firmware answers it for both `GET` with no override
active and `DELETE`, and `response.json()` on an empty body throws.

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd packages/api-client && yarn vitest run src/transport.test.ts 2>&1 | tail -10
```

Expected: FAIL — `Failed to resolve import "./transport"`.

- [ ] **Step 3: Write `packages/api-client/src/transport.ts`**

```ts
import { endpointUrl, type ApiPath, type DeviceConfig } from "./config";

export type FetchImpl = (input: string, init?: RequestInit) => Promise<Response>;

export type Transport = {
  device: DeviceConfig;
  fetchImpl: FetchImpl;
};

export type ApiErrorKind = "unauthorized" | "transport" | "malformed" | "status";

export class ApiError extends Error {
  readonly kind: ApiErrorKind;
  readonly status?: number;
  readonly detail?: string;

  constructor(kind: ApiErrorKind, message: string, status?: number, detail?: string) {
    super(message);
    this.name = "ApiError";
    this.kind = kind;
    this.status = status;
    this.detail = detail;
  }
}

export async function request<T>(
  transport: Transport,
  path: ApiPath,
  init: RequestInit = {},
): Promise<T> {
  const url = endpointUrl(transport.device, path);

  let response: Response;
  try {
    response = await transport.fetchImpl(url, {
      ...init,
      headers: {
        Authorization: `Bearer ${transport.device.token}`,
        ...(init.body === undefined ? {} : { "content-type": "application/json" }),
        ...init.headers,
      },
    });
  } catch (cause) {
    throw new ApiError("transport", `cannot reach ${transport.device.host}`, undefined, String(cause));
  }

  if (response.status === 401) {
    throw new ApiError("unauthorized", "device rejected the api token", 401);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  if (!response.ok) {
    const detail = (await response.text().catch(() => "")).trim();
    throw new ApiError("status", `device returned ${response.status}`, response.status, detail);
  }

  try {
    return (await response.json()) as T;
  } catch (cause) {
    throw new ApiError("malformed", "device returned a body that is not json", response.status, String(cause));
  }
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd packages/api-client && yarn vitest run src/transport.test.ts 2>&1 | tail -10
```

Expected: PASS, 7 tests.

- [ ] **Step 5: Write the failing test for the REST client**

`packages/api-client/src/client.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { createClient } from "./client";

const device = { host: "10.0.0.2", port: 80, token: "tok" };

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("createClient", () => {
  it("gets a telemetry snapshot", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(jsonResponse({ timestamp_epoch_s: 42 }));
    const client = createClient({ device, fetchImpl });

    const snapshot = await client.telemetry();

    expect(fetchImpl.mock.calls[0]![0]).toBe("http://10.0.0.2:80/api/telemetry");
    expect(snapshot.timestamp_epoch_s).toBe(42);
  });

  it("gets system health", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(jsonResponse({ uptime_seconds: 10, wifi_rssi_dbm: -55, free_heap_kb: 120 }));
    const client = createClient({ device, fetchImpl });

    await expect(client.health()).resolves.toEqual({
      uptime_seconds: 10,
      wifi_rssi_dbm: -55,
      free_heap_kb: 120,
    });
  });

  it("returns null when no lighting override is active", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    const client = createClient({ device, fetchImpl });

    await expect(client.lightingOverride()).resolves.toBeNull();
  });

  it("puts a channels override with duration_s, not expires_in_s", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      jsonResponse({ target: { channels: { white_pct: 20, rgb_pct: 10 } }, expires_in_s: 600 }),
    );
    const client = createClient({ device, fetchImpl });

    await client.setLightingOverride({
      target: { channels: { white_pct: 20, rgb_pct: 10 } },
      duration_s: 600,
    });

    const [url, init] = fetchImpl.mock.calls[0]!;
    expect(url).toBe("http://10.0.0.2:80/api/lighting/override");
    expect(init.method).toBe("PUT");
    expect(JSON.parse(init.body)).toEqual({
      target: { channels: { white_pct: 20, rgb_pct: 10 } },
      duration_s: 600,
    });
  });

  it("puts a profile override", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(jsonResponse({ target: { profile: "moonlight" }, expires_in_s: null }));
    const client = createClient({ device, fetchImpl });

    await client.setLightingOverride({ target: { profile: "moonlight" }, duration_s: null });

    expect(JSON.parse(fetchImpl.mock.calls[0]![1].body).target).toEqual({ profile: "moonlight" });
  });

  it("deletes the override", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    const client = createClient({ device, fetchImpl });

    await client.clearLightingOverride();

    expect(fetchImpl.mock.calls[0]![1].method).toBe("DELETE");
  });
});
```

The `duration_s` test is there deliberately: the request field is `duration_s`
and the response field is `expires_in_s`, and sending the wrong one gets silently
dropped by serde rather than rejected.

- [ ] **Step 6: Run the test to verify it fails**

```bash
cd packages/api-client && yarn vitest run src/client.test.ts 2>&1 | tail -10
```

Expected: FAIL — `Failed to resolve import "./client"`.

- [ ] **Step 7: Write `packages/api-client/src/client.ts`**

```ts
import type {
  LightingOverride,
  SetOverrideRequest,
  SystemHealthDto,
  TankTelemetryDto,
} from "./generated";
import { request, type Transport } from "./transport";

export type DeviceClient = {
  telemetry: () => Promise<TankTelemetryDto>;
  health: () => Promise<SystemHealthDto>;
  lightingOverride: () => Promise<LightingOverride | null>;
  setLightingOverride: (body: SetOverrideRequest) => Promise<LightingOverride>;
  clearLightingOverride: () => Promise<void>;
};

export function createClient(transport: Transport): DeviceClient {
  return {
    telemetry: () => request<TankTelemetryDto>(transport, "/api/telemetry"),

    health: () => request<SystemHealthDto>(transport, "/api/health"),

    lightingOverride: async () => {
      const over = await request<LightingOverride | undefined>(
        transport,
        "/api/lighting/override",
      );
      return over ?? null;
    },

    setLightingOverride: (body) =>
      request<LightingOverride>(transport, "/api/lighting/override", {
        method: "PUT",
        body: JSON.stringify(body),
      }),

    clearLightingOverride: () =>
      request<void>(transport, "/api/lighting/override", { method: "DELETE" }),
  };
}
```

`lightingOverride` normalises the firmware's `204 No Content` to `null`, because
"no override active" is a value the UI renders, not an error.

- [ ] **Step 8: Run the test to verify it passes**

```bash
cd packages/api-client && yarn vitest run src/client.test.ts 2>&1 | tail -10
```

Expected: PASS, 6 tests.

- [ ] **Step 9: Checkpoint**

```bash
cd packages/api-client && yarn typecheck && yarn test 2>&1 | tail -3
```

Expected: no type errors, 17 passing tests across three files.

---

## Task 7: `api-client` — SSE parser and telemetry stream

**Files:**
- Create: `packages/api-client/src/events.ts`, `packages/api-client/src/events.test.ts`

- [ ] **Step 1: Write the failing test for frame parsing**

The parser is the riskiest code in the package: chunk boundaries do not respect
frame boundaries, so it is driven by tests over a byte stream.

`packages/api-client/src/events.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { SseDecoder, streamTelemetry } from "./events";

const device = { host: "10.0.0.2", port: 80, token: "tok" };

function streamOf(...chunks: string[]): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      controller.close();
    },
  });
}

describe("SseDecoder", () => {
  it("decodes a complete event", () => {
    const decoder = new SseDecoder();

    expect(decoder.push('event: telemetry\ndata: {"a":1}\n\n')).toEqual([
      { event: "telemetry", data: '{"a":1}' },
    ]);
  });

  it("buffers a frame split across chunks", () => {
    const decoder = new SseDecoder();

    expect(decoder.push("event: telemetry\ndata: {\"a\"")).toEqual([]);
    expect(decoder.push(":1}\n\n")).toEqual([{ event: "telemetry", data: '{"a":1}' }]);
  });

  it("decodes two events arriving in one chunk", () => {
    const decoder = new SseDecoder();

    expect(decoder.push('event: telemetry\ndata: 1\n\nevent: telemetry\ndata: 2\n\n')).toEqual([
      { event: "telemetry", data: "1" },
      { event: "telemetry", data: "2" },
    ]);
  });

  it("joins multi-line data with newlines", () => {
    const decoder = new SseDecoder();

    expect(decoder.push("event: telemetry\ndata: line1\ndata: line2\n\n")).toEqual([
      { event: "telemetry", data: "line1\nline2" },
    ]);
  });

  it("ignores keepalive comment lines", () => {
    const decoder = new SseDecoder();

    expect(decoder.push(":keepalive\n\n")).toEqual([]);
  });

  it("tolerates crlf line endings", () => {
    const decoder = new SseDecoder();

    expect(decoder.push("event: telemetry\r\ndata: 1\r\n\r\n")).toEqual([
      { event: "telemetry", data: "1" },
    ]);
  });

  it("defaults the event name to message when absent", () => {
    const decoder = new SseDecoder();

    expect(decoder.push("data: 1\n\n")).toEqual([{ event: "message", data: "1" }]);
  });
});

describe("streamTelemetry", () => {
  it("yields parsed snapshots from telemetry events", async () => {
    const body = streamOf(
      'event: telemetry\ndata: {"timestamp_epoch_s":1}\n\n',
      ":keepalive\n\n",
      'event: telemetry\ndata: {"timestamp_epoch_s":2}\n\n',
    );
    const fetchImpl = async () => new Response(body, { status: 200 });

    const seen: number[] = [];
    for await (const snapshot of streamTelemetry({ device, fetchImpl })) {
      seen.push(snapshot.timestamp_epoch_s);
    }

    expect(seen).toEqual([1, 2]);
  });

  it("requests the events url with the token in the query", async () => {
    const urls: string[] = [];
    const fetchImpl = async (url: string) => {
      urls.push(url);
      return new Response(streamOf(), { status: 200 });
    };

    for await (const _ of streamTelemetry({ device, fetchImpl })) {
      // drain
    }

    expect(urls[0]).toBe("http://10.0.0.2:80/api/events?token=tok");
  });

  it("throws an unauthorized ApiError on 401", async () => {
    const fetchImpl = async () => new Response("unauthorized\n", { status: 401 });

    const iterate = async () => {
      for await (const _ of streamTelemetry({ device, fetchImpl })) {
        // drain
      }
    };

    await expect(iterate()).rejects.toMatchObject({ kind: "unauthorized" });
  });

  it("skips a telemetry event whose data is not json", async () => {
    const body = streamOf(
      "event: telemetry\ndata: not-json\n\n",
      'event: telemetry\ndata: {"timestamp_epoch_s":5}\n\n',
    );
    const fetchImpl = async () => new Response(body, { status: 200 });

    const seen: number[] = [];
    for await (const snapshot of streamTelemetry({ device, fetchImpl })) {
      seen.push(snapshot.timestamp_epoch_s);
    }

    expect(seen).toEqual([5]);
  });
});
```

The firmware sends `event: telemetry` on connect and on every change, and
`write_keepalive()` emits a comment line on the keepalive timeout — hence the
comment and multi-event cases. The malformed-data case must not kill the stream:
one bad frame should not take down a live screen.

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd packages/api-client && yarn vitest run src/events.test.ts 2>&1 | tail -10
```

Expected: FAIL — `Failed to resolve import "./events"`.

- [ ] **Step 3: Write `packages/api-client/src/events.ts`**

```ts
import { eventsUrl } from "./config";
import type { TankTelemetryDto } from "./generated";
import { ApiError, type Transport } from "./transport";

export type SseEvent = {
  event: string;
  data: string;
};

export class SseDecoder {
  private buffer = "";

  push(chunk: string): SseEvent[] {
    this.buffer += chunk.replace(/\r\n/g, "\n").replace(/\r/g, "\n");

    const events: SseEvent[] = [];
    let boundary = this.buffer.indexOf("\n\n");

    while (boundary !== -1) {
      const frame = this.buffer.slice(0, boundary);
      this.buffer = this.buffer.slice(boundary + 2);

      const event = decodeFrame(frame);
      if (event) events.push(event);

      boundary = this.buffer.indexOf("\n\n");
    }

    return events;
  }
}

function decodeFrame(frame: string): SseEvent | null {
  let name: string | null = null;
  const data: string[] = [];

  for (const line of frame.split("\n")) {
    if (line === "" || line.startsWith(":")) continue;

    const colon = line.indexOf(":");
    const field = colon === -1 ? line : line.slice(0, colon);
    const rawValue = colon === -1 ? "" : line.slice(colon + 1);
    const value = rawValue.startsWith(" ") ? rawValue.slice(1) : rawValue;

    if (field === "event") name = value;
    else if (field === "data") data.push(value);
  }

  if (data.length === 0) return null;

  return { event: name ?? "message", data: data.join("\n") };
}

export async function* streamTelemetry(
  transport: Transport,
  signal?: AbortSignal,
): AsyncGenerator<TankTelemetryDto> {
  const url = eventsUrl(transport.device);

  let response: Response;
  try {
    response = await transport.fetchImpl(url, {
      headers: { accept: "text/event-stream" },
      ...(signal ? { signal } : {}),
    });
  } catch (cause) {
    throw new ApiError("transport", `cannot reach ${transport.device.host}`, undefined, String(cause));
  }

  if (response.status === 401) {
    throw new ApiError("unauthorized", "device rejected the api token", 401);
  }

  if (!response.ok) {
    throw new ApiError("status", `device returned ${response.status}`, response.status);
  }

  if (!response.body) {
    throw new ApiError("malformed", "event stream response had no body", response.status);
  }

  const reader = response.body.getReader();
  const utf8 = new TextDecoder();
  const decoder = new SseDecoder();

  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) return;

      for (const event of decoder.push(utf8.decode(value, { stream: true }))) {
        if (event.event !== "telemetry") continue;

        const snapshot = tryParse(event.data);
        if (snapshot) yield snapshot;
      }
    }
  } finally {
    reader.releaseLock();
  }
}

function tryParse(data: string): TankTelemetryDto | null {
  try {
    return JSON.parse(data) as TankTelemetryDto;
  } catch {
    return null;
  }
}
```

A dropped frame is logged by the caller, not thrown: the stream is a live view and
one unparseable snapshot is less bad than a dead screen.

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd packages/api-client && yarn vitest run src/events.test.ts 2>&1 | tail -10
```

Expected: PASS, 11 tests.

- [ ] **Step 5: Checkpoint**

```bash
cd packages/api-client && yarn typecheck && yarn test 2>&1 | tail -3
```

Expected: no type errors, 28 passing tests.

---

## Task 8: `api-client` — response guard and query hooks

**Files:**
- Create: `packages/api-client/src/schema.ts`, `packages/api-client/src/schema.test.ts`, `packages/api-client/src/queries.ts`, `packages/api-client/src/index.ts`

- [ ] **Step 1: Write the failing test for the response guard**

The generated types describe the firmware this repository builds, not necessarily
the firmware flashed on the device in front of you. The guard checks only the
fields the two screens read.

`packages/api-client/src/schema.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { assertTelemetry } from "./schema";

const valid = {
  timestamp_epoch_s: 1,
  system_mode: "normal",
  system_health: { uptime_seconds: 5, wifi_rssi_dbm: -50, free_heap_kb: 100 },
  temperature: {
    tank_celsius: 25.5,
    sump_celsius: null,
    target_celsius: 26,
    heater_active: false,
    sensors_healthy: true,
  },
  lighting: {
    profile: "daylight",
    channel_white_pct: 60,
    channel_rgb_pct: 40,
    override_active: false,
  },
  leak_detected: false,
};

describe("assertTelemetry", () => {
  it("accepts a snapshot carrying the fields the app reads", () => {
    expect(assertTelemetry(valid).timestamp_epoch_s).toBe(1);
  });

  it("accepts a null tank temperature", () => {
    const snapshot = { ...valid, temperature: { ...valid.temperature, tank_celsius: null } };

    expect(assertTelemetry(snapshot).temperature.tank_celsius).toBeNull();
  });

  it("rejects a snapshot missing the temperature block", () => {
    const { temperature, ...rest } = valid;

    expect(() => assertTelemetry(rest)).toThrow(/temperature/);
  });

  it("rejects an unknown system mode", () => {
    expect(() => assertTelemetry({ ...valid, system_mode: "on_fire" })).toThrow(
      /system_mode/,
    );
  });

  it("ignores extra fields a newer firmware may add", () => {
    expect(() => assertTelemetry({ ...valid, future_sensor: { ok: true } })).not.toThrow();
  });
});
```

The last case is the important one: a newer firmware adding a field must not break
an older app, so the schema is deliberately non-strict.

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd packages/api-client && yarn vitest run src/schema.test.ts 2>&1 | tail -10
```

Expected: FAIL — `Failed to resolve import "./schema"`.

- [ ] **Step 3: Write `packages/api-client/src/schema.ts`**

```ts
import { z } from "zod";
import type { TankTelemetryDto } from "./generated";
import { ApiError } from "./transport";

const telemetryGuard = z.object({
  timestamp_epoch_s: z.number(),
  system_mode: z.enum(["normal", "maintenance", "emergency_lockout"]),
  system_health: z.object({
    uptime_seconds: z.number(),
    wifi_rssi_dbm: z.number(),
    free_heap_kb: z.number(),
  }),
  temperature: z.object({
    tank_celsius: z.number().nullable(),
    sump_celsius: z.number().nullable(),
    target_celsius: z.number(),
    heater_active: z.boolean(),
    sensors_healthy: z.boolean(),
  }),
  lighting: z.object({
    profile: z.enum(["off", "sunrise", "daylight", "sunset", "moonlight"]),
    channel_white_pct: z.number(),
    channel_rgb_pct: z.number(),
    override_active: z.boolean(),
  }),
  leak_detected: z.boolean(),
});

export function assertTelemetry(value: unknown): TankTelemetryDto {
  const result = telemetryGuard.safeParse(value);

  if (!result.success) {
    const where = result.error.issues
      .map((issue) => issue.path.join("."))
      .filter((path) => path.length > 0)
      .join(", ");

    throw new ApiError(
      "malformed",
      `device telemetry is missing or malformed fields: ${where}`,
      undefined,
      where,
    );
  }

  return value as TankTelemetryDto;
}
```

The guard covers a subset and returns the original value cast to the generated
type: it is a compatibility check, not a parser, and restating the whole DTO tree
here would duplicate what ts-rs already owns.

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd packages/api-client && yarn vitest run src/schema.test.ts 2>&1 | tail -10
```

Expected: PASS, 5 tests.

- [ ] **Step 5: Write `packages/api-client/src/queries.ts`**

Hooks are not unit-tested here — they are thin wiring over tested functions, and
testing them would mean standing up a React renderer for no new information.

```ts
import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { createClient } from "./client";
import type { DeviceConfig } from "./config";
import type {
  LightingOverride,
  SetOverrideRequest,
  SystemHealthDto,
  TankTelemetryDto,
} from "./generated";
import type { Transport } from "./transport";

export const queryKeys = {
  telemetry: (device: DeviceConfig) => ["telemetry", device.host, device.port] as const,
  health: (device: DeviceConfig) => ["health", device.host, device.port] as const,
  lightingOverride: (device: DeviceConfig) =>
    ["lighting-override", device.host, device.port] as const,
};

export function useTelemetry(transport: Transport): UseQueryResult<TankTelemetryDto> {
  return useQuery({
    queryKey: queryKeys.telemetry(transport.device),
    queryFn: () => createClient(transport).telemetry(),
    staleTime: 5_000,
  });
}

export function useHealth(transport: Transport): UseQueryResult<SystemHealthDto> {
  return useQuery({
    queryKey: queryKeys.health(transport.device),
    queryFn: () => createClient(transport).health(),
  });
}

export function useLightingOverride(
  transport: Transport,
): UseQueryResult<LightingOverride | null> {
  return useQuery({
    queryKey: queryKeys.lightingOverride(transport.device),
    queryFn: () => createClient(transport).lightingOverride(),
  });
}

export function useSetLightingOverride(
  transport: Transport,
): UseMutationResult<LightingOverride, Error, SetOverrideRequest> {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (body: SetOverrideRequest) =>
      createClient(transport).setLightingOverride(body),
    onSuccess: (over) => {
      queryClient.setQueryData(queryKeys.lightingOverride(transport.device), over);
    },
  });
}

export function useClearLightingOverride(
  transport: Transport,
): UseMutationResult<void, Error, void> {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => createClient(transport).clearLightingOverride(),
    onSuccess: () => {
      queryClient.setQueryData(queryKeys.lightingOverride(transport.device), null);
    },
  });
}

export function publishTelemetry(
  queryClient: QueryClient,
  device: DeviceConfig,
  snapshot: TankTelemetryDto,
): void {
  queryClient.setQueryData(queryKeys.telemetry(device), snapshot);
}
```

`publishTelemetry` is how the SSE stream feeds the cache, so the telemetry screen
reads one source whether the value arrived by poll or by push.

- [ ] **Step 6: Write the query-key test**

The spec calls for key stability, and it is worth asserting: two devices must not
share a cache entry, or switching devices shows the previous tank's readings.

`packages/api-client/src/queries.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { queryKeys } from "./queries";

const a = { host: "10.0.0.2", port: 80, token: "tok" };
const b = { host: "10.0.0.3", port: 80, token: "tok" };

describe("queryKeys", () => {
  it("is stable across calls for the same device", () => {
    expect(queryKeys.telemetry(a)).toEqual(queryKeys.telemetry(a));
  });

  it("separates devices on different hosts", () => {
    expect(queryKeys.telemetry(a)).not.toEqual(queryKeys.telemetry(b));
  });

  it("separates devices on different ports", () => {
    expect(queryKeys.telemetry(a)).not.toEqual(queryKeys.telemetry({ ...a, port: 8080 }));
  });

  it("does not put the token in the cache key", () => {
    expect(JSON.stringify(queryKeys.telemetry(a))).not.toContain("tok");
  });
});
```

The last case is deliberate: cache keys end up in devtools and logs, so the
secret stays out of them. Run it:

```bash
cd packages/api-client && yarn vitest run src/queries.test.ts 2>&1 | tail -10
```

Expected: PASS, 4 tests. If the token case fails, `queryKeys` is spreading the
whole `DeviceConfig` instead of naming `host` and `port`.

- [ ] **Step 7: Write `packages/api-client/src/index.ts`**

```ts
export { baseUrl, endpointUrl, eventsUrl, EVENTS_PATH } from "./config";
export type { ApiPath, DeviceConfig } from "./config";
export { ApiError } from "./transport";
export type { ApiErrorKind, FetchImpl, Transport } from "./transport";
export { createClient } from "./client";
export type { DeviceClient } from "./client";
export { SseDecoder, streamTelemetry } from "./events";
export type { SseEvent } from "./events";
export { assertTelemetry } from "./schema";
export {
  publishTelemetry,
  queryKeys,
  useClearLightingOverride,
  useHealth,
  useLightingOverride,
  useSetLightingOverride,
  useTelemetry,
} from "./queries";
export type * from "./generated";
```

- [ ] **Step 8: Checkpoint**

```bash
cd packages/api-client && yarn typecheck && yarn test 2>&1 | tail -3
```

Expected: no type errors, 37 passing tests across five files.

---

## Task 9: Expo app scaffold wired into the monorepo

**Files:**
- Create: `apps/mobile/` (scaffold), `apps/mobile/metro.config.js`
- Modify: `apps/mobile/package.json`, `apps/mobile/tsconfig.json`

- [ ] **Step 1: Scaffold the app**

Run from the repository root. The template is the TypeScript + expo-router one.

```bash
yarn dlx create-expo-app@latest apps/mobile --template default --no-install
```

Expected: `apps/mobile` contains `app/`, `package.json`, `tsconfig.json` and
`app.json`. `--no-install` matters: the root yarn workspace owns installation.

- [ ] **Step 2: Add the app's dependencies**

Edit `apps/mobile/package.json`. Keep every dependency the template generated and
add these, then set `name` and the scripts:

```json
{
  "name": "mobile",
  "private": true,
  "scripts": {
    "start": "expo start",
    "android": "expo run:android",
    "ios": "expo run:ios",
    "lint": "expo lint",
    "typecheck": "tsc --noEmit"
  },
  "dependencies": {
    "@aquarium/api-client": "workspace:*",
    "@hookform/resolvers": "7.0.3",
    "@tanstack/react-query": "5.106.1",
    "expo-secure-store": "58.0.2",
    "react-hook-form": "7.91.0",
    "zod": "4.6.7"
  }
}
```

`workspace:*` is what makes yarn link `packages/api-client` from source rather
than fetching it.

- [ ] **Step 3: Teach Metro about the monorepo**

Create `apps/mobile/metro.config.js`. Without this, Metro refuses to read files
outside the app directory and `@aquarium/api-client` fails to resolve.

```js
const { getDefaultConfig } = require("expo/metro-config");
const path = require("node:path");

const projectRoot = __dirname;
const workspaceRoot = path.resolve(projectRoot, "../..");

const config = getDefaultConfig(projectRoot);

config.watchFolders = [workspaceRoot];
config.resolver.nodeModulesPaths = [
  path.resolve(projectRoot, "node_modules"),
  path.resolve(workspaceRoot, "node_modules"),
];
config.resolver.disableHierarchicalLookup = true;

module.exports = config;
```

- [ ] **Step 4: Let the app's TypeScript see the package source**

`@aquarium/api-client` ships `.ts`, not `.d.ts`, so the app's `tsconfig.json`
needs the workspace on its include path. Merge into the generated file, keeping
`"extends": "expo/tsconfig.base"`:

```json
{
  "extends": "expo/tsconfig.base",
  "compilerOptions": {
    "strict": true,
    "paths": {
      "@/*": ["./*"],
      "@aquarium/api-client": ["../../packages/api-client/src/index.ts"]
    }
  },
  "include": ["**/*.ts", "**/*.tsx", ".expo/types/**/*.ts", "expo-env.d.ts"]
}
```

- [ ] **Step 5: Install and verify resolution**

```bash
cd $(git rev-parse --show-toplevel) && yarn install
node -e "console.log(require.resolve('@aquarium/api-client/package.json'))"
```

Expected: install succeeds and the path printed is under
`packages/api-client`, not `node_modules/@aquarium`.

- [ ] **Step 6: Verify the app type-checks against the package**

```bash
cd apps/mobile && yarn typecheck 2>&1 | tail -10
```

Expected: no errors. If `Cannot find module '@aquarium/api-client'`, the `paths`
entry in Step 4 did not take — confirm `tsconfig.json` was merged, not replaced.

- [ ] **Step 7: Verify `expo/fetch` exists in this SDK**

The SSE stream needs a streaming `fetch`. Confirm the export is there before the
screens depend on it:

```bash
cd apps/mobile && node -e "console.log(Object.keys(require('expo/fetch')))"
```

Expected: the output includes `fetch`. If this throws
`Cannot find module 'expo/fetch'`, the installed SDK predates it — the fallback
is an XHR-based reader behind the same `FetchImpl` seam in `transport.ts`, with no
change to `client.ts`, `events.ts` or the screens.

- [ ] **Step 8: Checkpoint**

```bash
cd $(git rev-parse --show-toplevel)
grep -q 'workspace:\*' apps/mobile/package.json || echo "FAIL: package not linked"
grep -q 'watchFolders' apps/mobile/metro.config.js || echo "FAIL: metro not monorepo-aware"
yarn workspaces list
```

Expected: no `FAIL` lines, and the listing shows the root, `apps/mobile` and
`packages/api-client`.

---

## Task 10: Settings screen — device address and token

**Files:**
- Create: `apps/mobile/src/device-config.ts`, `apps/mobile/src/device-config.test.ts`, `apps/mobile/src/device-store.ts`, `apps/mobile/src/providers.tsx`, `apps/mobile/app/settings.tsx`
- Modify: `apps/mobile/app/_layout.tsx`, `apps/mobile/package.json`

- [ ] **Step 1: Add Vitest to the app for the pure-logic test**

The form schema is pure TypeScript and worth testing; the screen is not. Add to
`apps/mobile/package.json` `devDependencies` and scripts:

```json
{
  "scripts": {
    "test": "vitest run"
  },
  "devDependencies": {
    "vitest": "5.1.0"
  }
}
```

Create `apps/mobile/vitest.config.ts`:

```ts
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    globals: true,
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
```

Then `yarn install` from the root.

- [ ] **Step 2: Write the failing test for the form schema**

`apps/mobile/src/device-config.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { deviceFormSchema, toDeviceConfig } from "./device-config";

describe("deviceFormSchema", () => {
  it("accepts a host, port and token", () => {
    const parsed = deviceFormSchema.parse({
      host: "192.168.1.50",
      port: "80",
      token: "abc123",
    });

    expect(parsed.port).toBe(80);
  });

  it("defaults the port to 80 when left empty", () => {
    expect(deviceFormSchema.parse({ host: "10.0.0.2", port: "", token: "t" }).port).toBe(80);
  });

  it("rejects an empty host", () => {
    expect(() => deviceFormSchema.parse({ host: "", port: "80", token: "t" })).toThrow();
  });

  it("rejects a host with a scheme, since the client adds it", () => {
    expect(() =>
      deviceFormSchema.parse({ host: "http://10.0.0.2", port: "80", token: "t" }),
    ).toThrow(/scheme/);
  });

  it("rejects a non-numeric port", () => {
    expect(() => deviceFormSchema.parse({ host: "10.0.0.2", port: "80a", token: "t" })).toThrow();
  });

  it("rejects a port outside the valid range", () => {
    expect(() => deviceFormSchema.parse({ host: "10.0.0.2", port: "70000", token: "t" })).toThrow();
  });

  it("rejects an empty token", () => {
    expect(() => deviceFormSchema.parse({ host: "10.0.0.2", port: "80", token: "" })).toThrow();
  });
});

describe("toDeviceConfig", () => {
  it("trims whitespace pasted around the token", () => {
    const config = toDeviceConfig({ host: " 10.0.0.2 ", port: 80, token: " tok\n" });

    expect(config).toEqual({ host: "10.0.0.2", port: 80, token: "tok" });
  });
});
```

The scheme and trimming cases come from how these values get entered: `baseUrl`
already prepends `http://`, and a token pasted from a terminal arrives with a
trailing newline.

- [ ] **Step 3: Run the test to verify it fails**

```bash
cd apps/mobile && yarn test 2>&1 | tail -10
```

Expected: FAIL — `Failed to resolve import "./device-config"`.

- [ ] **Step 4: Write `apps/mobile/src/device-config.ts`**

```ts
import type { DeviceConfig } from "@aquarium/api-client";
import { z } from "zod";

export const deviceFormSchema = z.object({
  host: z
    .string()
    .trim()
    .min(1, "enter the device address")
    .refine((value) => !value.includes("://"), "drop the scheme, just the host or ip"),
  port: z
    .string()
    .trim()
    .transform((value) => (value === "" ? "80" : value))
    .refine((value) => /^\d+$/.test(value), "port must be a number")
    .transform(Number)
    .refine((value) => value >= 1 && value <= 65535, "port must be between 1 and 65535"),
  token: z.string().trim().min(1, "enter the api token"),
});

export type DeviceFormValues = {
  host: string;
  port: string;
  token: string;
};

export type ParsedDeviceForm = z.output<typeof deviceFormSchema>;

export function toDeviceConfig(parsed: ParsedDeviceForm): DeviceConfig {
  return {
    host: parsed.host.trim(),
    port: parsed.port,
    token: parsed.token.trim(),
  };
}
```

- [ ] **Step 5: Run the test to verify it passes**

```bash
cd apps/mobile && yarn test 2>&1 | tail -10
```

Expected: PASS, 8 tests.

- [ ] **Step 6: Write the secure store wrapper**

`apps/mobile/src/device-store.ts`:

```ts
import type { DeviceConfig } from "@aquarium/api-client";
import * as SecureStore from "expo-secure-store";

const KEY = "aquarium.device";

export async function loadDevice(): Promise<DeviceConfig | null> {
  const raw = await SecureStore.getItemAsync(KEY);
  if (!raw) return null;

  try {
    return JSON.parse(raw) as DeviceConfig;
  } catch {
    return null;
  }
}

export async function saveDevice(device: DeviceConfig): Promise<void> {
  await SecureStore.setItemAsync(KEY, JSON.stringify(device));
}

export async function clearDevice(): Promise<void> {
  await SecureStore.deleteItemAsync(KEY);
}
```

The whole config goes into the secure store rather than splitting the token out,
because the host and port are not worth a second storage mechanism.

- [ ] **Step 7: Write the providers**

`apps/mobile/src/providers.tsx` owns the query client, the stored device and the
`expo/fetch` transport, so no screen constructs a transport itself:

```tsx
import type { DeviceConfig, Transport } from "@aquarium/api-client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fetch as expoFetch } from "expo/fetch";
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { loadDevice, saveDevice } from "./device-store";

const queryClient = new QueryClient();

type DeviceContextValue = {
  device: DeviceConfig | null;
  transport: Transport | null;
  setDevice: (device: DeviceConfig) => Promise<void>;
  loading: boolean;
};

const DeviceContext = createContext<DeviceContextValue | null>(null);

export function AppProviders({ children }: { children: ReactNode }) {
  const [device, setDeviceState] = useState<DeviceConfig | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadDevice()
      .then(setDeviceState)
      .finally(() => setLoading(false));
  }, []);

  const value = useMemo<DeviceContextValue>(
    () => ({
      device,
      transport: device
        ? { device, fetchImpl: expoFetch as unknown as Transport["fetchImpl"] }
        : null,
      setDevice: async (next) => {
        await saveDevice(next);
        setDeviceState(next);
      },
      loading,
    }),
    [device, loading],
  );

  return (
    <QueryClientProvider client={queryClient}>
      <DeviceContext.Provider value={value}>{children}</DeviceContext.Provider>
    </QueryClientProvider>
  );
}

export function useDevice(): DeviceContextValue {
  const value = useContext(DeviceContext);
  if (!value) throw new Error("useDevice must be used inside AppProviders");
  return value;
}
```

- [ ] **Step 8: Mount the providers in the router layout**

Edit `apps/mobile/app/_layout.tsx`, wrapping whatever the template generated:

```tsx
import { Stack } from "expo-router";
import { AppProviders } from "../src/providers";

export default function RootLayout() {
  return (
    <AppProviders>
      <Stack>
        <Stack.Screen name="index" options={{ title: "Telemetry" }} />
        <Stack.Screen name="settings" options={{ title: "Device" }} />
      </Stack>
    </AppProviders>
  );
}
```

Delete any `app/(tabs)` directory the template created — this app has two screens
and does not need a tab layout.

- [ ] **Step 9: Write the settings screen**

`apps/mobile/app/settings.tsx`. The health probe before saving is the point: a
typo fails here, not two screens later.

```tsx
import { ApiError, createClient } from "@aquarium/api-client";
import { zodResolver } from "@hookform/resolvers/zod";
import { fetch as expoFetch } from "expo/fetch";
import { router } from "expo-router";
import { useState } from "react";
import { Controller, useForm } from "react-hook-form";
import { Button, Text, TextInput, View } from "react-native";
import { deviceFormSchema, toDeviceConfig, type DeviceFormValues } from "../src/device-config";
import { useDevice } from "../src/providers";

export default function SettingsScreen() {
  const { device, setDevice } = useDevice();
  const [probeError, setProbeError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);

  const { control, handleSubmit, formState } = useForm<DeviceFormValues>({
    resolver: zodResolver(deviceFormSchema),
    defaultValues: {
      host: device?.host ?? "",
      port: device ? String(device.port) : "80",
      token: device?.token ?? "",
    },
  });

  const onSubmit = handleSubmit(async (values) => {
    const config = toDeviceConfig(deviceFormSchema.parse(values));
    setProbing(true);
    setProbeError(null);

    try {
      await createClient({
        device: config,
        fetchImpl: expoFetch as never,
      }).health();
    } catch (error) {
      setProbeError(
        error instanceof ApiError && error.kind === "unauthorized"
          ? "the device rejected that token"
          : `could not reach ${config.host}:${config.port}`,
      );
      setProbing(false);
      return;
    }

    await setDevice(config);
    setProbing(false);
    router.replace("/");
  });

  return (
    <View style={{ padding: 16, gap: 12 }}>
      {(["host", "port", "token"] as const).map((name) => (
        <Controller
          key={name}
          control={control}
          name={name}
          render={({ field }) => (
            <View style={{ gap: 4 }}>
              <Text>{name}</Text>
              <TextInput
                value={field.value}
                onChangeText={field.onChange}
                onBlur={field.onBlur}
                autoCapitalize="none"
                autoCorrect={false}
                keyboardType={name === "port" ? "number-pad" : "default"}
                secureTextEntry={name === "token"}
                placeholder={name === "host" ? "192.168.1.50" : undefined}
                style={{ borderWidth: 1, borderRadius: 6, padding: 10 }}
              />
              {formState.errors[name] ? (
                <Text style={{ color: "#b00020" }}>{formState.errors[name]?.message}</Text>
              ) : null}
            </View>
          )}
        />
      ))}

      {probeError ? <Text style={{ color: "#b00020" }}>{probeError}</Text> : null}

      <Button
        title={probing ? "checking…" : "save and connect"}
        onPress={onSubmit}
        disabled={probing}
      />
    </View>
  );
}
```

- [ ] **Step 10: Checkpoint**

```bash
cd apps/mobile && yarn typecheck && yarn test 2>&1 | tail -3
```

Expected: no type errors, 8 passing tests.

---

## Task 11: Telemetry screen with the live stream

**Files:**
- Create: `apps/mobile/src/use-telemetry-stream.ts`, `apps/mobile/app/index.tsx`

- [ ] **Step 1: Write the stream hook**

`apps/mobile/src/use-telemetry-stream.ts`. It owns subscription lifetime and the
connection state the screen displays; the parsing and framing it relies on are
already tested in `api-client`.

```ts
import {
  ApiError,
  assertTelemetry,
  publishTelemetry,
  streamTelemetry,
  type Transport,
} from "@aquarium/api-client";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

export type StreamState = "idle" | "live" | "reconnecting" | "unauthorized";

export function useTelemetryStream(transport: Transport | null): StreamState {
  const queryClient = useQueryClient();
  const [state, setState] = useState<StreamState>("idle");

  useEffect(() => {
    if (!transport) {
      setState("idle");
      return;
    }

    const active = transport;
    const controller = new AbortController();
    let cancelled = false;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;

    async function run() {
      while (!cancelled) {
        try {
          for await (const raw of streamTelemetry(active, controller.signal)) {
            if (cancelled) return;
            setState("live");
            publishTelemetry(queryClient, active.device, assertTelemetry(raw));
          }
          if (cancelled) return;
          setState("reconnecting");
        } catch (error) {
          if (cancelled) return;

          if (error instanceof ApiError && error.kind === "unauthorized") {
            setState("unauthorized");
            return;
          }

          setState("reconnecting");
        }

        await new Promise<void>((resolve) => {
          retryTimer = setTimeout(resolve, 3_000);
        });
      }
    }

    void run();

    return () => {
      cancelled = true;
      controller.abort();
      if (retryTimer) clearTimeout(retryTimer);
    };
  }, [transport, queryClient]);

  return state;
}
```

An `unauthorized` stream stops retrying: hammering the device with a token it has
already rejected cannot succeed, and the screen tells the user to fix it in
settings.

- [ ] **Step 2: Write the telemetry screen**

`apps/mobile/app/index.tsx`:

```tsx
import { useTelemetry, type TankTelemetryDto } from "@aquarium/api-client";
import { Link } from "expo-router";
import { Text, View } from "react-native";
import { useDevice } from "../src/providers";
import { useTelemetryStream } from "../src/use-telemetry-stream";

const STREAM_LABEL: Record<string, string> = {
  idle: "not connected",
  live: "live",
  reconnecting: "reconnecting…",
  unauthorized: "token rejected — open settings",
};

export default function TelemetryScreen() {
  const { transport, loading } = useDevice();
  const streamState = useTelemetryStream(transport);

  if (loading) return <Centered>loading…</Centered>;

  if (!transport) {
    return (
      <Centered>
        <Text>no device configured</Text>
        <Link href="/settings">open settings</Link>
      </Centered>
    );
  }

  return <Snapshot label={STREAM_LABEL[streamState] ?? streamState} transport={transport} />;
}

function Snapshot({ label, transport }: { label: string; transport: NonNullable<ReturnType<typeof useDevice>["transport"]> }) {
  const { data, error, isPending } = useTelemetry(transport);

  if (isPending) return <Centered>fetching snapshot…</Centered>;
  if (error) return <Centered>{error.message}</Centered>;
  if (!data) return <Centered>no data</Centered>;

  return (
    <View style={{ padding: 16, gap: 10 }}>
      <Text style={{ opacity: 0.6 }}>{label}</Text>
      <Rows snapshot={data} />
      <Link href="/settings">settings</Link>
    </View>
  );
}

function Rows({ snapshot }: { snapshot: TankTelemetryDto }) {
  const rows: [string, string][] = [
    ["mode", snapshot.system_mode],
    ["tank", formatCelsius(snapshot.temperature.tank_celsius)],
    ["sump", formatCelsius(snapshot.temperature.sump_celsius)],
    ["target", formatCelsius(snapshot.temperature.target_celsius)],
    ["heater", snapshot.temperature.heater_active ? "on" : "off"],
    ["lighting", snapshot.lighting.profile],
    ["white / rgb", `${snapshot.lighting.channel_white_pct}% / ${snapshot.lighting.channel_rgb_pct}%`],
    ["override", snapshot.lighting.override_active ? "active" : "none"],
    ["leak", snapshot.leak_detected ? "DETECTED" : "none"],
    ["uptime", `${snapshot.system_health.uptime_seconds}s`],
    ["wifi", `${snapshot.system_health.wifi_rssi_dbm} dBm`],
    ["free heap", `${snapshot.system_health.free_heap_kb} kB`],
  ];

  return (
    <View style={{ gap: 6 }}>
      {rows.map(([label, value]) => (
        <View key={label} style={{ flexDirection: "row", justifyContent: "space-between" }}>
          <Text style={{ opacity: 0.6 }}>{label}</Text>
          <Text>{value}</Text>
        </View>
      ))}
    </View>
  );
}

function formatCelsius(value: number | null): string {
  return value === null ? "—" : `${value.toFixed(2)} °C`;
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 8 }}>
      {typeof children === "string" ? <Text>{children}</Text> : children}
    </View>
  );
}
```

A `null` temperature renders as `—`, not `0.00 °C`: the firmware sends `null` when
a DS18B20 reading fails validation, and showing zero degrees would read as a real
measurement.

- [ ] **Step 3: Verify types and lint**

```bash
cd apps/mobile && yarn typecheck 2>&1 | tail -10 && yarn lint 2>&1 | tail -5
```

Expected: no type errors, no lint errors. The `formatCelsius` body contains
`toFixed(2)` — if the type checker reports a syntax error there, fix the
transcription.

- [ ] **Step 4: Verify the bundler builds the app**

This is the first check that Metro resolves `@aquarium/api-client` through the
monorepo config from Task 9.

```bash
cd apps/mobile && yarn expo export --platform web --output-dir /tmp/aquarium-export 2>&1 | tail -15
```

Expected: `Exported` with no unresolved-module errors. A failure naming
`@aquarium/api-client` means `metro.config.js` did not take effect.

- [ ] **Step 5: Hardware acceptance**

Type-checking and bundling do not prove the stack works. With the firmware flashed
and on the same network:

```bash
cd $(git rev-parse --show-toplevel) && make smoke IP=<device-ip>
cd apps/mobile && yarn start
```

Then on a device or simulator: enter the host, port and the `API_TOKEN` from
`.env`, save, and confirm the telemetry screen shows `live` with values that
change. A wrong token must show `token rejected — open settings`, and a wrong
host must show the unreachable message.

- [ ] **Step 6: Checkpoint**

```bash
cd apps/mobile && yarn typecheck && yarn test 2>&1 | tail -3
cd $(git rev-parse --show-toplevel) && yarn workspaces foreach --all run test 2>&1 | tail -5
```

Expected: 8 app tests and 37 `api-client` tests passing.

---

## Task 12: CI, Makefile JS targets, and documentation

**Files:**
- Modify: `.github/workflows/rust_ci.yml`, `Makefile`, `.vscode/tasks.json`, `.devcontainer/Dockerfile`, `README.md`
- Create: `.github/workflows/js_ci.yml`

- [ ] **Step 1: Add the JS targets to the Makefile**

Append a new section before the `clean` section:

```make
# ==============================================================================
# Mobile client
# ==============================================================================

.PHONY: types
types: ## Regenerate the TypeScript contract from the Rust DTOs
	@$(MSG) "Generating TypeScript contract"
	@TS_RS_EXPORT_DIR=packages/api-client/src/generated \
		cargo test -p $(CORE_CRATE) --features ts-export >/dev/null
	@$(OK) "Contract written to packages/api-client/src/generated"

.PHONY: types-check
types-check: types ## Fail if the generated contract is out of date
	@$(MSG) "Checking contract drift"
	@git diff --exit-code -- packages/api-client/src/generated \
		|| { $(ERR) "generated contract is stale - run 'make types'"; exit 1; }
	@$(OK) "Contract up to date"

.PHONY: app-install
app-install: ## Install JavaScript dependencies
	@$(MSG) "yarn install"
	@yarn install --immutable

.PHONY: app-start
app-start: ## Start the Expo dev server
	@$(MSG) "Expo dev server"
	@yarn workspace mobile run start

.PHONY: js-lint
js-lint: ## Lint and type-check every yarn workspace
	@$(MSG) "Lint + typecheck (JavaScript)"
	@yarn lint
	@yarn typecheck

.PHONY: js-test
js-test: ## Run JavaScript unit tests
	@$(MSG) "JavaScript tests"
	@yarn test
```

Then replace the `ci` target:

```make
ci: fmt-check lint test types-check js-lint js-test ## Everything CI should gate on
	@$(OK) "CI passed"
```

- [ ] **Step 2: Verify the Makefile targets**

```bash
make types && make types-check && make js-lint && make js-test
```

Expected: `types-check` reports the contract is up to date, lint and tests pass.
If `types-check` fails right after `types`, the generated output is
nondeterministic — check whether `TS_RS_EXPORT_DIR` is picking up the directory
from Task 3 rather than a default `bindings/`.

- [ ] **Step 3: Scope the Rust workflow and fix its paths**

Edit `.github/workflows/rust_ci.yml`. Add path filters so an app-only change does
not spend minutes on the xtensa toolchain, and split the workspace-wide clippy,
which would now try to build the firmware for the host:

```yaml
on:
  push:
    branches:
      - main
    paths:
      - "crates/**"
      - "Cargo.toml"
      - "Cargo.lock"
      - "rust-toolchain.toml"
      - ".github/workflows/rust_ci.yml"
  pull_request:
    paths:
      - "crates/**"
      - "Cargo.toml"
      - "Cargo.lock"
      - "rust-toolchain.toml"
      - ".github/workflows/rust_ci.yml"
  workflow_dispatch:
```

Replace the matrix with explicit steps, since the three commands now need
different working directories:

```yaml
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4
      - name: Setup Rust
        uses: esp-rs/xtensa-toolchain@v1.6
        with:
          default: true
          buildtargets: esp32s3
          ldproxy: true
      - name: Enable caching
        uses: Swatinem/rust-cache@v2
      - name: Format
        run: cargo fmt --all -- --check --color always
      - name: Clippy (core, host)
        run: cargo clippy -p aquarium-core --all-targets -- -D warnings
      - name: Host tests
        run: cargo test -p aquarium-core
      - name: Contract is up to date
        run: |
          TS_RS_EXPORT_DIR=packages/api-client/src/generated \
            cargo test -p aquarium-core --features ts-export
          git diff --exit-code -- packages/api-client/src/generated
      - name: Clippy (firmware)
        working-directory: crates/firmware
        run: cargo clippy --release -- -D warnings
      - name: Build firmware
        working-directory: crates/firmware
        run: cargo build --release
```

The contract drift check lives here because this is the job that has a Rust
toolchain.

- [ ] **Step 4: Add the JavaScript workflow**

Create `.github/workflows/js_ci.yml`:

```yaml
name: JavaScript CI

on:
  push:
    branches:
      - main
    paths:
      - "apps/**"
      - "packages/**"
      - "package.json"
      - "yarn.lock"
      - ".yarnrc.yml"
      - ".github/workflows/js_ci.yml"
  pull_request:
    paths:
      - "apps/**"
      - "packages/**"
      - "package.json"
      - "yarn.lock"
      - ".yarnrc.yml"
      - ".github/workflows/js_ci.yml"
  workflow_dispatch:

jobs:
  js-checks:
    name: JavaScript Checks
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4
      - name: Setup Node
        uses: actions/setup-node@v4
        with:
          node-version: 22
      - name: Enable corepack
        run: corepack enable
      - name: Install
        run: yarn install --immutable
      - name: Lint
        run: yarn lint
      - name: Typecheck
        run: yarn typecheck
      - name: Test
        run: yarn test
```

Note: the firmware build inside `rust_ci.yml` is what proves the Task 1 move; the
contract check proves Task 3. Neither belongs in this workflow.

- [ ] **Step 5: Add Node to the devcontainer**

`.vscode/tasks.json` needs no change — verify this rather than assuming it:

```bash
grep -n 'cargo' .vscode/tasks.json || echo "confirmed: tasks delegate to make only"
```

Expected: the `confirmed` line. Every task runs `make <target>` with
`cwd: ${workspaceFolder}`, so the moved paths are already absorbed by Task 2.

`.devcontainer/Dockerfile` does need Node. Debian bookworm ships Node 18, which
Expo 58 does not support, so it comes from NodeSource. It must go in the root
stage — insert it immediately after the existing `apt-get install` block and
**before** the `# Set users` comment, because after `USER ${CONTAINER_USER}` the
image can no longer install packages:

```dockerfile
# Install Node for the mobile client workspace
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && corepack enable \
    && apt-get clean -y && rm -rf /var/lib/apt/lists/*
```

`curl` is already installed by the block above, and the `apt-get clean -y && rm -rf
/var/lib/apt/lists/*` tail matches the convention the existing block uses.

- [ ] **Step 6: Update the README**

The README documents setup and usage against the old layout. The sections to
touch, by their existing headings:

- **Project Structure** — replace the tree with the `crates/` / `apps/` /
  `packages/` layout from this plan's File Structure section
- **Installation → Requirements** — add Node 22 and corepack alongside the
  existing esp toolchain requirements
- **Installation → Other useful targets** — add `types`, `types-check`,
  `app-install`, `app-start`, `js-lint`, `js-test`
- **Development and Testing** — note that host tests no longer need a `--target`
  flag, and document the contract workflow: change a DTO, run `make types`,
  review the diff
- **HTTP API** — add a line that `packages/api-client` is the generated
  TypeScript view of this same contract
- a new **Mobile client** section after **HTTP API**: `make app-install`,
  `make app-start`, entering host / port / token in the settings screen, and that
  the token is the `API_TOKEN` from `.env`

Also grep for stale paths before declaring this done:

```bash
grep -n 'core/src\|^src/\|`src/\|(src/' README.md
```

Expected: every hit either already says `crates/...` or gets updated.

- [ ] **Step 7: Verify the whole gate**

```bash
make ci 2>&1 | tail -20
```

Expected: `CI passed`. This runs `fmt-check`, both clippy passes, host tests, the
contract drift check, JS lint and typecheck, and the JS tests. The firmware clippy
pass needs the esp environment, so run it in a shell that has sourced
`$HOME/export-esp.sh`.

- [ ] **Step 8: Checkpoint**

```bash
test -f .github/workflows/js_ci.yml || echo "FAIL: js workflow missing"
grep -q 'working-directory: crates/firmware' .github/workflows/rust_ci.yml || echo "FAIL: rust workflow not repointed"
grep -q 'types-check' Makefile || echo "FAIL: drift check missing"
git status --short | head -40
```

Expected: no `FAIL` lines. Review the `git status` output with the user — the work
is complete but uncommitted, which is intended.

---

## Verification Summary

| Claim | Command |
| --- | --- |
| Host tests run without a target override | `cargo test -p aquarium-core` |
| Firmware still builds | `cd crates/firmware && cargo build --release` |
| ts-rs is absent from the default build | `cargo tree -p aquarium-core \| grep -c ts-rs` → `0` |
| Generated contract matches the wire format | `cat packages/api-client/src/generated/LightingTarget.ts` |
| Contract is not stale | `make types-check` |
| `api-client` behaves | `yarn workspace @aquarium/api-client test` → 37 passing |
| Metro resolves the workspace package | `cd apps/mobile && yarn expo export --platform web` |
| The slice works on real hardware | `make app-start`, enter host + token, telemetry reads `live` |

---

## Execution notes

What the plan assumed and reality corrected, recorded so the next reader does not
re-derive it.

**`TS_RS_EXPORT_DIR` is relative to the crate, not the invocation.** Running the
export from the root with `packages/api-client/src/generated` wrote to
`crates/core/packages/api-client/src/generated`. `make types` therefore runs from
`crates/core` with `../../packages/api-client/src/generated`.

**ts-rs 12 enables `serde-compat` by default**, so it already honours
`#[serde(rename_all)]`. The planned `ts(rename_all)` mirror was removed after
confirming byte-identical output without it — keeping both would be two sources
of truth for one rename.

**`u64` maps to `bigint`.** `timestamp_epoch_s` and `uptime_seconds` generated as
`bigint`, but `serde_json` writes them as JSON numbers, so `JSON.parse` returns a
`number` and the type was lying. Both are pinned with `#[ts(type = "number")]`.

**The generated directory is gitignored**, reversing the plan's "committed, drift
checked by `git diff`". With no committed copy there is nothing to diff, so
`make types-check` regenerates and then type-checks the client against the
result — which catches breakage rather than merely change. `js_ci.yml` installs
stable Rust (`cargo +stable`, overriding the `esp` pin in `rust-toolchain.toml`)
to generate before it checks.

**Yarn needed an empty `yarn.lock` to claim the repo.** There is a `package.json`
in the user's home directory; without a lockfile here, yarn 4 treats this repo as
a stray subdirectory of that project and refuses to install. Corepack also
manages the yarn binary via `packageManager`, so there is no `.yarn/releases` to
commit.

**Root workspace scripts must exclude the root.** `yarn workspaces foreach --all`
includes the root workspace, whose `lint`/`typecheck`/`test` scripts call
`foreach` again — unbounded recursion. They pass `--exclude
aquarium-controller-monorepo`.

**`expo lint` does not work in this monorepo.** It installs eslint 9 and writes
`eslint.config.js`, then fails its own version check because it looks for eslint
in `apps/mobile/node_modules` rather than the hoisted root. The app's `lint`
script calls `eslint .` directly, which is what the error message recommends.

**The Expo 58 template uses `src/app/`, not `app/`,** and ships TypeScript 6.
`api-client` was aligned to TS 6 rather than holding 5.9, so the monorepo carries
one compiler. The template's demo components do not type-check under TS 6 strict
and became unreferenced after the screens were written, so they were deleted.

**`packages/api-client` could not import `@tanstack/react-query` in tests**
before the app brought React in. `queryKeys` moved to `query-keys.ts`, which has
no react-query import — the keys are pure data and the test now exercises them
without a React runtime.

**Vitest 5 needs `vite` declared explicitly**; it is a peer dependency, not a
transitive one.

**The design system was ported, not copied.** See the spec's *Component
architecture* section: the portfolio's atoms are DOM components built on
`className` and `.scss`, so the atoms were reimplemented on React Native
primitives with the same prop vocabulary, and `palette.scss` became
`components/utility/theme.ts`.

**Firmware acceptance found a silent regression that every compile-time check
passed.** The first flash after the move booted and panicked immediately with
`LoadProhibited`, registers full of `0xa5a5a5a5`. Root cause: the move split
three path references that used to coincide at the repo root.

1. `esp-idf-sys` could not find `sdkconfig.defaults`, so the build used stock
   ESP-IDF defaults — `CONFIG_ESP_MAIN_TASK_STACK_SIZE` 3584 instead of 131072,
   no PSRAM, 160 MHz, 10 lwIP sockets, no end-of-stack watchpoint, 2 MB flash.
   The main task could not survive boot. Fixed by uncommenting
   `CARGO_WORKSPACE_DIR = { value = "", relative = true }` in
   `crates/firmware/.cargo/config.toml`.
2. `CONFIG_PARTITION_TABLE_CUSTOM_FILENAME` resolves six levels above
   `esp-idf-sys`'s `OUT_DIR`, which is the workspace root — it still pointed at
   the old `partitions.csv` location. Repointed at
   `crates/firmware/partitions.csv`.
3. `espflash` reads `espflash.toml` from its working directory, and the Makefile
   ran it from the repo root. It silently derived a partition layout from flash
   size (`16,384,000` bytes instead of the real `3,145,728`). The flash, monitor
   and erase recipes now `cd $(FW_DIR)`.

Each fix was applied and verified one at a time, the generated `sdkconfig`
re-read after each. Diagnosis order mattered: fixing (1) made (2) fail loudly,
which is how (2) was found.

**Hardware acceptance now passes.** Flashed with `temperature: Off` (wifi + API
only, no probes wired): the device boots, joins Wi-Fi at 192.168.1.25,
`free_heap_kb` reports 8302 which confirms PSRAM is live, and `make smoke` passes
the whole probe — auth, telemetry, SSE, and the override lifecycle. The
`api-client` package was then run against the real device and passes 5/5.

Two observations recorded, neither caused by this work and neither fixed here:
`expires_in_s` is a countdown rather than an echo of `duration_s` (600 reads back
599), and the device reports `timestamp_epoch_s: 0` alongside
`uptime_seconds: 1793986123`, which looks like epoch seconds in the uptime field
while the timestamp stays unset.
