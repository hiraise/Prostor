# trackflow developer commands. `make` lists them.

NODE_VERSION := $(shell cat .nvmrc)
# Tools installed per user (rustup, nvm) without touching the shell profile.
export PATH := $(HOME)/.cargo/bin:$(HOME)/.nvm/versions/node/v$(NODE_VERSION)/bin:$(PATH)
# macOS ships GNU Make 3.81, which looks up recipe commands in the original PATH; passing it to the shell fixes that.
SHELL := env PATH=$(PATH) /bin/bash

COMPOSE := docker compose -f deploy/compose.dev.yaml
# The small server of the performance budget for `make bench`, and the 8 GB one of its stress run
# for `make bench-stress` (architecture.md §7, D-032).
BENCH_COMPOSE := docker compose -f deploy/compose.bench.yaml
STRESS_COMPOSE := docker compose -f deploy/compose.stress.yaml
NPM := npm --prefix web
# The dev database (deploy/compose.dev.yaml). sqlx checks every query against it while compiling;
# `make db-prepare` saves those checks to .sqlx/, so release builds need no database (D-015).
export DATABASE_URL ?= postgres://trackflow:trackflow@127.0.0.1:5433/trackflow
MIGRATIONS := crates/store/migrations
# A database of its own for the end-to-end tests, made fresh on every run.
E2E_DATABASE_URL := postgres://trackflow:trackflow@127.0.0.1:5433/trackflow_e2e
# Stages print one line each; the full output goes to target/logs/<name>.log (D-039). V=1 streams it.
QUIET := python3 tools/quiet.py
# `make bench` builds with the faster-to-build `perf` profile (D-039); BENCH_PROFILE=release for the real one.
BENCH_PROFILE ?= perf
# Loopback port of the benchmark's database; another project's container may hold the default.
BENCH_PORT ?= 5450

.DEFAULT_GOAL := help
.PHONY: help doctor setup dev dev-server dev-web web size build run check fmt test nextest clippy web-test wasm e2e e2e-run bench bench-stress bench-down api db-up db-down db-reset db-migrate db-prepare clean

help: ## List commands
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  make %-10s %s\n", $$1, $$2}'

doctor: ## Check that every tool is installed
	@tools/doctor.sh

setup: ## Install web dependencies and the browser for component tests (after a fresh clone)
	$(NPM) ci --no-audit --no-fund
	cd web && npx playwright install chromium

dev: db-migrate ## Run server and web app with live reload: http://localhost:5173
	@$(MAKE) --no-print-directory -j2 dev-server dev-web

# Account emails go to Mailpit (http://127.0.0.1:8025); uploaded files to data/files.
dev-server:
	TRACKFLOW_SMTP_URL=$${TRACKFLOW_SMTP_URL:-smtp://127.0.0.1:1025} \
	TRACKFLOW_MAIL_FROM="$${TRACKFLOW_MAIL_FROM:-trackflow <trackflow@localhost>}" \
	cargo run -p trackflow-server

dev-web: wasm
	$(NPM) run -s dev

# The query language for the browser (D-038), copied where the web app imports it (not committed).
WASM_OUT := target/wasm32-unknown-unknown/wasm/trackflow_query_wasm.wasm
wasm: ## Build the query language's WebAssembly module into web/src/lib/query
	@cargo build --quiet --locked -p trackflow-query-wasm --target wasm32-unknown-unknown --profile wasm
	@cp $(WASM_OUT) web/src/lib/query/query.wasm

web: wasm ## Build the web app into web/build
	$(NPM) run -s build

size: web ## Size of the web app's shell (gzip) against the budget (architecture.md §7)
	python3 tools/web_size.py

build: web ## Build the release binary with the web app inside: target/release/trackflow
	SQLX_OFFLINE=true cargo build --release --locked -p trackflow-server

run: build ## Run the release binary: http://127.0.0.1:7700
	./target/release/trackflow

check: db-migrate wasm ## Everything that must be green before a commit (needs Docker); log in target/logs/check.log
	@$(QUIET) --name check \
	  'fmt: cargo fmt --check' \
	  'clippy: cargo clippy --all-targets --locked' \
	  'rust tests: cargo nextest run --locked' \
	  'sqlx cache: cargo sqlx prepare --workspace --check -- --all-targets' \
	  'web lint: $(NPM) run -s lint' \
	  'web types: $(NPM) run -s typecheck' \
	  'web tests: $(NPM) run -s test' \
	  'api types: $(NPM) run -s api:check' \
	  'web build: $(NPM) run -s build' \
	  'shell size: python3 tools/web_size.py' \
	  'e2e: $(MAKE) --no-print-directory e2e-run'

fmt: ## Format all code
	cargo fmt
	$(NPM) run -s format

test: db-migrate wasm ## Run all tests
	@$(QUIET) --name test 'rust tests: cargo nextest run' 'web tests: $(NPM) run -s test'

nextest: db-migrate ## Rust tests, short output. ARGS="-p trackflow-service --test search qry_013"
	@$(QUIET) --name nextest 'rust tests: cargo nextest run --no-fail-fast $(ARGS)'

clippy: ## Lints, short output. ARGS="-p trackflow-query"
	@$(QUIET) --name clippy 'clippy: cargo clippy --all-targets --locked $(ARGS)'

web-test: wasm ## Web tests, short output. ARGS="src/lib/query"
	@$(QUIET) --name web-test 'web tests: $(NPM) run -s test -- $(ARGS)'

e2e: db-up wasm ## Run the end-to-end tests (Playwright) against a fresh database
	@$(QUIET) --name e2e \
	  'web build: $(NPM) run -s build' \
	  'shell size: python3 tools/web_size.py' \
	  'e2e: $(MAKE) --no-print-directory e2e-run'

# The server binary reads the built web app from web/build; `check` and `e2e` build it first.
e2e-run:
	cargo build --locked -p trackflow-server
	sqlx database drop -y --database-url $(E2E_DATABASE_URL)
	sqlx database create --database-url $(E2E_DATABASE_URL)
	rm -rf web/.playwright/files
	cd web && TRACKFLOW_E2E_DATABASE_URL=$(E2E_DATABASE_URL) npx playwright test

bench: ## Measure the performance budget on the target dataset (100k issues), then stop its database. Options: BENCH_ARGS=--help, BENCH_KEEP=1, BENCH_PROFILE=release, BENCH_PORT
	@TRACKFLOW_BENCH_PORT=$(BENCH_PORT) $(BENCH_COMPOSE) --progress quiet up -d --wait
	@set -o pipefail; SQLX_OFFLINE=true TRACKFLOW_BENCH_DATABASE_URL=postgres://trackflow:trackflow@127.0.0.1:$(BENCH_PORT)/trackflow_bench cargo run --profile $(BENCH_PROFILE) --locked -p trackflow-bench -- $(BENCH_ARGS) 2>&1 | $(QUIET) --filter; \
	status=$$?; [ -n "$(BENCH_KEEP)" ] || $(BENCH_COMPOSE) --progress quiet down; exit $$status

bench-stress: ## Stress run on 1M issues, at the end of a phase (D-032); the first run generates them (about six minutes)
	@$(STRESS_COMPOSE) --progress quiet up -d --wait
	@set -o pipefail; SQLX_OFFLINE=true TRACKFLOW_BENCH_DATABASE_URL=postgres://trackflow:trackflow@127.0.0.1:5435/trackflow_stress \
	cargo run --release --locked -p trackflow-bench -- --scale 1 $(BENCH_ARGS) 2>&1 | $(QUIET) --filter; \
	status=$$?; [ -n "$(BENCH_KEEP)" ] || $(STRESS_COMPOSE) --progress quiet down; exit $$status

bench-down: ## Stop the benchmark databases, keep their datasets
	$(BENCH_COMPOSE) down
	$(STRESS_COMPOSE) down

api: db-migrate ## Regenerate the API document and the web app's TypeScript types from the server
	@$(QUIET) --name api \
	  'api document: TRACKFLOW_UPDATE_API=1 cargo nextest run -p trackflow-server --test openapi' \
	  'api types: $(NPM) run -s api'

db-up: ## Start PostgreSQL (port 5433) and Mailpit (http://127.0.0.1:8025)
	@$(COMPOSE) --progress quiet up -d --wait

db-down: ## Stop PostgreSQL and Mailpit, keep their data
	$(COMPOSE) down

db-reset: ## Stop PostgreSQL and Mailpit and delete their data (also test databases)
	$(COMPOSE) down -v

db-migrate: db-up ## Apply migrations to the dev database
	@sqlx migrate run --source $(MIGRATIONS)

db-prepare: db-migrate ## Save compile-time query checks to .sqlx/ (after changing SQL)
	@$(QUIET) --name db-prepare 'sqlx cache: cargo sqlx prepare --workspace -- --all-targets'

clean: ## Remove build outputs
	cargo clean
	rm -rf web/build web/.svelte-kit web/.vitest
