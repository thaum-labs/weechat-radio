package com.weechatradio.app.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import com.weechatradio.app.R

// Material 3 tonal-spot scheme seeded from #7d9bff, the weechat-radio accent.
// Values from docs/design/android/src/android-material/src/palette.json.
private val Dark = darkColorScheme(
    primary = Color(0xFFB5C4FF), onPrimary = Color(0xFF1C2D61),
    primaryContainer = Color(0xFF344479), onPrimaryContainer = Color(0xFFDCE1FF),
    secondary = Color(0xFFC1C5DD), onSecondary = Color(0xFF2B3042),
    secondaryContainer = Color(0xFF414659), onSecondaryContainer = Color(0xFFDEE1F9),
    tertiary = Color(0xFFE3BADB), onTertiary = Color(0xFF432740),
    tertiaryContainer = Color(0xFF5B3D57), onTertiaryContainer = Color(0xFFFFD7F6),
    error = Color(0xFFFFB4AB), onError = Color(0xFF690005),
    errorContainer = Color(0xFF93000A), onErrorContainer = Color(0xFFFFDAD6),
    background = Color(0xFF121318), onBackground = Color(0xFFE3E1E9),
    surface = Color(0xFF121318), onSurface = Color(0xFFE3E1E9),
    surfaceVariant = Color(0xFF45464F), onSurfaceVariant = Color(0xFFC6C6D0),
    outline = Color(0xFF8F909A), outlineVariant = Color(0xFF45464F),
    surfaceContainerLowest = Color(0xFF0D0E13), surfaceContainerLow = Color(0xFF1A1B21),
    surfaceContainer = Color(0xFF1E1F25), surfaceContainerHigh = Color(0xFF292A2F),
    surfaceContainerHighest = Color(0xFF34343A),
    inverseSurface = Color(0xFFE3E1E9), inverseOnSurface = Color(0xFF2F3036),
    inversePrimary = Color(0xFF4C5C92),
)

private val Light = lightColorScheme(
    primary = Color(0xFF4C5C92), onPrimary = Color.White,
    primaryContainer = Color(0xFFDCE1FF), onPrimaryContainer = Color(0xFF344479),
    secondary = Color(0xFF595E72), onSecondary = Color.White,
    secondaryContainer = Color(0xFFDEE1F9), onSecondaryContainer = Color(0xFF414659),
    tertiary = Color(0xFF745470), onTertiary = Color.White,
    tertiaryContainer = Color(0xFFFFD7F6), onTertiaryContainer = Color(0xFF5B3D57),
    error = Color(0xFFBA1A1A), onError = Color.White,
    errorContainer = Color(0xFFFFDAD6), onErrorContainer = Color(0xFF93000A),
    background = Color(0xFFFAF8FF), onBackground = Color(0xFF1A1B21),
    surface = Color(0xFFFAF8FF), onSurface = Color(0xFF1A1B21),
    surfaceVariant = Color(0xFFE3E1E9), onSurfaceVariant = Color(0xFF45464F),
    outline = Color(0xFF767680), outlineVariant = Color(0xFFC6C6D0),
    surfaceContainerLowest = Color.White, surfaceContainerLow = Color(0xFFF4F3FA),
    surfaceContainer = Color(0xFFEEEDF4), surfaceContainerHigh = Color(0xFFE9E7EF),
    surfaceContainerHighest = Color(0xFFE3E1E9),
    inverseSurface = Color(0xFF2F3036), inverseOnSurface = Color(0xFFF1F0F7),
    inversePrimary = Color(0xFFB5C4FF),
)

/**
 * Radio meaning, fixed in both themes so TX, OK, warn and the waterfall stay
 * recognisable (design README "Radio semantics are kept as fixed custom roles").
 */
data class RadioColors(
    /** TX, own call, Tune radio, the on-air banner, unread mail dot. */
    val tx: Color,
    /** Text and icons on a filled [tx] button. */
    val onTx: Color,
    val txContainer: Color,
    val onTxContainer: Color,
    /** Delivered, linked, MAIL GW up. */
    val ok: Color,
    val okContainer: Color,
    /** Untested, marginal, battery, licence notes. */
    val warn: Color,
    val warnContainer: Color,
    val onWarnContainer: Color,
    /** Other stations' callsigns. */
    val otherCall: Color,
)

private val DarkRadio = RadioColors(
    tx = Color(0xFFFF8A50), onTx = Color(0xFF2A0D00), txContainer = Color(0xFF5D2C12), onTxContainer = Color(0xFFFFDBC9),
    ok = Color(0xFF78DC66), okContainer = Color(0xFF1B5220),
    warn = Color(0xFFFFD166), warnContainer = Color(0xFF574300), onWarnContainer = Color(0xFFFFE9B0),
    otherCall = Color(0xFF5FD3E0),
)

private val LightRadio = RadioColors(
    tx = Color(0xFFB3410A), onTx = Color.White, txContainer = Color(0xFFFFDBC9), onTxContainer = Color(0xFF5D2C12),
    ok = Color(0xFF1D7A22), okContainer = Color(0xFFC8F0BE),
    warn = Color(0xFF7A5900), warnContainer = Color(0xFFFFE9B0), onWarnContainer = Color(0xFF3D2E00),
    otherCall = Color(0xFF006A73),
)

/** The same in both themes, as on the desktop and the website. */
object ModeColors {
    val internet = Color(0xFF00E5FF)
    val internetRadio = Color(0xFF39FF14)
    val radio = Color(0xFFFFBF00)
    val radioPlus = Color(0xFFFF4DFF)
}

/** IN / OUT meter zones, same as desktop: green 0–66 %, purple 66–88 %, orange 88–100 %. */
object MeterColors {
    val green = Color(0xFF3FBF4A)
    val purple = Color(0xFF9B59D0)
    val orange = Color(0xFFFF7A3D)
}

/** Waterfall palette, same in light and dark; newest row at the top. */
object WaterfallColors {
    val quiet = Color(0xFF0B1230)
    val noise = Color(0xFF8C3CC8)
    val signal = Color(0xFFFFE14D)
    val yourTx = Color(0xFFFF7A3D)
}

val LocalRadioColors = staticCompositionLocalOf { DarkRadio }

/** Roboto Mono: only for callsigns, frequencies, grid squares, preset codes and mail addresses. */
val Mono = FontFamily(Font(R.font.roboto_mono))

@Composable
fun WeechatTheme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalRadioColors provides if (dark) DarkRadio else LightRadio) {
        // Roboto is Android's own UI font, so the default typography already uses it.
        MaterialTheme(colorScheme = if (dark) Dark else Light, content = content)
    }
}
