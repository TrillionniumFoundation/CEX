#!/usr/bin/env bash
set -euo pipefail

readonly EXPECTED_VERSION='1.98.1'
readonly EXPECTED_RUST_COMMIT='48a229ceaefd4985c50990b14116b6d856af0985'
readonly EXPECTED_CARGO_COMMIT='797e8a9bca276c1c9f9f738d2a20f484fa4eea9d'

for binary in rustc cargo rustfmt cargo-fmt clippy-driver cargo-clippy awk; do
  command -v "$binary" >/dev/null 2>&1 || {
    echo "required exact Rust toolchain component is unavailable: $binary" >&2
    exit 67
  }
done

rust_release="$(rustc -vV | awk -F': ' '$1 == "release" {print $2}')"
rust_commit="$(rustc -vV | awk -F': ' '$1 == "commit-hash" {print $2}')"
cargo_commit="$(cargo -Vv | awk -F': ' '$1 == "commit-hash" {print $2}')"
host="$(rustc -vV | awk -F': ' '$1 == "host" {print $2}')"

[[ "$rust_release" == "$EXPECTED_VERSION" ]] || {
  echo "installed rustc release mismatch: expected=$EXPECTED_VERSION actual=$rust_release" >&2
  exit 68
}
[[ "$rust_commit" == "$EXPECTED_RUST_COMMIT" ]] || {
  echo "installed rustc commit mismatch: expected=$EXPECTED_RUST_COMMIT actual=$rust_commit" >&2
  exit 68
}
[[ "$cargo_commit" == "$EXPECTED_CARGO_COMMIT" ]] || {
  echo "installed Cargo commit mismatch: expected=$EXPECTED_CARGO_COMMIT actual=$cargo_commit" >&2
  exit 68
}

printf '{"schema":"cex.rust-toolchain-runtime.v1","status":"installed_exact_toolchain_valid","rust_version":"%s","rust_commit":"%s","cargo_commit":"%s","host":"%s","production_authorization":"not_granted"}\n' \
  "$rust_release" "$rust_commit" "$cargo_commit" "$host"
