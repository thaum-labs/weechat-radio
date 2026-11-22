package com.weechatradio.app.ui

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.weechatradio.app.core.Core
import com.weechatradio.app.radio.BtAudio
import com.weechatradio.app.radio.Link
import com.weechatradio.app.radio.Mode
import com.weechatradio.app.radio.Ptt
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.ui.theme.Mono

// Design screens 16–20: setup. Steps: 1 radio, 1b Bluetooth (only for Bluetooth), 2 you, 3 levels, 4 permissions.

enum class SetupStep(val number: Int) { Radio(1), Bluetooth(1), You(2), Levels(3), Permissions(4) }

@Composable
fun SetupScreen(radio: Radio, step: SetupStep, onStep: (SetupStep) -> Unit, onClose: () -> Unit, onDone: () -> Unit) {
    val s = radio.status
    val next: SetupStep? = when (step) {
        SetupStep.Radio -> if (s.link == Link.Bluetooth) SetupStep.Bluetooth else SetupStep.You
        SetupStep.Bluetooth -> SetupStep.You
        SetupStep.You -> if (s.link == Link.InternetOnly) SetupStep.Permissions else SetupStep.Levels
        SetupStep.Levels -> SetupStep.Permissions
        SetupStep.Permissions -> null
    }
    val back: SetupStep? = when (step) {
        SetupStep.Radio -> null
        SetupStep.Bluetooth -> SetupStep.Radio
        SetupStep.You -> if (s.link == Link.Bluetooth) SetupStep.Bluetooth else SetupStep.Radio
        SetupStep.Levels -> SetupStep.You
        SetupStep.Permissions -> if (s.link == Link.InternetOnly) SetupStep.You else SetupStep.Levels
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { onDone() }
    val primary = when (next) {
        SetupStep.You -> "Next: you"
        SetupStep.Levels -> "Next: levels"
        SetupStep.Permissions -> "Next: permissions"
        SetupStep.Bluetooth -> "Next: find your radio"
        else -> "Start station"
    }
    Column(Modifier.fillMaxSize()) {
        TopBar(
            "Set up", "Step ${step.number} of 4", titleSize = 20.sp,
            lead = {
                if (back == null) IconBtn(Icons.Filled.Close, "Close", onClick = onClose)
                else BackLead { onStep(back) }
            },
        ) { Box(Modifier.padding(end = 16.dp)) { BrandMark(24.dp) } }
        Row(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            repeat(4) { i -> Box(Modifier.weight(1f).height(4.dp).clip(RoundedCornerShape(2.dp)).background(if (i < step.number) colors().primary else colors().secondaryContainer)) }
        }
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            when (step) {
                SetupStep.Radio -> StepRadio(radio)
                SetupStep.Bluetooth -> StepBluetooth(radio)
                SetupStep.You -> StepYou(radio)
                SetupStep.Levels -> StepLevels(radio)
                SetupStep.Permissions -> StepPermissions(radio)
            }
        }
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(topStart = 16.dp, topEnd = 16.dp)).background(colors().surfaceContainer)
                .padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 14.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (back != null) TextButton({ onStep(back) }, Modifier.weight(1f)) { Text("Back", fontSize = 15.sp) }
            Button(
                {
                    if (next != null) onStep(next) else {
                        val wanted = buildList {
                            if (s.linked) add(Manifest.permission.RECORD_AUDIO)
                            add(Manifest.permission.ACCESS_FINE_LOCATION)
                            if (Build.VERSION.SDK_INT >= 33) add(Manifest.permission.POST_NOTIFICATIONS)
                            if (s.link == Link.Bluetooth && Build.VERSION.SDK_INT >= 31) add(Manifest.permission.BLUETOOTH_CONNECT)
                        }
                        ask.launch(wanted.toTypedArray())
                    }
                },
                Modifier.weight(2f),
            ) { Text(primary, fontSize = 15.sp) }
        }
    }
}

@Composable
private fun Heading(text: String) = Text(text, Modifier.padding(top = 6.dp), fontSize = 28.sp, lineHeight = 34.sp)

@Composable
private fun Option(selected: Boolean, icon: ImageVector, title: String, sub: String, onClick: () -> Unit, extra: (@Composable () -> Unit)? = null) {
    val shape = RoundedCornerShape(16.dp)
    Row(
        Modifier.fillMaxWidth().clip(shape)
            .background(if (selected) colors().primary.copy(alpha = 0.12f) else Color.Transparent)
            .border(if (selected) 2.dp else 1.dp, if (selected) colors().primary else colors().outlineVariant, shape)
            .clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 14.dp),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        RadioButton(selected, onClick, Modifier.size(20.dp))
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(title, Modifier.weight(1f), fontSize = 16.sp, fontWeight = FontWeight.Medium)
                Icon(icon, null, tint = colors().primary)
            }
            Text(sub, Modifier.padding(top = 3.dp), fontSize = 13.5.sp, lineHeight = 19.sp, color = colors().onSurfaceVariant)
            extra?.invoke()
        }
    }
}

/** Step 1: how is the radio connected. */
@Composable
private fun StepRadio(radio: Radio) {
    val s = radio.status
    Heading("How is your radio connected?")
    Option(s.link == Link.Bluetooth, Icons.Filled.Bluetooth, "Bluetooth", "VR-N76, UV-PRO, GA-5WB, VR-N7600. No cables: the radio does audio and PTT. Or any radio as a Bluetooth headset, keyed by VOX.", {
        radio.update { it.copy(link = Link.Bluetooth, rig = "VR-N76", preset = "vhf-fm") }
    })
    Option(s.link == Link.Usb, Icons.Filled.Usb, "USB cable", "Digirig or a USB sound card, with CAT or VOX for PTT. Best for vhf-fast, mail and files.", {
        radio.update { it.copy(link = Link.Usb, rig = "Digirig Mini", preset = "vhf-fast") }
    }) {
        if (s.link == Link.Usb) {
            Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Dot(radioColors().ok); Text("Digirig Mini found · audio + serial", fontSize = 13.sp, color = radioColors().ok)
            }
        }
    }
    Option(s.link == Link.InternetOnly, Icons.Filled.Cloud, "No radio, internet only", "Chat and email over the hub. You can add a radio later.", {
        radio.update { it.copy(link = Link.InternetOnly, mode = Mode.Internet) }
    })
    Row(Modifier.fillMaxWidth().padding(top = 4.dp), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
        listOf(Icons.Filled.PhoneAndroid, if (s.link == Link.Bluetooth) Icons.Filled.Bluetooth else Icons.Filled.Usb, Icons.Filled.Mic, Icons.Filled.SettingsInputAntenna).forEachIndexed { i, ic ->
            Box(Modifier.size(44.dp).clip(CircleShape).background(colors().secondaryContainer), contentAlignment = Alignment.Center) { Icon(ic, null, tint = colors().onSecondaryContainer) }
            if (i < 3) Text(" — ", color = colors().onSurfaceVariant)
        }
    }
    Small("You are the control operator. Stay within your licence and your band plan.", Modifier.fillMaxWidth(), lineHeight = 17.sp)
}

/** Step 1b: find the Bluetooth radio, and pick what runs over the link (Headset · VOX lives here). */
@Composable
private fun StepBluetooth(radio: Radio) {
    val s = radio.status
    Heading("Find your radio")
    Segmented(BtAudio.entries.map { it.label }, s.btAudio.ordinal) { i ->
        val a = BtAudio.entries[i]
        radio.update {
            it.copy(
                btAudio = a,
                rig = if (a == BtAudio.HeadsetVox) "UV-36 Pro" else "VR-N76",
                preset = when (a) { BtAudio.HeadsetVox -> "hf-poor"; BtAudio.Tnc1200 -> "afsk-1200"; BtAudio.Modem73 -> "vhf-fm" },
            )
        }
    }
    if (s.btAudio == BtAudio.HeadsetVox) {
        DeviceRow("UV-36 Pro", "headset · ${s.btAddress} · −49 dBm") { Chip("linked", small = true, container = radioColors().okContainer, content = colors().onSurface) }
    } else {
        DeviceRow("VR-N76", "${s.btAddress} · −52 dBm") { Chip("linked", small = true, container = radioColors().okContainer, content = colors().onSurface) }
        DeviceRow("UV-PRO", "11:3A:5C:07:E2:90 · not paired") { FilledTonalButton({}, Modifier.height(34.dp), contentPadding = PaddingValues(horizontal = 16.dp)) { Text("Pair") } }
    }
    OutlinedButton({}, Modifier.fillMaxWidth()) { Icon(Icons.Filled.Refresh, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Find radio again") }
    Column {
        Label("ON THE RADIO, ONCE", Modifier.padding(top = 4.dp, bottom = 4.dp))
        if (s.btAudio == BtAudio.HeadsetVox) {
            Check(true, "Pair the radio as a Bluetooth headset (phone audio)")
            Check(true, "VOX on, level 4 · squelch 1 (VOX will not key at squelch 0)")
            Check(true, "Squelch 1 can hide very short bursts; that is the trade-off for VOX")
            Check(false, "No beacons on VOX: the VOX tone would walk on incoming frames")
        } else {
            Check(true, "Squelch 0 (short bursts vanish at 1)")
            Check(true, if (s.btAudio == BtAudio.Tnc1200) "KISS TNC on · Digital Mode off" else "KISS TNC off · Digital Mode off")
            Check(true, "Menu → Pairing, PIN 0000 if asked")
            Check(false, "Close the HT phone app: the radio takes one Bluetooth client at a time")
        }
    }
    Card(color = colors().surfaceContainer, padding = PaddingValues(horizontal = 14.dp, vertical = 12.dp)) {
        Label(if (s.btAudio == BtAudio.HeadsetVox) "PRESETS ON HEADSET VOX" else "PRESETS ON BLUETOOTH AUDIO", Modifier.padding(bottom = 8.dp))
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            presetsFor(s).forEachIndexed { i, p ->
                Chip(
                    if (i == 0) "$p · default" else p, selected = i == 0, mono = true,
                    border = if (p == "vhf-fast") radioColors().warn else null,
                    trail = { if (p == "vhf-fast") Text("untested on Bluetooth", fontSize = 11.sp, color = radioColors().warn) },
                )
            }
        }
    }
}

@Composable
private fun DeviceRow(name: String, sub: String, right: @Composable () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        Box(Modifier.size(40.dp).clip(CircleShape).background(colors().primaryContainer), contentAlignment = Alignment.Center) {
            Icon(Icons.Filled.Bluetooth, null, Modifier.size(22.dp), tint = colors().onPrimaryContainer)
        }
        Column(Modifier.weight(1f)) {
            Text(name, fontSize = 16.sp, lineHeight = 24.sp)
            MonoText(sub, color = colors().onSurfaceVariant, size = 12.sp)
        }
        right()
    }
}

@Composable
private fun Check(ok: Boolean, text: String) {
    Row(Modifier.padding(vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        if (ok) Icon(Icons.Filled.Check, null, Modifier.size(20.dp), tint = radioColors().ok)
        else Icon(Icons.Filled.Warning, null, Modifier.size(20.dp), tint = radioColors().warn)
        Text(text, fontSize = 14.sp, lineHeight = 19.sp)
    }
}

/** Step 2: callsign, locator, mode. */
@Composable
private fun StepYou(radio: Radio) {
    val s = radio.status
    var call by rememberSaveable { mutableStateOf(s.call) }
    var grid by rememberSaveable { mutableStateOf(s.grid) }
    Heading("Who is on the air?")
    OutlinedTextField(
        call, { call = it.uppercase(); radio.update { st -> st.copy(call = call.trim()) } }, Modifier.fillMaxWidth().padding(top = 6.dp),
        label = { Text("Callsign") }, singleLine = true, textStyle = TextStyle(fontFamily = Mono, fontSize = 15.sp),
        trailingIcon = { if (call.length >= 3) Icon(Icons.Filled.Check, "ok", tint = radioColors().ok) },
        supportingText = { Text("Every frame carries it. Add /M, /P or /A if you operate that way.") },
    )
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        OutlinedTextField(
            grid, { grid = it.uppercase(); radio.update { st -> st.copy(grid = grid.trim()) } }, Modifier.weight(1f),
            label = { Text("Locator (grid)") }, singleLine = true, textStyle = TextStyle(fontFamily = Mono, fontSize = 15.sp),
        )
        FilledTonalButton({
            val g = Core.gridFromLocation(51.2308, -2.3207) ?: "IO81UF"
            grid = g; radio.update { it.copy(grid = g) }
        }, contentPadding = PaddingValues(start = 12.dp, end = 18.dp)) {
            Icon(Icons.Filled.MyLocation, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Use GPS")
        }
    }
    Small("GPS fix 51.2308, −2.3207 matches ${s.grid}. Used for the map; never message text.", Modifier.padding(start = 16.dp), lineHeight = 16.sp)
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(colors().surfaceContainer), verticalAlignment = Alignment.CenterVertically) {
        TileMap(LatLon(51.2308, -2.3207), 9, {}, Modifier.size(120.dp, 72.dp), tiles = LocalTilesOn.current) { project ->
            val p = project(LatLon(51.2308, -2.3207))
            drawCircle(Color(0x4DFF8A50), 14.dp.toPx(), p)
            drawCircle(Color(0xFFFF8A50), 10.dp.toPx(), p)
        }
        Column(Modifier.padding(horizontal = 14.dp)) {
            MonoText(s.grid, size = 14.sp)
            Text("Frome · grid square preview", fontSize = 12.5.sp, color = colors().onSurfaceVariant)
        }
    }
    Label("MODE", Modifier.padding(top = 2.dp))
    Mode.entries.forEach { m ->
        Row(
            Modifier.fillMaxWidth().clickable { radio.update { it.copy(mode = m) } }.padding(horizontal = 4.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            RadioButton(s.mode == m, { radio.update { it.copy(mode = m) } }, Modifier.size(20.dp))
            Column {
                Text(m.id, fontSize = 15.sp, color = modeColor(m))
                Text(m.words, fontSize = 12.5.sp, lineHeight = 17.sp, color = colors().onSurfaceVariant)
            }
        }
    }
}

/** Step 3: preset, audio levels and PTT. */
@Composable
private fun StepLevels(radio: Radio) {
    val s = radio.status
    val bt = s.link == Link.Bluetooth
    Heading("Preset and audio levels")
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        presetsFor(s).forEach { p ->
            Chip(
                p, selected = p == s.preset, mono = true, border = if (bt && p == "vhf-fast" && p != s.preset) radioColors().warn else null,
                onClick = { radio.update { it.copy(preset = p) } },
            )
        }
    }
    val about = Core.presets().firstOrNull { it.name == s.preset }?.description ?: presetWords[s.preset] ?: ""
    Text(
        androidx.compose.ui.text.buildAnnotatedString {
            append("$about. ")
            if (bt && s.preset == "vhf-fast") {
                pushStyle(androidx.compose.ui.text.SpanStyle(color = radioColors().warn)); append("Untested on Bluetooth. "); pop()
            }
            append("Frequency ")
            pushStyle(androidx.compose.ui.text.SpanStyle(fontFamily = Mono, color = radioColors().tx)); append("${s.freq} MHz"); pop()
            append(" ${s.emission}, NARROW, low power indoors.")
        },
        fontSize = 13.sp, lineHeight = 19.sp, color = colors().onSurfaceVariant,
    )
    Card(padding = PaddingValues(horizontal = 16.dp, vertical = 14.dp)) {
        Row(verticalAlignment = Alignment.Bottom) {
            Text("Level check", Modifier.weight(1f), fontSize = 16.sp, fontWeight = FontWeight.Medium)
            Small("live from the radio")
        }
        Box(Modifier.padding(top = 12.dp)) { MeterRow("IN", s.inLevel, s.inDb) }
        Small("Open squelch or key a friend. Green is healthy; orange is clipping.", Modifier.padding(start = 42.dp, top = 4.dp))
        if (!bt) {
            Row(Modifier.padding(top = 14.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Label("OUT", Modifier.width(30.dp))
                Slider(
                    value = (s.outSetDb + 20f) / 20f, onValueChange = { v -> radio.update { it.copy(outSetDb = (v * 20f - 20f)) } },
                    modifier = Modifier.weight(1f),
                )
                MonoText("%.1f dB".format(s.outSetDb), Modifier.width(64.dp), size = 12.sp)
            }
            Row(Modifier.padding(top = 10.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilledTonalButton({}, Modifier.weight(1f)) { Icon(Icons.Filled.PlayArrow, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Test tone") }
                OutlinedButton({}, Modifier.weight(1f)) { Text("Auto-set") }
            }
        }
    }
    if (!bt) {
        Card(padding = PaddingValues(horizontal = 16.dp, vertical = 14.dp)) {
            Text("PTT method", Modifier.padding(bottom = 10.dp), fontSize = 14.sp, fontWeight = FontWeight.Medium)
            Segmented(Ptt.entries.map { it.name }, s.ptt.ordinal) { i -> radio.update { it.copy(ptt = Ptt.entries[i]) } }
            Text(
                when (s.ptt) {
                    Ptt.CAT -> "The phone keys the radio over the Digirig's serial port. Steadiest choice."
                    Ptt.VOX -> "The radio keys itself on our audio: VOX on, level 4, squelch 1. Starts on hf-poor; no beacons on VOX."
                    Ptt.RTS -> "The Digirig's RTS line keys the radio, for radios without CAT."
                },
                Modifier.padding(top = 10.dp), fontSize = 12.5.sp, lineHeight = 18.sp, color = colors().onSurfaceVariant,
            )
        }
    }
    Small("Set the radio to NFM narrow, low power indoors. Handhelds: short rests between TX blocks.", size = 12.5.sp, lineHeight = 18.sp)
}

private val presetWords = mapOf(
    "vhf-fast" to "VHF/UHF FM, strong local links (OFDM QAM16 1/2). Cabled PTT preferred",
    "vhf-fm" to "VHF/UHF FM, the safe default for handhelds",
    "hf-good" to "HF, good conditions",
    "hf-poor" to "HF, poor conditions; also the preset that decodes over a speaker and mic",
    "hf-weak" to "HF, weak signals",
    "hf-deep" to "HF, very weak signals, slowest",
    "afsk-1200" to "1200 baud AFSK, for radios with a built-in TNC",
)

/** Step 4: what the phone needs to run a station. Listen in background is on by default. */
@Composable
private fun StepPermissions(radio: Radio) {
    val s = radio.status
    Heading("Let the phone run a station")
    Column {
        Permission(Icons.Filled.Mic, "Microphone", "Hear the radio through the USB or Bluetooth audio, and measure levels. Never records to disk.") {
            Chip("allowed", small = true, container = radioColors().okContainer, content = colors().onSurface)
        }
        Permission(Icons.Filled.Usb, "USB device", "Digirig: audio, CAT and PTT. Asked when you plug it in.") {
            Chip(if (s.link == Link.Usb) "ask on plug" else "not needed", small = true, container = if (s.link == Link.Usb) radioColors().warnContainer else null, content = if (s.link == Link.Usb) radioColors().onWarnContainer else null)
        }
        Permission(Icons.Filled.Bluetooth, "Nearby devices", "Find and link the VR-N76 over Bluetooth.") {
            Chip(if (s.link == Link.Bluetooth) "allow" else "not needed", small = true, container = if (s.link == Link.Bluetooth) radioColors().txContainer else null, content = if (s.link == Link.Bluetooth) radioColors().onTxContainer else null)
        }
        Permission(Icons.Filled.Place, "Location", "Locator and map pins, while the app is open.") {
            Chip("while using", small = true, container = radioColors().okContainer, content = colors().onSurface)
        }
        Permission(Icons.Filled.Notifications, "Notifications", "New mail, direct messages and TX alerts, even with the screen off.") {
            Chip("allow", small = true, container = radioColors().txContainer, content = radioColors().onTxContainer)
        }
        Permission(
            Icons.Filled.BatteryFull, "Listen in background",
            "On by default. Keeps listening with the screen off and shows a notification: weechat-radio listening on ${s.freq}. Uses battery; turn off in Settings.",
        ) {
            Switch(s.listenInBackground, { on -> radio.update { it.copy(listenInBackground = on) } })
        }
    }
}

@Composable
private fun Permission(icon: ImageVector, title: String, sub: String, right: @Composable () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 10.dp), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        Icon(icon, null, Modifier.padding(top = 2.dp), tint = colors().primary)
        Column(Modifier.weight(1f)) {
            Text(title, fontSize = 15.sp, lineHeight = 24.sp)
            Text(sub, fontSize = 13.sp, lineHeight = 18.sp, color = colors().onSurfaceVariant)
        }
        Box(Modifier.padding(top = 2.dp)) { right() }
    }
}

/** False in tests, so the map draws without the network. */
val LocalTilesOn = staticCompositionLocalOf { true }
