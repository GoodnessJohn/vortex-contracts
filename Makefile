# Vortex Contracts — Makefile
#
# Wraps the commands documented in README.md so contributors never need to
# copy-paste raw multi-flag invocations.  All targets run inside the
# intent_settlement workspace directory.
#
# Usage:
#   make          — print help
#   make all      — fmt + lint + test + build (full pre-push check)
#   make fmt      — format all source files
#   make lint     — clippy with -D warnings (identical to CI)
#   make test     — run unit / integration tests
#   make build    — compile the release wasm binary
#   make deploy-testnet — deploy to Stellar testnet (requires STELLAR_SOURCE)

.DEFAULT_GOAL := help

# ── Configuration ─────────────────────────────────────────────────────────────

WORKSPACE       := intent_settlement
WASM_TARGET     := wasm32-unknown-unknown
WASM_OUT        := $(WORKSPACE)/target/$(WASM_TARGET)/release/vortex_intent_settlement.wasm
STELLAR_NETWORK ?= testnet
# Set STELLAR_SOURCE to your secret key or key-name before running deploy-testnet
STELLAR_SOURCE  ?=

# Crates that carry Kani harnesses and/or property-based tests (issue #544).
KANI_CRATES     := intent_settlement solver_registry proof_registry

# ── Compound targets ──────────────────────────────────────────────────────────

.PHONY: all
all: fmt lint test build  ## Run fmt, lint, test, and build (full pre-push check)

# ── Individual targets ────────────────────────────────────────────────────────

.PHONY: fmt
fmt:  ## Format all source files with rustfmt
	cd $(WORKSPACE) && cargo fmt --all

.PHONY: fmt-check
fmt-check:  ## Check formatting without modifying files (used by CI)
	cd $(WORKSPACE) && cargo fmt --all -- --check

.PHONY: lint
lint:  ## Run clippy with -D warnings (identical to the CI command)
	cd $(WORKSPACE) && cargo clippy --all-targets -- -D warnings

.PHONY: test
test:  ## Run the full test suite
	cd $(WORKSPACE) && cargo test

.PHONY: build
build:  ## Build the release wasm binary
	cd $(WORKSPACE) && cargo build --target $(WASM_TARGET) --release

.PHONY: audit
audit:  ## Run cargo-audit against the dependency tree
	cd $(WORKSPACE) && cargo audit

.PHONY: clean
clean:  ## Remove build artefacts
	cd $(WORKSPACE) && cargo clean

# ── Deploy ────────────────────────────────────────────────────────────────────

.PHONY: deploy-testnet
deploy-testnet: build  ## Deploy the wasm to Stellar testnet (set STELLAR_SOURCE first)
ifndef STELLAR_SOURCE
	$(error STELLAR_SOURCE is not set. Export your Stellar secret key or key-name: export STELLAR_SOURCE=<SECRET_KEY>)
endif
	stellar contract deploy \
		--wasm $(WASM_OUT) \
		--source $(STELLAR_SOURCE) \
		--network $(STELLAR_NETWORK)

# ── Local integration testing ────────────────────────────────────────────────

.PHONY: local-network-start
local-network-start:  ## Start local Soroban test network via docker-compose
	./setup-local-network.sh start

.PHONY: local-network-stop
local-network-stop:  ## Stop local Soroban test network
	./setup-local-network.sh stop

.PHONY: local-network-clean
local-network-clean:  ## Clean and remove local network volumes (destructive)
	./setup-local-network.sh clean

.PHONY: local-network-logs
local-network-logs:  ## Tail local network logs
	./setup-local-network.sh logs

.PHONY: local-network-status
local-network-status:  ## Check local network status
	./setup-local-network.sh status

# ── Kani model checking (issues #415, #544) ─────────────────────────────────

.PHONY: kani
kani:  ## Run Kani proof harnesses for pure arithmetic helpers (requires kani-verifier)
	@command -v cargo-kani >/dev/null 2>&1 || { \
		echo "Kani not found. Install with: cargo install --locked kani-verifier && cargo kani setup"; \
		exit 1; \
	}
	@set -e; \
	for crate in $(KANI_CRATES); do \
		echo "==> cargo kani in $$crate"; \
		case $$crate in \
			intent_settlement) \
				harnesses=" \
					--harness kani_slash_never_negative \
					--harness kani_slash_never_exceeds_bond \
					--harness kani_slash_floor_one \
					--harness kani_tier_fill_window_monotone \
					--harness kani_tier_fill_window_no_overflow \
					--harness kani_fee_never_exceeds_amount \
					--harness kani_fee_rounding_direction \
					--harness kani_dutch_decay_bounds \
					--harness kani_discount_in_range \
					--harness kani_decode_payload_roundtrip \
					--harness kani_slash_bps_monotone \
					--harness kani_decode_i128_be_bounds_safe" ;; \
			solver_registry) \
				harnesses=" \
					--harness kani_slash_bps_never_negative \
					--harness kani_slash_bps_never_exceeds_bond \
					--harness kani_slash_bps_monotone \
					--harness kani_tier_threshold_monotone \
					--harness kani_tier_lookup_in_range \
					--harness kani_bond_never_negative" ;; \
			proof_registry) \
				harnesses=" \
					--harness kani_slash_percentage_never_negative \
					--harness kani_slash_percentage_never_exceeds_bond \
					--harness kani_slash_percentage_monotone \
					--harness kani_bond_never_negative \
					--harness kani_bond_addition_no_overflow" ;; \
			*) \
				harnesses="" ;; \
		esac; \
		( cd $$crate && cargo kani $$harnesses ); \
	done

# ── Property-based tests (issue #544) ───────────────────────────────────────

.PHONY: proptest
proptest:  ## Run property-based tests across all Kani-covered crates
	@set -e; \
	for crate in $(KANI_CRATES); do \
		echo "==> cargo test --features testutils in $$crate"; \
		( cd $$crate && cargo test --features testutils ); \
	done

# ── Help ──────────────────────────────────────────────────────────────────────

.PHONY: help
help:  ## Show this help message
	@echo "Usage: make <target>"
	@echo ""
	@echo "Targets:"
	@grep -E '^[a-zA-Z_-]+:.*##' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*##"}; {printf "  %-20s %s\n", $$1, $$2}'
	@echo ""
	@echo "Kani-covered crates (KANI_CRATES): $(KANI_CRATES)"