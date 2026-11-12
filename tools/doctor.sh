#!/usr/bin/env bash
# Checks that every tool the project needs is installed. Run through `make doctor`,
# which puts ~/.cargo/bin and the pinned Node version on PATH first.
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
failed=0

ok() { printf '  \033[32mok\033[0m    %-16s %s\n' "$1" "$2"; }
bad() { printf '  \033[31mmissing\033[0m %-14s %s\n' "$1" "$2"; failed=1; }

rust_want="$(sed -n 's/^channel = "\(.*\)"/\1/p' "$root/rust-toolchain.toml")"
if rust_have="$(cd "$root" && rustc --version 2>/dev/null)"; then
  case "$rust_have" in
    "rustc $rust_want "*) ok rust "$rust_have" ;;
    *) bad rust "want $rust_want, have: $rust_have" ;;
  esac
else
  bad rust "install rustup: https://rustup.rs"
fi

if (cd "$root" && rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown); then
  ok wasm32 "target installed"
else
  bad wasm32 "rustup target add wasm32-unknown-unknown"
fi

if v="$(cargo nextest --version 2>/dev/null | head -1)"; then ok cargo-nextest "$v"; else bad cargo-nextest "cargo install cargo-nextest --locked"; fi
if v="$(sqlx --version 2>/dev/null)"; then ok sqlx-cli "$v"; else bad sqlx-cli "cargo install sqlx-cli --locked --no-default-features --features rustls,postgres"; fi
if v="$(cargo audit --version 2>/dev/null)"; then ok cargo-audit "$v"; else bad cargo-audit "cargo install cargo-audit --locked"; fi

node_want="$(cat "$root/.nvmrc")"
if node_have="$(node --version 2>/dev/null)"; then
  if [ "${node_have#v}" = "$node_want" ]; then ok node "$node_have"; else bad node "want v$node_want (nvm install $node_want), have $node_have"; fi
else
  bad node "nvm install $node_want"
fi
if v="$(npm --version 2>/dev/null)"; then ok npm "$v"; else bad npm "comes with Node"; fi

if v="$(docker info --format '{{.ServerVersion}}' 2>/dev/null)"; then ok docker "engine $v"; else bad docker "install and start Docker"; fi
if v="$(docker compose version --short 2>/dev/null)"; then ok compose "$v"; else bad compose "Docker Compose v2"; fi
if v="$(python3 --version 2>/dev/null)"; then ok python3 "$v"; else bad python3 "needed by tools/catalog.py"; fi

if [ ! -d "$root/web/node_modules" ]; then
  printf '  note    web dependencies are not installed: make setup\n'
elif chromium="$(cd "$root/web" && node -e "process.stdout.write(require('playwright').chromium.executablePath())" 2>/dev/null)" && [ -x "$chromium" ]; then
  ok chromium "for component tests"
else
  bad chromium "make setup (installs it with Playwright)"
fi

# Dependency advisories (step 1.12b, plan §5.1 item 8): informational — they never fail doctor
# and are not part of `make check`. `cargo audit` reads Cargo.lock, `npm audit` the web lockfile;
# both need the network, so an unreachable registry shows as a note, not an error.
if cargo audit --version >/dev/null 2>&1; then
  if out="$(cd "$root" && cargo audit 2>&1)"; then
    ok advisories "cargo audit: no vulnerable Rust crates"
  else
    printf '  \033[33madvise\033[0m  %-14s cargo audit: vulnerable Rust crates\n' rust
    printf '%s\n' "$out" | grep -Ei '^error|vulnerabilit|^Crate:' | head -6
  fi
else
  printf '  note    cargo audit skipped: cargo-audit is not installed\n'
fi
if [ -f "$root/web/package-lock.json" ]; then
  if out="$(cd "$root/web" && npm audit --audit-level=high 2>&1)"; then
    summary="$(printf '%s\n' "$out" | grep -E '[0-9]+ vulnerabilities' | tail -1)"
    ok advisories "npm audit (level high): ${summary:-no vulnerabilities}"
  else
    summary="$(printf '%s\n' "$out" | grep -E '[0-9]+ vulnerabilities' | tail -1)"
    printf '  \033[33madvise\033[0m  %-14s npm audit: %s\n' web "${summary:-high-severity findings (cd web && npm audit)}"
    printf '%s\n' "$out" | grep -E '^Severity|^[a-z][a-zA-Z0-9_-]* +[<>=~^*]' | head -5
  fi
else
  printf '  note    npm audit skipped: web/package-lock.json is missing (make setup)\n'
fi

exit "$failed"
