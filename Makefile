# omariglol — build entry point.
#
# `make` (or `make build`) compiles the workspace in release mode and copies the
# resulting executables into ./bin, so they can be run or packaged without
# digging through target/. `make debug` does the same for a debug build, and
# `make app` builds the Tauri desktop app.
#
# Run `make help` for the full target list.

CARGO   ?= cargo
BIN_DIR ?= bin

# Workspace executables published into $(BIN_DIR). Add new CLI binaries here.
BINARIES := riglol

# The Tauri desktop app is a separate package (excluded from the workspace), so
# it builds into its own target directory.
APP_BIN := src-tauri/target/release/omariglol

.PHONY: all build debug app test lint fmt check clean help

all: build

## build: compile the workspace (release) and publish binaries to bin/
build:
	$(CARGO) build --release --locked --workspace
	@mkdir -p $(BIN_DIR)
	@for b in $(BINARIES); do \
		src="target/release/$$b"; \
		[ -x "$$src" ] || { echo "missing $$src" >&2; exit 1; }; \
		cp -f "$$src" "$(BIN_DIR)/$$b"; \
		echo "  $(BIN_DIR)/$$b"; \
	done

## debug: compile the workspace (debug) and publish binaries to bin/
debug:
	$(CARGO) build --locked --workspace
	@mkdir -p $(BIN_DIR)
	@for b in $(BINARIES); do \
		src="target/debug/$$b"; \
		[ -x "$$src" ] || { echo "missing $$src" >&2; exit 1; }; \
		cp -f "$$src" "$(BIN_DIR)/$$b"; \
		echo "  $(BIN_DIR)/$$b"; \
	done

## app: build the Tauri desktop app and publish it to bin/
app:
	npm run tauri build
	@mkdir -p $(BIN_DIR)
	@[ -x "$(APP_BIN)" ] || { echo "missing $(APP_BIN)" >&2; exit 1; }
	@cp -f "$(APP_BIN)" "$(BIN_DIR)/omariglol"
	@echo "  $(BIN_DIR)/omariglol"

## test: run the workspace test suite
test:
	$(CARGO) test --workspace

## lint: run clippy with warnings denied
lint:
	$(CARGO) clippy --all-targets -- -D warnings

## fmt: format the workspace
fmt:
	$(CARGO) fmt --all

## check: fmt check, clippy and tests
check:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --all-targets -- -D warnings
	$(CARGO) test --workspace

## clean: remove build artefacts and bin/
clean:
	$(CARGO) clean
	rm -rf $(BIN_DIR)

## help: list the available targets
help:
	@printf 'omariglol build targets:\n'
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/^## /  /'
