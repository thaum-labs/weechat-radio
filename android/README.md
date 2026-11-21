# WeeChat Radio for Android

Early groundwork. The app runs against a **simulated radio** until Phase 0
(real Bluetooth audio and PTT to a VR-N76) is done on a real phone.

| Folder | What it is |
| --- | --- |
| `app/` | The app: Kotlin, Jetpack Compose, Material 3. Three tabs (Chats, Mail, Radio), Settings behind the gear on Radio. |
| `app/.../ui/theme/Theme.kt` | Both looks: **Standard** (website colours, light and dark follow the phone) and **modem73** (the desktop modem73 look). |
| `app/.../radio/` | `Radio` (what the screens need) and `SimulatedRadio` (the fake one used for now). |
| `app/.../core/Core.kt` | The calls into the wcr core (bands, presets, airtime, grid). |
| `core/` | The wcr core as an Android library (Rust, UniFFI). Its own Cargo workspace, so desktop CI never builds it. |
| `build-native.sh` | Builds `libwcr_mobile.so`, its Kotlin bindings, and modem73 (as `libmodem73.so`) with the NDK. |

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
