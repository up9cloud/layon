#!/bin/sh
# Builds the Rust library for iOS and generates its Swift bindings into Generated/.
set -eu
cd "$(dirname "$0")/../.."
export IPHONEOS_DEPLOYMENT_TARGET=17.0
cargo build --release -p layon-ffi --target aarch64-apple-ios
# uniffi reads the interface from a host build of the same crate.
cargo build -p layon-ffi
out=apps/ios/Generated
rm -rf "$out"
mkdir -p "$out/include"
cargo run -q -p layon-ffi --features bindgen --bin uniffi-bindgen -- \
    generate --library target/debug/liblayon_ffi.dylib --language swift --out-dir "$out"
mv "$out/layon_ffiFFI.h" "$out/include/"
mv "$out/layon_ffiFFI.modulemap" "$out/include/module.modulemap"
cp target/aarch64-apple-ios/release/liblayon_ffi.a "$out/"
