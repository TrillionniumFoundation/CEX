#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: run-cex-exact-surface-v1.sh REPOSITORY SURFACE" >&2
  exit 64
fi
repository="$1"
surface="$2"
case "$surface" in
  head|prospective-merge) ;;
  *) echo "unsupported surface: $surface" >&2; exit 64 ;;
esac

test -d "$repository/.git"
cd "$repository"
test -z "$(git status --porcelain=v1 --untracked-files=no)"
printf 'CEX_SURFACE_BEGIN=%s\n' "$surface"
printf 'CEX_SURFACE_COMMIT=%s\n' "$(git rev-parse HEAD)"
printf 'CEX_SURFACE_TREE=%s\n' "$(git rev-parse 'HEAD^{tree}')"

git diff --check
semantic_check="${RUNNER_TEMP:-/tmp}/cex-${surface}-semantics.json"
python3 scripts/generate-repository-semantics.py --stdout > "$semantic_check"
cmp "$semantic_check" docs/repository-contract-semantics-v1.json
python3 scripts/check-sequence54-rustsec-admission.py
python3 scripts/check-rust-advisory-exceptions.py
python3 scripts/test-trnm-build-evidence.py
python3 scripts/check-consumer-projection-boundary.py
python3 scripts/test-consumer-projection-boundary.py
python3 scripts/test-consumer-route-contract.py

cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked

psql -h 127.0.0.1 -U postgres -d postgres -v ON_ERROR_STOP=1 \
  -c 'drop database if exists cex_exact_tuple_test with (force);'
psql -h 127.0.0.1 -U postgres -d postgres -v ON_ERROR_STOP=1 \
  -c 'create database cex_exact_tuple_test;'
DATABASE_URL='postgres://postgres:postgres@127.0.0.1:5432/cex_exact_tuple_test' \
TEST_DATABASE_URL='postgres://postgres:postgres@127.0.0.1:5432/cex_exact_tuple_test' \
  cargo test --workspace --all-targets --locked -- --test-threads=1

printf 'CEX_SURFACE_PASS=%s\n' "$surface"
printf 'PRODUCTION_AUTHORIZATION=not_granted\n'
