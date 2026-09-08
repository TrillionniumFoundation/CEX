#!/usr/bin/env bash
set -euo pipefail

readonly RUST_VERSION='1.98.1'
readonly DIST_DATE='2026-09-03'
readonly RUST_COMMIT='48a229ceaefd4985c50990b14116b6d856af0985'
readonly CARGO_COMMIT='797e8a9bca276c1c9f9f738d2a20f484fa4eea9d'
readonly X86_64_SHA256='5326b36c53de11d148c8f8dab6553a3d1006c2cfd32123683073fad3c302605b'
readonly AARCH64_SHA256='0b514a8cc1cbcd939bff0f151661fe58b6ea5c7a7f645a5098c69e32e8c1e0a2'
readonly PREFIX="${1:-/opt/cex-rust/${RUST_VERSION}}"

for binary in curl sha256sum tar awk grep mktemp; do
  command -v "$binary" >/dev/null 2>&1 || {
    echo "missing Rust toolchain installer dependency: $binary" >&2
    exit 69
  }
done

case "$(uname -m)" in
  x86_64 | amd64)
    target='x86_64-unknown-linux-gnu'
    expected_sha256="$X86_64_SHA256"
    ;;
  aarch64 | arm64)
    target='aarch64-unknown-linux-gnu'
    expected_sha256="$AARCH64_SHA256"
    ;;
  *)
    echo "unsupported Rust toolchain host architecture: $(uname -m)" >&2
    exit 65
    ;;
esac

archive="rust-${RUST_VERSION}-${target}.tar.xz"
url="https://static.rust-lang.org/dist/${DIST_DATE}/${archive}"
tmp="$(mktemp -d)"
cleanup() { rm -rf -- "$tmp"; }
trap cleanup EXIT

curl --fail --location --proto '=https' --tlsv1.2 \
  --retry 5 --retry-all-errors --connect-timeout 30 \
  --output "$tmp/$archive" "$url"
printf '%s  %s\n' "$expected_sha256" "$tmp/$archive" | sha256sum --check --strict -

tar -xJf "$tmp/$archive" -C "$tmp"
installer="$tmp/rust-${RUST_VERSION}-${target}/install.sh"
[[ -x "$installer" ]] || {
  echo "Rust standalone installer missing from verified archive" >&2
  exit 66
}

rm -rf -- "$PREFIX"
"$installer" \
  --prefix="$PREFIX" \
  --without=rust-docs \
  --disable-ldconfig

for binary in rustc cargo rustfmt cargo-fmt clippy-driver cargo-clippy; do
  [[ -x "$PREFIX/bin/$binary" ]] || {
    echo "verified Rust distribution lacks required component: $binary" >&2
    exit 67
  }
done

rust_release="$($PREFIX/bin/rustc -vV | awk -F': ' '$1 == "release" {print $2}')"
rust_commit="$($PREFIX/bin/rustc -vV | awk -F': ' '$1 == "commit-hash" {print $2}')"
cargo_commit="$($PREFIX/bin/cargo -Vv | awk -F': ' '$1 == "commit-hash" {print $2}')"
host="$($PREFIX/bin/rustc -vV | awk -F': ' '$1 == "host" {print $2}')"

[[ "$rust_release" == "$RUST_VERSION" ]] || {
  echo "Rust release mismatch: expected=$RUST_VERSION actual=$rust_release" >&2
  exit 68
}
[[ "$rust_commit" == "$RUST_COMMIT" ]] || {
  echo "rustc commit mismatch: expected=$RUST_COMMIT actual=$rust_commit" >&2
  exit 68
}
[[ "$cargo_commit" == "$CARGO_COMMIT" ]] || {
  echo "Cargo commit mismatch: expected=$CARGO_COMMIT actual=$cargo_commit" >&2
  exit 68
}
[[ "$host" == "$target" ]] || {
  echo "Rust host mismatch: expected=$target actual=$host" >&2
  exit 68
}

cat <<JSON
{
  "schema": "cex.rust-toolchain-install.v1",
  "source": "$url",
  "archive_sha256": "$expected_sha256",
  "host": "$host",
  "rust_release": "$rust_release",
  "rust_commit": "$rust_commit",
  "cargo_commit": "$cargo_commit",
  "prefix": "$PREFIX",
  "production_authorization": "not_granted"
}
JSON
