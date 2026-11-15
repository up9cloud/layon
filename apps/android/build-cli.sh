#!/bin/sh
# Builds the layon CLI for Android and places it where the app bundles it.
set -eu
cd "$(dirname "$0")/../.."
cargo ndk -t arm64-v8a -t x86_64 build --release -p layon-client --bin layon
for pair in arm64-v8a:aarch64-linux-android x86_64:x86_64-linux-android; do
    abi=${pair%%:*}
    dir=apps/android/app/src/main/jniLibs/$abi
    mkdir -p "$dir"
    cp "target/${pair#*:}/release/layon" "$dir/liblayon.so"
done
