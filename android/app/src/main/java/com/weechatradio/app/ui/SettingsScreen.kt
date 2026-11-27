package com.weechatradio.app.ui

import android.Manifest
import android.content.Context
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.weechatradio.app.BuildConfig
import com.weechatradio.app.REAL_RADIO
import com.weechatradio.app.hasBluetooth
import com.weechatradio.app.radio.Station
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.ui.theme.Look

// Settings: only what a user sets (Station, Radio, Phone). Opens from the gear; no nav bar.

@Composable
fun SettingsScreen(radio: Radio, look: Look, onLook: (Look) -> Unit, onBack: () -> Unit, onSetup: (SetupStep) -> Unit) {
    val s = radio.status
    Column(Modifier.fillMaxSize()) {
        TopBar("Settings", lead = { BackLead(onBack) }) {
            Text("v${BuildConfig.VERSION_NAME}", Modifier.padding(end = 16.dp), fontSize = 11.sp, color = colors().onSurfaceVariant)
        }
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) {
            // Kept short on purpose (Tobias, 2026-11-27): only what a user sets.
            Section("Station", top = 4.dp)
            Item("Callsign", value = s.call, mono = true) { onSetup(SetupStep.You) }
            Item("Locator", value = s.grid, mono = true) { onSetup(SetupStep.You) }
            Divider()
            Section("Radio")
            RealRadio(s.real)
            Item("Frequency", value = s.freq, mono = true, after = " MHz", chevron = false) {}
            Item("Preset", value = s.preset, mono = true) { onSetup(SetupStep.Levels) }
            Divider()
            Section("Phone")
            Toggle("Theme: modem73", null, look == Look.Modem73) { on -> onLook(if (on) Look.Modem73 else Look.Material) }
            Toggle("Listen in background", null, s.listenInBackground) { on -> radio.update { it.copy(listenInBackground = on, listening = true) } }
        }
    }
}

@Composable
private fun Divider() = HorizontalDivider(Modifier.padding(horizontal = 16.dp), color = colors().outlineVariant)

@Composable
private fun Item(
    title: String,
    value: String? = null,
    sub: String? = null,
    mono: Boolean = false,
    after: String = "",
    valueColor: Color = Color.Unspecified,
    chevron: Boolean = true,
    onClick: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).heightIn(min = if (sub != null) 64.dp else 52.dp).padding(horizontal = 16.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, fontSize = 16.sp, lineHeight = 24.sp)
            if (sub != null) Text(sub, fontSize = 13.sp, color = colors().onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (value != null) {
            val col = if (valueColor == Color.Unspecified) colors().onSurfaceVariant else valueColor
            if (mono) MonoText(value, color = col, size = 13.sp) else Text(value, fontSize = 14.sp, color = col, maxLines = 1)
            if (after.isNotEmpty()) Text(after, fontSize = 14.sp, color = colors().onSurfaceVariant)
        }
        if (chevron) Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.padding(start = 4.dp), tint = colors().onSurfaceVariant)
    }
}

@Composable
private fun Toggle(title: String, sub: String?, on: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable { onChange(!on) }.heightIn(min = if (sub != null) 64.dp else 52.dp).padding(horizontal = 16.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, fontSize = 16.sp, lineHeight = 24.sp)
            if (sub != null) Text(sub, fontSize = 13.sp, color = colors().onSurfaceVariant)
        }
        Switch(on, onChange)
    }
}

/**
 * Phase 0: run the real station with the VR-N76 paired to this phone instead
 * of the simulated radio. Asks for Bluetooth first. Remembered across restarts.
 */
@Composable
private fun RealRadio(on: Boolean) {
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences("settings", Context.MODE_PRIVATE) }
    fun set(value: Boolean) {
        prefs.edit().putBoolean(REAL_RADIO, value).apply()
        Station.useReal(context, value)
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted -> if (granted) set(true) }
    Toggle("Use the real radio", "test: the VR-N76 paired with this phone, over Bluetooth", on) { want ->
        when {
            !want -> set(false)
            hasBluetooth(context) -> set(true)
            else -> ask.launch(Manifest.permission.BLUETOOTH_CONNECT)
        }
    }
}
