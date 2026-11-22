# WeeChat Radio for Android

Early groundwork. The app runs against a **simulated radio** until Phase 0
(real Bluetooth audio and PTT to a VR-N76) is done on a real phone.

| Folder | What it is |
| --- | --- |
| `app/` | The app: Kotlin, Jetpack Compose, Material 3, built to the designs in `docs/design/android/material/`. Four tabs (Chat, Mail, Map, Radio), Settings behind the gear, setup in four steps. |
| `app/.../ui/theme/Theme.kt` | Material 3 palette seeded from #7d9bff, dark first plus light (follows the phone), Roboto Mono for callsigns and frequencies, and the fixed radio colours (TX orange, meter zones, waterfall). |
| `app/.../radio/` | `Radio` (what the screens need), `SimulatedRadio` (the fake one used for now, with the designs' sample data) and `ListeningService` (background listening notification). |
| `app/.../core/Core.kt` | The calls into the wcr core (bands, presets, airtime, grid). |
| `core/` | The wcr core as an Android library (Rust, UniFFI). Its own Cargo workspace, so desktop CI never builds it. |
| `build-native.sh` | Builds `libwcr_mobile.so`, its Kotlin bindings, and modem73 (as `libmodem73.so`) with the NDK. |

## Screenshots

`./gradlew testDebugUnitTest` draws every design screen, dark and light, into
`app/build/screens/` (CI uploads them as `android-screens`). The map shows a
plain grid there, since tests have no network for map tiles.

## Build

CI does it all: the `android` workflow uploads the APK on every run that
touches `android/` (Actions > android > the run > Artifacts).

Locally, with the Android SDK and NDK installed:

    rustup target add aarch64-linux-android
    cargo install cargo-ndk
    ANDROID_NDK_HOME=/path/to/ndk ./build-native.sh
    ./gradlew assembleDebug

The APK is `app/build/outputs/apk/debug/app-debug.apk`. It only contains
64-bit ARM code, so it runs on a phone, not on an x86 emulator (there the
app still opens, the core just reports "not loaded").

## Signing

Release builds are signed in CI from four repository secrets:
`ANDROID_KEYSTORE_BASE64` (the .jks file, base64), `ANDROID_KEYSTORE_PASSWORD`,
`ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`. Without them CI builds a debug APK.
Never commit the key. Losing it means existing installs cannot be updated.
