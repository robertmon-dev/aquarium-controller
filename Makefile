# ==============================================================================
# Aquarium Controller - ESP32-S3 firmware
# ==============================================================================

TARGET        := xtensa-esp32s3-espidf
HOST_TARGET   ?= x86_64-unknown-linux-gnu
CORE_CRATE    := aquarium-core
BIN           := aquarium-controller
CHIP          ?= esp32s3

ELF_RELEASE   := target/$(TARGET)/release/$(BIN)
ELF_DEBUG     := target/$(TARGET)/debug/$(BIN)

-include .env
export

# ------------------------------------------------------------------------------
# Pacman-style output
# ------------------------------------------------------------------------------
BOLD   := \033[1m
RED    := \033[31m
GREEN  := \033[32m
YELLOW := \033[33m
BLUE   := \033[34m
CYAN   := \033[36m
RESET  := \033[0m

MSG  := printf '$(BOLD)$(BLUE)==>$(RESET) $(BOLD)%s$(RESET)\n'
SUB  := printf '$(BOLD)$(CYAN)  ->$(RESET) %s\n'
OK   := printf '$(BOLD)$(GREEN)==>$(RESET) $(BOLD)%s$(RESET)\n'
WARN := printf '$(BOLD)$(YELLOW)==> WARNING:$(RESET) %s\n'
ERR  := printf '$(BOLD)$(RED)==> ERROR:$(RESET) %s\n'

ESP_ENV = command -v ldproxy >/dev/null 2>&1 || . $$HOME/export-esp.sh >/dev/null 2>&1;

.DEFAULT_GOAL := help

# ==============================================================================
# Utility
# ==============================================================================

.PHONY: help
help: ## Display this help message
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "$(CYAN)%-24s$(RESET) %s\n", $$1, $$2}'

# ==============================================================================
# Environment
# ==============================================================================

.PHONY: check-env
check-env: ## Verify build-time secrets are present
	@test -n "$(WIFI_SSID)" || { $(ERR) "WIFI_SSID is missing - fill in .env (make env-example)"; exit 1; }
	@test -n "$(WIFI_PASS)" || { $(ERR) "WIFI_PASS is missing - fill in .env (make env-example)"; exit 1; }
	@test -n "$(API_TOKEN)" || { $(ERR) "API_TOKEN is missing - fill in .env (make token)"; exit 1; }
	@$(SUB) "secrets present (values are never printed)"

.PHONY: env-example
env-example: ## Write a .env skeleton with a fresh API token
	@test ! -f .env || { $(ERR) ".env already exists - refusing to overwrite"; exit 1; }
	@$(MSG) "Creating .env"
	@printf 'WIFI_SSID=\nWIFI_PASS=\nAPI_TOKEN=%s\n' "$$(head -c 24 /dev/urandom | base64 | tr -d '/+=')" > .env
	@$(SUB) "fill in WIFI_SSID and WIFI_PASS"
	@grep -qxF '.env' .gitignore 2>/dev/null && $(SUB) ".env is gitignored" || $(WARN) "add .env to .gitignore"

.PHONY: token
token: ## Print a fresh URL-safe API token
	@head -c 24 /dev/urandom | base64 | tr -d '/+='

# ==============================================================================
# Build
# ==============================================================================

.PHONY: build
build: check-env ## Build firmware (release)
	@$(MSG) "Building firmware (release)"
	@$(ESP_ENV) cargo build --release
	@$(OK) "Built $(ELF_RELEASE)"

.PHONY: build-debug
build-debug: check-env ## Build firmware (debug)
	@$(MSG) "Building firmware (debug)"
	@$(ESP_ENV) cargo build
	@$(OK) "Built $(ELF_DEBUG)"

.PHONY: check
check: check-env ## Type-check firmware without linking
	@$(MSG) "Checking firmware"
	@$(ESP_ENV) cargo check --release

# ==============================================================================
# Flash & Monitor
# ==============================================================================

.PHONY: flash
flash: build ## Flash release firmware and attach the monitor
	@$(MSG) "Flashing $(CHIP)"
	@$(ESP_ENV) espflash flash --chip $(CHIP) --monitor $(ELF_RELEASE)

.PHONY: flash-debug
flash-debug: build-debug ## Flash debug firmware and attach the monitor
	@$(MSG) "Flashing $(CHIP) (debug)"
	@$(ESP_ENV) espflash flash --chip $(CHIP) --monitor $(ELF_DEBUG)

.PHONY: monitor
monitor: ## Attach the serial monitor without flashing
	@$(MSG) "Serial monitor (Ctrl-C to quit)"
	@$(ESP_ENV) espflash monitor

.PHONY: erase
erase: ## Erase the whole flash, including NVS (WiFi credentials)
	@$(WARN) "this also wipes NVS - stored WiFi credentials will be lost"
	@printf 'Continue? [y/N] '; read ans; [ "$$ans" = "y" ] || exit 1
	@$(ESP_ENV) espflash erase-flash

# ==============================================================================
# Tests
# ==============================================================================

.PHONY: test
test: ## Run host tests for aquarium-core
	@$(MSG) "Host tests ($(CORE_CRATE))"
	@cargo test -p $(CORE_CRATE) --target $(HOST_TARGET)

.PHONY: test-watch
test-watch: ## Re-run host tests on file changes (needs cargo-watch)
	@command -v cargo-watch >/dev/null 2>&1 || { $(ERR) "cargo-watch is missing: cargo install cargo-watch"; exit 1; }
	@cargo watch -x 'test -p $(CORE_CRATE) --target $(HOST_TARGET)'

# ==============================================================================
# Quality
# ==============================================================================

.PHONY: fmt
fmt: ## Format the whole workspace
	@$(MSG) "Formatting"
	@cargo fmt --all

.PHONY: fmt-check
fmt-check: ## Verify formatting without writing
	@$(MSG) "Checking formatting"
	@cargo fmt --all -- --check

.PHONY: lint
lint: lint-core lint-firmware ## Run clippy on core and firmware
	@$(OK) "Lint complete"

.PHONY: lint-core
lint-core: ## Run clippy on aquarium-core (host target)
	@$(MSG) "Clippy: $(CORE_CRATE)"
	@cargo clippy -p $(CORE_CRATE) --target $(HOST_TARGET) --all-targets -- -D warnings

.PHONY: lint-firmware
lint-firmware: check-env ## Run clippy on the firmware crate
	@$(MSG) "Clippy: firmware"
	@$(ESP_ENV) cargo clippy --release -- -D warnings

.PHONY: ci
ci: fmt-check lint test ## Everything CI should gate on
	@$(OK) "CI passed"

# ==============================================================================
# Firmware footprint
# ==============================================================================

.PHONY: size
size: ## Report real flash image size against the factory partition
	@test -f $(ELF_RELEASE) || { $(ERR) "$(ELF_RELEASE) is missing - run make build first"; exit 1; }
	@$(MSG) "Flash footprint"
	@sz=$$(find $$HOME/.espressif/tools -name 'xtensa-esp32s3-elf-size' 2>/dev/null | head -1); \
	test -n "$$sz" || { $(ERR) "xtensa-esp32s3-elf-size not found"; exit 1; }; \
	img=$$("$$sz" $(ELF_RELEASE) | awk 'NR==2 {print $$1 + $$2}'); \
	part=$$(awk -F',' '/^factory/ { gsub(/ /, "", $$5); print strtonum($$5) }' partitions.csv); \
	awk -v i="$$img" -v p="$$part" 'BEGIN { \
		printf "  image   %10.2f MiB\n", i/1048576; \
		printf "  factory %10.2f MiB\n", p/1048576; \
		printf "  used    %9.1f%%  (free %.2f MiB)\n", 100*i/p, (p-i)/1048576; \
		if (i > p) { printf "  IMAGE DOES NOT FIT THE PARTITION\n"; exit 1 } \
		else if (i > 0.85*p) { printf "  less than 15%% headroom left\n" } }'

.PHONY: partitions
partitions: ## Dump the partition table ESP-IDF actually generated
	@$(MSG) "Partition table from the build"
	@bin=$$(find target/$(TARGET)/release/build -path '*partition_table*' -name 'partition-table.bin' 2>/dev/null | head -1); \
	test -n "$$bin" || { $(ERR) "partition-table.bin is missing - run make build first"; exit 1; }; \
	python3 -c "import struct,sys; \
	d=open(sys.argv[1],'rb').read(); \
	t={0:'app',1:'data'}; \
	[print('  {:10} {:5} {:>9} {:>9}  {:6.2f} MiB'.format(e[12:28].rstrip(b'\0').decode(), t.get(e[2],e[2]), hex(struct.unpack('<I',e[4:8])[0]), hex(struct.unpack('<I',e[8:12])[0]), struct.unpack('<I',e[8:12])[0]/1048576)) for e in (d[i:i+32] for i in range(0,len(d),32)) if e[:2]==b'\xaaP']" "$$bin"
	@$(SUB) "espflash writes the table from partitions.csv - these two must agree"

# ==============================================================================
# Device smoke test
# ==============================================================================

.PHONY: smoke
smoke: ## Probe a flashed device over HTTP (make smoke IP=192.168.1.50)
	@test -n "$(IP)" || { $(ERR) "pass an address: make smoke IP=192.168.1.50"; exit 1; }
	@test -n "$(API_TOKEN)" || { $(ERR) "API_TOKEN is missing from .env"; exit 1; }
	@$(MSG) "Smoke test against $(IP)"
	@$(SUB) "no token -> expecting 401"
	@code=$$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 http://$(IP)/api/telemetry); \
	[ "$$code" = "401" ] || { $(ERR) "got $$code instead of 401"; exit 1; }
	@$(SUB) "wrong token -> expecting 401"
	@code=$$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 -H 'Authorization: Bearer wrong' http://$(IP)/api/telemetry); \
	[ "$$code" = "401" ] || { $(ERR) "got $$code instead of 401"; exit 1; }
	@$(SUB) "query token outside /api/events -> expecting 401"
	@code=$$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 "http://$(IP)/api/telemetry?token=$(API_TOKEN)"); \
	[ "$$code" = "401" ] || { $(ERR) "the query-token fallback leaks beyond /api/events: $$code"; exit 1; }
	@$(SUB) "valid token -> expecting 200 and a snapshot"
	@curl -s --max-time 5 -H "Authorization: Bearer $(API_TOKEN)" http://$(IP)/api/telemetry | python3 -m json.tool
	@$(SUB) "SSE -> expecting event:telemetry"
	@curl -sN --max-time 5 "http://$(IP)/api/events?token=$(API_TOKEN)" | head -c 400; echo
	@$(OK) "Smoke test passed"
	@$(WARN) "still manual: secrets in log, slot limit, sensor hot-unplug, 15-min heap drift"

.PHONY: telemetry
telemetry: ## Pretty-print one telemetry snapshot (make telemetry IP=...)
	@test -n "$(IP)" || { $(ERR) "pass an address: make telemetry IP=192.168.1.50"; exit 1; }
	@curl -s --max-time 5 -H "Authorization: Bearer $(API_TOKEN)" http://$(IP)/api/telemetry | python3 -m json.tool

.PHONY: events
events: ## Follow the SSE stream (make events IP=...)
	@test -n "$(IP)" || { $(ERR) "pass an address: make events IP=192.168.1.50"; exit 1; }
	@$(MSG) "SSE stream from $(IP) (Ctrl-C to quit)"
	@curl -sN "http://$(IP)/api/events?token=$(API_TOKEN)"

# ==============================================================================
# Clean
# ==============================================================================

.PHONY: clean
clean: ## Remove cargo build artifacts
	@$(MSG) "Cleaning artifacts"
	@cargo clean
	@$(OK) "Clean complete"

.PHONY: clean-idf
clean-idf: ## Also drop the ESP-IDF build tree (forces a full reconfigure)
	@$(WARN) "the next build will rebuild the whole ESP-IDF - several minutes"
	@$(MSG) "Dropping the ESP-IDF build tree"
	@rm -rf target/$(TARGET)/*/build/esp-idf-sys-*
	@$(OK) "Clean complete"
