#!/usr/bin/env bash
# Packs the release archive from target/release (used by the Release workflow;
# install-test.sh installs from it like self-update does).
#
#   scripts/package.sh <version> [outdir=dist]
set -euo pipefail

cd "$(dirname "$0")/.."
version="${1:?version}"
out="$(realpath -m "${2:-dist}")"
name="vela-$version-$(uname -m)-linux"
# Every binary the crate builds: a hand-kept list missed vela-pulse once.
bins=(vela $(basename -s .rs src/bin/*.rs))

rm -rf "${out:?}/$name"
mkdir -p "$out/$name/target/release"
for b in "${bins[@]}"; do
    cp "target/release/$b" "$out/$name/target/release/"
done
cp -r contrib data shell install.sh uninstall.sh README.md LICENSE Cargo.toml Cargo.lock "$out/$name/"
tar -C "$out" -czf "$out/$name.tar.gz" "$name"
echo "$out/$name.tar.gz"
