package com.weechatradio.app.ui

import android.Manifest
import android.content.Context
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.weechatradio.app.BuildConfig
import com.weechatradio.app.REAL_RADIO
import com.weechatradio.app.hasBluetooth
import com.weechatradio.app.radio.LiveRadio
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.radio.Station
import com.weechatradio.app.ui.theme.Console
import com.weechatradio.app.ui.theme.Look
import com.weechatradio.app.ui.theme.Mono

// Settings (gear on the Radio tab): the eight things a user sets, nothing else
// (Tobias, 2026-11-27). Layout from the approved design canvas "Android app themes".

/** Presets and what each is for, as the design lists them. */
private val PresetWhat = mapOf(
    "vhf-fm" to "VHF/UHF FM, clean local links · QPSK 1/2",
    "vhf-fast" to "Strong local links · QAM16",
    "afsk-1200" to "The radio's own TNC · 1200 bd packet",
    "hf-good" to "Good HF SSB path · 8PSK 1/2",
    "hf-poor" to "Fading HF, NVIS · RDM-600S",
    "hf-weak" to "Very weak HF · RDM-300S",
    "hf-deep" to "Deep-fade HF backup · MFSK-32R",
)

@Composable
fun SettingsScreen(radio: Radio, look: Look, onLook: (Look) -> Unit, onBack: () -> Unit, onSetup: (SetupStep) -> Unit) {
    val s = radio.status
    val m73 = console()
    var presetOpen by rememberSaveable { mutableStateOf(false) }
    // Only presets this link can carry: on the real radio, the Bluetooth-safe ones.
    val presets = if (radio is LiveRadio) PresetWhat.keys.filter { it in LiveRadio.BT_PRESETS } else presetsFor(s)
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            TopBar(if (m73) "SETUP" else "Settings", lead = { BackLead(onBack) })
            Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) {
                Head("Station")
                Setting(if (m73) "CALL" else "Callsign", { onSetup(SetupStep.You) }) { Value(s.call, bold = m73) }
                Setting(if (m73) "GRID" else "Locator", { onSetup(SetupStep.You) }) { Value(s.grid) }
                RestartForCallsign(radio, Modifier.padding(horizontal = 16.dp, vertical = 6.dp))

                Head("Radio")
                RealRadio(s.real, s.rig)
                Frequency(s.freq) { radio.tune(it, s.emission, s.preset) }
                Setting(if (m73) "PRESET" else "Preset", { presetOpen = !presetOpen }) {
                    if (m73) {
                        Box(Modifier.height(36.dp).border(1.dp, colors().outline).background(colors().surfaceContainer).padding(horizontal = 10.dp), contentAlignment = Alignment.Center) {
                            Text("${s.preset}  v", fontSize = 14.sp, color = Console.lilac)
                        }
                    } else {
                        MonoText(s.preset, color = colors().primary, size = 15.sp)
                        Icon(if (presetOpen) Icons.Filled.KeyboardArrowUp else Icons.Filled.KeyboardArrowDown, null, Modifier.size(18.dp), tint = colors().primary)
                    }
                }
                if (presetOpen) Presets(presets, s.preset) { presetOpen = false; radio.tune(s.freq, s.emission, it) }

                Head("Phone")
                Setting(if (m73) "THEME" else "Theme") {
                    if (m73) {
                        Choice("modem73", look == Look.Modem73) { onLook(Look.Modem73) }
                        Spacer(Modifier.width(14.dp))
                        Choice("standard", look == Look.Material) { onLook(Look.Material) }
                    } else {
                        Box(Modifier.width(220.dp)) { Segmented(listOf(Look.Material.label, Look.Modem73.label), look.ordinal) { onLook(Look.entries[it]) } }
                    }
                }
                OnOff(
                    if (m73) "LISTEN IN BG" else "Listen in background",
                    "Keeps listening with the screen off",
                    s.listenInBackground,
                ) { on -> radio.update { it.copy(listenInBackground = on, listening = true) } }

                Head("About")
                Setting(if (m73) "VERSION" else "App version") { Text(BuildConfig.VERSION_NAME, fontSize = if (m73) 14.sp else 15.sp, color = if (m73) colors().onSurface else colors().onSurfaceVariant) }
                Spacer(Modifier.height(16.dp))
            }
        }
        radio.note?.let { Note(it, Modifier.align(Alignment.BottomCenter).padding(bottom = 16.dp)) }
    }
}

/** Section heading: blue words in the stock look, grey capitals in the console. */
@Composable
private fun Head(text: String) {
    if (console()) Text(text.uppercase(), Modifier.padding(start = 16.dp, end = 16.dp, top = 14.dp, bottom = 4.dp), fontSize = 11.sp, letterSpacing = 2.sp, color = colors().onSurfaceVariant)
    else Text(text, Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 4.dp), fontSize = 14.sp, lineHeight = 20.sp, fontWeight = FontWeight.Medium, color = colors().primary)
}

/** One setting: its name, then its value or control. */
@Composable
private fun Setting(title: String, onClick: (() -> Unit)? = null, sub: String? = null, value: @Composable RowScope.() -> Unit) {
    val m73 = console()
    androidx.compose.foundation.layout.Row(
        Modifier.fillMaxWidth().then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .heightIn(min = if (m73) 44.dp else if (sub != null) 72.dp else 56.dp).padding(horizontal = 16.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(if (m73) 0.dp else 8.dp),
    ) {
        if (m73) {
            Text(title, Modifier.width(128.dp), fontSize = 12.sp, letterSpacing = 1.sp, color = colors().onSurfaceVariant)
        } else {
            Column(Modifier.weight(1f)) {
                Text(title, fontSize = 16.sp, lineHeight = 24.sp)
                if (sub != null) Text(sub, fontSize = 14.sp, lineHeight = 20.sp, color = colors().onSurfaceVariant)
            }
        }
        value()
    }
}

@Composable
private fun Value(text: String, bold: Boolean = false) {
    if (console()) {
        Box(Modifier.height(32.dp).widthIn(min = 120.dp).border(1.dp, colors().outline).background(colors().surfaceContainer).padding(horizontal = 10.dp), contentAlignment = Alignment.CenterStart) {
            Text(text, fontSize = 14.sp, fontWeight = if (bold) FontWeight.Bold else null)
        }
    } else MonoText(text, color = colors().onSurfaceVariant, size = 15.sp)
}

/** The console's radio-button text: "(*) modem73" or "( ) standard". */
@Composable
private fun Choice(text: String, on: Boolean, onClick: () -> Unit) =
    Text((if (on) "(*) " else "( ) ") + text, Modifier.clickable(onClick = onClick).padding(vertical = 10.dp), fontSize = 13.sp, color = if (on) radioColors().ok else colors().onSurfaceVariant)

/** A switch; the console writes it as "[x] on" or "[ ] off". */
@Composable
private fun OnOff(title: String, sub: String, on: Boolean, onText: String = "on", onChange: (Boolean) -> Unit) {
    if (console()) {
        Setting(title, { onChange(!on) }) { Text(if (on) "[x] $onText" else "[ ] off", fontSize = 13.sp, color = if (on) radioColors().ok else colors().onSurfaceVariant) }
    } else {
        Setting(title, { onChange(!on) }, sub) { Switch(on, onChange) }
    }
}

/** Frequency in MHz, typed in and set with Set. */
@Composable
private fun Frequency(freq: String, onSet: (String) -> Unit) {
    val m73 = console()
    val focus = LocalFocusManager.current
    var draft by remember(freq) { mutableStateOf(freq) }
    var bad by remember { mutableStateOf(false) }
    val set = {
        val f = draft.trim().replace(',', '.').toDoubleOrNull()
        bad = f == null || f < 1 || f > 1300
        if (!bad) { onSet("%.3f".format(java.util.Locale.ROOT, f)); focus.clearFocus() }
    }
    Setting(if (m73) "FREQ" else "Frequency", sub = if (bad) "Enter MHz, like 433.625" else null) {
        val shape = if (m73) RoundedCornerShape(0.dp) else RoundedCornerShape(8.dp)
        androidx.compose.foundation.layout.Row(
            Modifier.height(if (m73) 36.dp else 44.dp).clip(shape).border(1.dp, if (bad) colors().error else colors().outline, shape)
                .then(if (m73) Modifier.background(colors().surfaceContainer) else Modifier).padding(horizontal = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            BasicTextField(
                draft, { draft = it; bad = false }, Modifier.width(76.dp), singleLine = true,
                textStyle = TextStyle(fontFamily = Mono, fontSize = 15.sp, color = colors().onSurface),
                cursorBrush = SolidColor(colors().primary),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal, imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { set() }),
            )
            Text("MHz", fontSize = if (m73) 12.sp else 14.sp, color = colors().onSurfaceVariant)
        }
        if (m73) {
            Spacer(Modifier.weight(1f))
            Text("[SET]", Modifier.clickable(onClick = set).padding(start = 8.dp, top = 12.dp, bottom = 12.dp), fontSize = 12.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp, color = radioColors().ok)
        } else {
            FilledTonalButton(set, Modifier.height(40.dp), contentPadding = PaddingValues(horizontal = 16.dp)) { Text("Set") }
        }
    }
}

/** The preset list under Preset, one tap to pick. */
@Composable
private fun Presets(ids: List<String>, current: String, onPick: (String) -> Unit) {
    val m73 = console()
    Column(
        Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp).fillMaxWidth()
            .then(if (m73) Modifier.border(1.dp, colors().outline).background(colors().surfaceContainer) else Modifier.clip(RoundedCornerShape(16.dp)).background(colors().surfaceContainer)),
    ) {
        ids.forEach { id ->
            val on = id == current
            androidx.compose.foundation.layout.Row(
                Modifier.fillMaxWidth().clickable { onPick(id) }.heightIn(min = 48.dp).padding(horizontal = if (m73) 10.dp else 12.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(if (m73) 8.dp else 12.dp),
            ) {
                if (m73) {
                    Text(if (on) "(*)" else "( )", fontSize = 13.sp, color = if (on) radioColors().ok else colors().onSurfaceVariant)
                } else {
                    Box(Modifier.size(20.dp).clip(CircleShape).border(2.dp, if (on) colors().primary else colors().onSurfaceVariant, CircleShape), contentAlignment = Alignment.Center) {
                        if (on) Box(Modifier.size(10.dp).clip(CircleShape).background(colors().primary))
                    }
                }
                Column(Modifier.weight(1f)) {
                    Text(id, fontFamily = Mono, fontSize = if (m73) 13.sp else 14.sp, fontWeight = if (m73) FontWeight.Bold else FontWeight.Medium, color = if (m73 && on) radioColors().ok else Color.Unspecified)
                    PresetWhat[id]?.let { Text(it, fontSize = if (m73) 11.sp else 12.sp, lineHeight = 16.sp, color = colors().onSurfaceVariant) }
                }
            }
        }
    }
}

/**
 * Run the real station with the radio paired to this phone instead of the
 * sample data. Asks for Bluetooth first. Remembered across restarts.
 */
@Composable
private fun RealRadio(on: Boolean, rig: String) {
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences("settings", Context.MODE_PRIVATE) }
    fun set(value: Boolean) {
        prefs.edit().putBoolean(REAL_RADIO, value).apply()
        Station.useReal(context, value)
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted -> if (granted) set(true) }
    val name = if (on) rig else "VR-N76"
    OnOff(if (console()) "REAL RADIO" else "Use the real radio", "$name over Bluetooth", on, onText = "$name · bluetooth") { want ->
        when {
            !want -> set(false)
            hasBluetooth(context) -> set(true)
            else -> ask.launch(Manifest.permission.BLUETOOTH_CONNECT)
        }
    }
}
