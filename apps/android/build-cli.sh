#!/bin/sh
# Builds the layon binary for Android and places it where the app bundles it.
set -eu
cd "$(dirname "$0")/../.."
# Matches minSdk in app/build.gradle.kts; getifaddrs needs 24 or later.
cargo ndk --platform 26 -t arm64-v8a -t x86_64 build --release -p layon
for pair in arm64-v8a:aarch64-linux-android x86_64:x86_64-linux-android; do
    abi=${pair%%:*}
    dir=apps/android/app/src/main/jniLibs/$abi
    mkdir -p "$dir"
    cp "target/${pair#*:}/release/layon" "$dir/liblayon.so"
done
