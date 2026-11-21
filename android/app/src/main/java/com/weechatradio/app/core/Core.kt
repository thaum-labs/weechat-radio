package com.weechatradio.app.core

import uniffi.wcr_mobile.PresetInfo

/**
 * The wcr core (Rust, android/core) as the app sees it.
 *
 * If the native library is missing (an x86 emulator, or a build that skipped
 * build-native.sh) every call answers "unknown" instead of crashing the app.
 */
object Core {
    private val loaded: Boolean =
        runCatching { uniffi.wcr_mobile.coreVersion() }.isSuccess

    /** e.g. "1.0.21"; null when the core is not loaded. */
    val version: String? get() = if (loaded) uniffi.wcr_mobile.coreVersion() else null

    fun presets(): List<PresetInfo> =
        if (loaded) uniffi.wcr_mobile.presets() else emptyList()

    /** "145.500" to "145.500 MHz (2m)". */
    fun describeFrequency(mhz: String): String? =
        if (loaded) uniffi.wcr_mobile.describeFrequency(mhz) else null

    /** A warning for APRS, PMR446 or out-of-band frequencies, if any. */
    fun frequencyWarning(mhz: String): String? =
        if (loaded) uniffi.wcr_mobile.frequencyWarning(mhz) else null

    fun airtimeSeconds(preset: String, bytes: Int): Double? =
        if (loaded) uniffi.wcr_mobile.airtimeSeconds(preset, bytes.toUInt()) else null

    fun gridFromLocation(lat: Double, lon: Double): String? =
        if (loaded) uniffi.wcr_mobile.gridFromLocation(lat, lon) else null
}
