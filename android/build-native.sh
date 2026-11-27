#!/usr/bin/env bash
# Builds the native parts of the Android app. Run it before ./gradlew.
#
#   app/src/main/jniLibs/arm64-v8a/libwcr_mobile.so
#       the wcr core (Rust, android/core), which the Kotlin app calls
#   app/build/generated/uniffi/
#       the Kotlin bindings for those calls (UniFFI)
#   app/src/main/jniLibs/arm64-v8a/libmodem73.so
#       modem73 (C++), a program the app runs the same way the desktop app
#       does. Android only lets an app run programs from its native library
#       folder, hence the lib*.so name.
#
# Needs: ANDROID_NDK_HOME, git, cargo-ndk, and
#   rustup target add aarch64-linux-android
set -euo pipefail

ABI=arm64-v8a
TRIPLE=aarch64-linux-android
API=26                      # Android 8.0, the app's minSdk
UNIFFI_VERSION=0.32.2       # same as android/core/Cargo.toml
MODEM73_TAG=v2.5.0          # same as .github/workflows/modem73.yml

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
libs="$here/app/src/main/jniLibs/$ABI"
work="$here/.native-build"
: "${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME to the Android NDK folder}"
mkdir -p "$libs" "$work"

echo "== wcr core"
(cd "$here/core" && cargo ndk -t "$ABI" -P "$API" build --release)
cp "$here/core/target/$TRIPLE/release/libwcr_mobile.so" "$libs/"

echo "== Kotlin bindings"
if ! uniffi-bindgen --version 2>/dev/null | grep -q "$UNIFFI_VERSION"; then
  cargo install uniffi --version "$UNIFFI_VERSION" --features cli --bin uniffi-bindgen --locked
fi
rm -rf "$here/app/build/generated/uniffi"
uniffi-bindgen generate --library "$libs/libwcr_mobile.so" --language kotlin \
  --no-format --out-dir "$here/app/build/generated/uniffi"

echo "== modem73 $MODEM73_TAG"
src="$work/modem73-$MODEM73_TAG"
if [ ! -d "$src" ]; then
  git clone -q --depth 1 -b "$MODEM73_TAG" https://github.com/RFnexus/modem73 "$src"
  for p in wcr-features encode-before-busy-test keyed-bursts; do
    git -C "$src" apply "$repo/third_party/modem73/patches/$p.patch"
  done
fi
bin="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
cc="$bin/$TRIPLE$API-clang"
cxx="$bin/$TRIPLE$API-clang++"
inc="-I$src/deps/aicodix/dsp -I$src/deps/aicodix/code -I$src/deps/aicodix/modem"
# No terminal UI, CM108 or Hamlib on a phone: wcr drives modem73 over its
# control port and feeds it the radio's Bluetooth audio over TCP, as on desktop.
"$cc" -c -O2 -o "$work/miniaudio.o" "$src/deps/miniaudio.c"
"$cc" -c -O2 -o "$work/cJSON.o" "$src/deps/cJSON.c"
"$cxx" -std=c++17 -O3 -DMODEM73_VERSION="\"${MODEM73_TAG#v}-wcr\"" $inc \
  -o "$libs/libmodem73.so" "$src/kiss_tnc.cc" "$work/miniaudio.o" "$work/cJSON.o" \
  -static-libstdc++ -ldl -lm -llog
echo "== done: $(ls "$libs")"
