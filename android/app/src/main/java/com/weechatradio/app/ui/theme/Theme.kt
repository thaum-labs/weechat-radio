package com.weechatradio.app.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily

/** Settings > Look > Theme. One layout, two looks. */
enum class ThemeChoice(val label: String) { Standard("Standard"), Modem73("modem73") }

/**
 * Colours Material has no slot for: "Good" levels, warnings and the "Waiting"
 * tag, the orange of rooms and things we sent, and the pink of radio-plus.
 */
data class StatusColors(
    val good: Color,
    val warn: Color,
    val warnContainer: Color,
    val accent: Color,
    val accentContainer: Color,
    val plus: Color,
)

private val StandardLightStatus = StatusColors(
    good = Color(0xFF178A2A), warn = Color(0xFF9A6A00), warnContainer = Color(0xFFFFE9B8),
    accent = Color(0xFFC2491A), accentContainer = Color(0xFFFFE2D4), plus = Color(0xFFB0189F),
)
private val StandardDarkStatus = StatusColors(
    good = Color(0xFF39FF14), warn = Color(0xFFFFBF00), warnContainer = Color(0xFF3A2C00),
    accent = Color(0xFFFF7A3D), accentContainer = Color(0xFF4A2010), plus = Color(0xFFFF4DFF),
)
private val Modem73Status = StatusColors(
    good = Color(0xFF18FC7C), warn = Color(0xFFE6B800), warnContainer = Color(0xFF2A2000),
    accent = Color(0xFFFF7A3D), accentContainer = Color(0xFF2A1030), plus = Color(0xFFFF4DFF),
)

val LocalStatusColors = staticCompositionLocalOf { StandardLightStatus }

// Standard: the weechatradio.com website palette mapped to Material 3 roles
// (same values as the mockups in mobile-app-material/screens-source.html).
private val StandardLight = lightColorScheme(
    primary = Color(0xFF3654D1), onPrimary = Color.White,
    primaryContainer = Color(0xFFDDE3FF), onPrimaryContainer = Color(0xFF0E1C66),
    // Selected chips, tabs and tonal buttons use the secondary slots: blue, as in the mockups.
    secondary = Color(0xFF3654D1), onSecondary = Color.White,
    secondaryContainer = Color(0xFFDDE3FF), onSecondaryContainer = Color(0xFF0E1C66),
    tertiary = Color(0xFF6A4FC9),
    background = Color(0xFFFCFAFF), onBackground = Color(0xFF1A1726),
    surface = Color(0xFFFCFAFF), onSurface = Color(0xFF1A1726),
    surfaceVariant = Color(0xFFE7E2F0), onSurfaceVariant = Color(0xFF5E5870),
    surfaceContainerLow = Color(0xFFF5F2FA), surfaceContainer = Color(0xFFF5F2FA),
    surfaceContainerHigh = Color(0xFFEFEBF6), surfaceContainerHighest = Color(0xFFE7E2F0),
    outline = Color(0xFFCDC6D9), outlineVariant = Color(0xFFE7E2F0),
    error = Color(0xFFC8174A),
)

private val StandardDark = darkColorScheme(
    primary = Color(0xFF7D9BFF), onPrimary = Color(0xFF06081A),
    primaryContainer = Color(0xFF222C5C), onPrimaryContainer = Color(0xFFD9E0FF),
    secondary = Color(0xFF7D9BFF), onSecondary = Color(0xFF06081A),
    secondaryContainer = Color(0xFF222C5C), onSecondaryContainer = Color(0xFFD9E0FF),
    tertiary = Color(0xFFC4B5FD),
    background = Color(0xFF07070A), onBackground = Color(0xFFF1ECFA),
    surface = Color(0xFF07070A), onSurface = Color(0xFFF1ECFA),
    surfaceVariant = Color(0xFF1E1C27), onSurfaceVariant = Color(0xFF948CA3),
    surfaceContainerLow = Color(0xFF100F15), surfaceContainer = Color(0xFF100F15),
    surfaceContainerHigh = Color(0xFF16151D), surfaceContainerHighest = Color(0xFF1E1C27),
    outline = Color(0xFF2E2B38), outlineVariant = Color(0xFF1E1C27),
    error = Color(0xFFFF3B6B),
)

// modem73: the desktop app's modem73 look (crates/wcr/src/gui/modem73.rs).
// Always dark, monospace, green accents.
private val Modem73 = darkColorScheme(
    primary = Color(0xFF18FC7C), onPrimary = Color(0xFF0A0005),
    primaryContainer = Color(0xFF2A1030), onPrimaryContainer = Color(0xFF18FC7C),
    secondary = Color(0xFF18FC7C), onSecondary = Color(0xFF0A0005),
    secondaryContainer = Color(0xFF2A1030), onSecondaryContainer = Color(0xFF18FC7C),
    tertiary = Color(0xFF00D0D0),
    background = Color(0xFF100008), onBackground = Color(0xFFE8E0E4),
    surface = Color(0xFF100008), onSurface = Color(0xFFE8E0E4),
    surfaceVariant = Color(0xFF0A0005), onSurfaceVariant = Color(0xFF8A8286),
    surfaceContainerLow = Color(0xFF0A0005), surfaceContainer = Color(0xFF0A0005),
    surfaceContainerHigh = Color(0xFF16060E), surfaceContainerHighest = Color(0xFF1E0A14),
    outline = Color(0xFF3A3236), outlineVariant = Color(0xFF3A3236),
    error = Color(0xFFFF4D4D),
)

private fun Typography.mono(): Typography {
    fun TextStyle.m() = copy(fontFamily = FontFamily.Monospace)
    return Typography(
        displayLarge.m(), displayMedium.m(), displaySmall.m(),
        headlineLarge.m(), headlineMedium.m(), headlineSmall.m(),
        titleLarge.m(), titleMedium.m(), titleSmall.m(),
        bodyLarge.m(), bodyMedium.m(), bodySmall.m(),
        labelLarge.m(), labelMedium.m(), labelSmall.m(),
    )
}

@Composable
fun WeeChatRadioTheme(
    choice: ThemeChoice,
    dark: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val scheme: ColorScheme
    val status: StatusColors
    val type: Typography
    when {
        choice == ThemeChoice.Modem73 -> {
            scheme = Modem73
            status = Modem73Status
            type = Typography().mono()
        }
        dark -> {
            scheme = StandardDark
            status = StandardDarkStatus
            type = Typography()
        }
        else -> {
            scheme = StandardLight
            status = StandardLightStatus
            type = Typography()
        }
    }
    CompositionLocalProvider(LocalStatusColors provides status) {
        MaterialTheme(colorScheme = scheme, typography = type, content = content)
    }
}
