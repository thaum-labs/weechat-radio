package com.weechatradio.app.ui

import androidx.compose.foundation.background
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.weechatradio.app.core.Core
import com.weechatradio.app.radio.BtAudio
import com.weechatradio.app.radio.FrameKind
import com.weechatradio.app.radio.Link
import com.weechatradio.app.radio.LiveRadio
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.radio.Status
import com.weechatradio.app.ui.theme.ModeColors

// Design screens 13–15: Radio tab on USB (Digirig) and Bluetooth (VR-N76), and the Station page.

/** Presets offered on Bluetooth audio: the full ladder, vhf-fast with a small caution. */
val BluetoothPresets = listOf("vhf-fm", "hf-good", "hf-poor", "hf-weak", "hf-deep", "vhf-fast")
val UsbPresets = listOf("vhf-fast", "vhf-fm", "hf-good", "hf-poor", "hf-weak", "hf-deep", "afsk-1200")

/** The presets this link can use. Headset VOX starts on hf-poor; the radio's TNC only does afsk-1200. */
fun presetsFor(s: Status): List<String> = when {
    s.link == Link.Usb -> UsbPresets
    s.btAudio == BtAudio.Tnc1200 -> listOf("afsk-1200")
    s.btAudio == BtAudio.HeadsetVox -> listOf("hf-poor", "hf-good", "hf-weak", "hf-deep", "vhf-fm")
    else -> BluetoothPresets
}

@Composable
fun RadioScreen(radio: Radio, onStation: () -> Unit, onSettings: () -> Unit, onSetup: () -> Unit) {
    val s = radio.status
    val bt = s.link == Link.Bluetooth
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            TopBar("Radio", if (s.linked) "${s.rig} · linked" else "no radio", lead = { LogoLead() }) {
                IconBtn(Icons.Filled.Settings, "Settings", onClick = onSettings)
            }
            if (radio is LiveRadio) {
                RealRadioPanel(radio)
                return@Column
            }
            if (!s.linked) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text("Internet only: chat and email go over the hub.", fontSize = 15.sp)
                    Button(onSetup) { Text("Add a radio") }
                }
                return@Column
            }
            Column(
                Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(start = 12.dp, end = 12.dp, top = 4.dp, bottom = 12.dp),
                verticalArrangement = Arrangement.spacedBy(if (bt) 10.dp else 12.dp),
            ) {
                DeviceCard(s, onSetup)
                if (bt) BatteryCard(s)
                QsyRow(radio)
                if (bt) PresetLadder(radio) else UsbChips(radio)
                WaterfallCard(radio, if (bt) 48.dp else 176.dp, tx = radio.onAir != null || !bt, compact = bt)
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    MeterRow("IN", s.inLevel, s.inDb)
                    if (!bt) MeterRow("OUT", s.outLevel, s.outDb)
                }
                if (!bt) {
                    Row(Modifier.clickable(onClick = onStation), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        listOf("SNR" to "${s.snrDb} dB", "TX" to (radio.onAir?.let { "${it.piece}/${it.of}" } ?: "—"), "RETRY" to "${s.retry}").forEach { (k, v) ->
                            Card(Modifier.weight(1f), color = colors().surfaceContainer, padding = PaddingValues(horizontal = 12.dp, vertical = 8.dp)) {
                                Label(k); MonoText(v, Modifier.padding(top = 1.dp), size = 16.sp)
                            }
                        }
                    }
                } else {
                    TextButton(onStation) { Text("Station and on-air frames") }
                }
            }
        }
        radio.note?.let { Note(it, Modifier.align(Alignment.BottomCenter).padding(bottom = 16.dp)) }
    }
}

/** The real radio (Phase 0): only facts the station reports, one per row. */
@Composable
private fun RealRadioPanel(radio: LiveRadio) {
    val s = radio.status
    Column(Modifier.verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        val tx = radio.onAir != null
        val (word, tone) = when {
            tx -> "TRANSMITTING" to radioColors().tx
            s.radioUp -> "LINKED" to radioColors().ok
            else -> "NOT LINKED" to radioColors().warn
        }
        Card(color = colors().surfaceContainer, padding = PaddingValues(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Dot(tone); Text(word, fontSize = 22.sp, fontWeight = FontWeight.Medium, color = tone)
            }
            Text("Real radio test · ${s.rig}", Modifier.padding(top = 4.dp), fontSize = 13.sp, color = colors().onSurfaceVariant)
        }
        Column {
            radio.facts.forEach { (k, v) -> KvRow(k, v, mono = true, modifier = Modifier.padding(vertical = 5.dp)) }
        }
        MeterRow("IN", s.inLevel, s.inDb)
        MeterRow("OUT", s.outLevel, s.outDb)
    }
}

@Composable
private fun DeviceCard(s: Status, onSetup: () -> Unit) {
    val (icon: ImageVector, sub: String) = when {
        s.link == Link.Usb -> Icons.Filled.Usb to "USB audio 48 kHz · CAT serial 9600 · PTT by ${s.ptt}. Radio stays plugged in; the phone can sleep."
        s.btAudio == BtAudio.HeadsetVox -> Icons.Filled.Bluetooth to "Bluetooth headset audio · PTT by the radio's VOX · squelch 1 · VOX 4 · ${s.btAddress}"
        s.btAudio == BtAudio.Tnc1200 -> Icons.Filled.Bluetooth to "The radio's own TNC · afsk-1200 · ${s.btAddress}"
        else -> Icons.Filled.Bluetooth to "Bluetooth audio (modem73) · PTT by Bluetooth · ${s.btAddress} · squelch 0 · HT app closed"
    }
    Card(padding = PaddingValues(start = 16.dp, end = 16.dp, top = 14.dp, bottom = 12.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(Modifier.size(40.dp).clip(CircleShape).background(colors().primaryContainer), contentAlignment = Alignment.Center) {
                Icon(icon, null, tint = colors().onPrimaryContainer)
            }
            Column(Modifier.weight(1f)) {
                Text(s.rig, fontSize = 16.sp, lineHeight = 22.sp, fontWeight = FontWeight.Medium)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    val tone = if (s.radioUp) radioColors().ok else radioColors().warn
                    Dot(tone); Text(if (s.radioUp) "linked" else s.radioNote.ifEmpty { "not linked" }, fontSize = 13.sp, color = tone, maxLines = 2)
                }
            }
        }
        Text(
            sub + if (s.link == Link.Usb && s.listenInBackground) " Background listening: on." else "",
            Modifier.padding(top = 10.dp, bottom = 12.dp), fontSize = 13.sp, lineHeight = 19.sp, color = colors().onSurfaceVariant,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilledTonalButton({}, Modifier.weight(1f), contentPadding = PaddingValues(horizontal = 8.dp)) { Text("Reconnect") }
            OutlinedButton({}, Modifier.weight(1f), contentPadding = PaddingValues(horizontal = 8.dp)) { Text("Test PTT") }
            TextButton(onSetup, contentPadding = PaddingValues(horizontal = 10.dp)) {
                Text(if (s.link == Link.Usb) "Disconnect" else "Unpair", color = colors().onSurfaceVariant)
            }
        }
    }
}

/** Bluetooth: phone battery from Android; the radio's battery shows a dash until the radio reports it. */
@Composable
private fun BatteryCard(s: Status) {
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(radioColors().warnContainer).padding(horizontal = 16.dp, vertical = 12.dp)) {
        val fg = radioColors().onWarnContainer
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Icon(Icons.Filled.BatteryFull, null, Modifier.size(20.dp), tint = fg)
            Text("Battery · Bluetooth link", fontSize = 14.sp, fontWeight = FontWeight.Medium, color = fg)
        }
        Column(Modifier.padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(7.dp)) {
            BatteryRow("PHONE", s.phoneBattery)
            BatteryRow("RADIO", s.radioBattery)
        }
        Text(
            (if (s.listenInBackground) "Background listening is on (foreground service). " else "") +
                "Rest between TX blocks: handhelds drain fast when keyed repeatedly.",
            Modifier.padding(top = 9.dp), fontSize = 12.5.sp, lineHeight = 17.sp, color = fg.copy(alpha = 0.9f),
        )
    }
}

@Composable
private fun BatteryRow(label: String, percent: Int?) {
    val fg = radioColors().onWarnContainer
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Label(label, Modifier.width(52.dp), color = fg)
        Box(Modifier.weight(1f).height(8.dp).clip(RoundedCornerShape(4.dp)).background(fg.copy(alpha = 0.14f))) {
            if (percent != null) Box(Modifier.fillMaxWidth(percent / 100f).fillMaxHeight().clip(RoundedCornerShape(4.dp)).background(radioColors().ok))
        }
        MonoText(percent?.let { "$it%" } ?: "—", Modifier.width(40.dp), color = fg, size = 13.sp)
    }
}

/** Big frequency with the QSY menu (PMR446 included; the core's warning shows when picked). */
@Composable
private fun QsyRow(radio: Radio) {
    val s = radio.status
    var open by remember { mutableStateOf(false) }
    val band = Bands.firstOrNull { it.freq == s.freq }
    val warning = Core.frequencyWarning(s.freq)
        ?: if (band?.label == "PMR446") "PMR446 is licence-free: only handhelds with a fixed antenna are allowed there. Check your rules before you transmit." else null
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.Bottom) {
            Box(Modifier.weight(1f)) {
                BigFreq(s.freq, "${s.emission} · NARROW · ${if (s.link == Link.Bluetooth) "low power" else s.power}")
            }
            Box {
                FilledTonalButton({ open = true }, contentPadding = PaddingValues(start = 20.dp, end = 12.dp)) {
                    Text("QSY"); Icon(Icons.Filled.ArrowDropDown, null, Modifier.size(18.dp))
                }
                DropdownMenu(open, { open = false }) {
                    Bands.forEach { b ->
                        DropdownMenuItem(
                            text = { Row { MonoText(b.freq, size = 14.sp); Text("  ${b.label} · ${b.emission}", fontSize = 14.sp) } },
                            onClick = { open = false; radio.tune(b.freq, b.emission, b.preset) },
                        )
                    }
                }
            }
        }
        if (warning != null) Small(warning, color = radioColors().warn)
    }
}

@Composable
private fun UsbChips(radio: Radio) {
    val s = radio.status
    var open by remember { mutableStateOf(false) }
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Box {
            Chip(s.preset, selected = true, mono = true, onClick = { open = true })
            DropdownMenu(open, { open = false }) {
                presetsFor(s).forEach { p ->
                    DropdownMenuItem(text = { MonoText(p, size = 14.sp) }, onClick = { open = false; radio.update { it.copy(preset = p) } })
                }
            }
        }
        Chip(s.mode.id, content = modeColor(s.mode), border = modeColor(s.mode).copy(alpha = 0.33f))
        val tx = radio.onAir != null
        Chip(
            if (tx) "PTT TX" else "PTT RX",
            container = if (tx) radioColors().txContainer else radioColors().okContainer,
            content = if (tx) radioColors().onTxContainer else colors().onSurface,
            lead = { Dot(if (tx) radioColors().tx else radioColors().ok) },
        )
        Chip("OCC ${s.occupancy}%")
    }
}

/** Bluetooth: the full ladder; vhf-fast stays selectable with a small "untested" caution. */
@Composable
private fun PresetLadder(radio: Radio) {
    val s = radio.status
    Column {
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            presetsFor(s).forEach { p ->
                val untested = p == "vhf-fast"
                Chip(
                    p, selected = p == s.preset, mono = true,
                    border = if (untested && p != s.preset) radioColors().warn else null,
                    trail = {
                        if (untested) Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Filled.Warning, null, Modifier.size(13.dp), tint = radioColors().warn)
                            Text("untested", fontSize = 11.sp, color = radioColors().warn)
                        }
                    },
                    onClick = { radio.update { it.copy(preset = p) } },
                )
            }
        }
        val note = when (s.btAudio) {
            BtAudio.HeadsetVox -> "Headset VOX starts on hf-poor, the preset that decodes over a speaker and mic. No beacons on VOX."
            BtAudio.Tnc1200 -> "The radio's own TNC only does afsk-1200."
            BtAudio.Modem73 -> null
        }
        if (note != null) {
            Small(note, Modifier.padding(top = 6.dp))
        } else {
            Text(
                androidx.compose.ui.text.buildAnnotatedString {
                    pushStyle(androidx.compose.ui.text.SpanStyle(color = radioColors().warn)); append("vhf-fast is untested on Bluetooth."); pop()
                    append(" For mail files and fast links, USB (Digirig) is the safe choice.")
                },
                Modifier.padding(top = 6.dp), fontSize = 12.sp, lineHeight = 17.sp, color = colors().onSurfaceVariant,
            )
        }
    }
}

/** Design 15: everything about the station, and the read-only list of frames on the air. */
@Composable
fun StationScreen(radio: Radio, onBack: () -> Unit, onSettings: () -> Unit) {
    val s = radio.status
    Column(Modifier.fillMaxSize()) {
        TopBar("Station", if (s.linked) "${s.rig} · linked" else "internet only", lead = { BackLead(onBack) }) {
            IconBtn(Icons.Filled.Settings, "Settings", onClick = onSettings)
        }
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) {
            Section("Station", top = 4.dp)
            val grid = listOf(
                Triple("Call", s.call, colors().primary), Triple("Grid", s.grid, Color.Unspecified),
                Triple("Mode", s.mode.id, Color.Unspecified), Triple("PTT", if (radio.onAir != null) "TX" else "RX", Color.Unspecified),
                Triple("Queue", "${s.queue}", Color.Unspecified), Triple("Air", "${s.air}", Color.Unspecified),
                Triple("Retry", "${s.retry}", Color.Unspecified), Triple("Beacon", s.beacon, Color.Unspecified),
            )
            grid.chunked(2).forEach { pair ->
                Row(Modifier.padding(horizontal = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    pair.forEach { (k, v, col) -> KvRow(k, v, col, mono = k != "Beacon", modifier = Modifier.weight(1f)) }
                }
            }
            KvRow("GPS", s.gps, mono = true)
            KvRow("Radio", if (s.linked) s.rig else "none", radioColors().ok)
            KvRow("Hold", s.hold)
            Row(Modifier.padding(horizontal = 16.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("Hub", Modifier.weight(1f), fontSize = 14.sp, color = colors().onSurfaceVariant)
                if (s.hubUp) Chip("UP", small = true, container = radioColors().okContainer, content = colors().onSurface)
                else Chip("DOWN", small = true, container = colors().errorContainer, content = colors().onErrorContainer)
            }
            Row(Modifier.padding(start = 16.dp, end = 16.dp, top = 2.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Text("Mail GW", Modifier.width(70.dp), fontSize = 14.sp, color = colors().onSurfaceVariant)
                GatewayBars(s.mailGwBars, Modifier.weight(1f))
            }
            HorizontalDivider(Modifier.padding(horizontal = 16.dp), color = colors().outlineVariant)
            Row(Modifier.padding(start = 16.dp, end = 16.dp, top = 14.dp, bottom = 2.dp), verticalAlignment = Alignment.Bottom) {
                Text("On the air", Modifier.weight(1f), fontSize = 16.sp, fontWeight = FontWeight.Medium)
                Small("${radio.frames.size} frames")
            }
            Small(
                "Every frame this station heard or sent, newest first. Read only: to answer a station, open a direct chat.",
                Modifier.padding(start = 16.dp, end = 16.dp, bottom = 6.dp), lineHeight = 16.sp,
            )
            radio.frames.forEach { f ->
                val (icon, tint) = when (f.kind) {
                    FrameKind.Bulletin -> Icons.Filled.Tag to Color(0xFFC4B5FD)
                    FrameKind.Receipt -> Icons.Filled.DoneAll to radioColors().ok
                    FrameKind.Mail -> Icons.Filled.Mail to radioColors().warn
                    FrameKind.Direct -> Icons.Filled.Person to colors().primary
                }
                Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 16.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    Icon(icon, f.kind.words, Modifier.size(22.dp), tint = tint)
                    Column(Modifier.weight(1f)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            MonoText(f.route, color = if (f.sent) radioColors().tx else radioColors().otherCall)
                            Text(
                                " · ${f.kind.words}" + (f.snrDb?.let { " · $it dB" } ?: "") + " · ${f.via}",
                                fontSize = 12.sp, color = colors().onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis,
                            )
                        }
                        if (f.text.isNotEmpty()) Text(f.text, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    MonoText(f.time, color = colors().onSurfaceVariant, size = 11.sp)
                }
            }
        }
    }
}

@Composable
private fun KvRow(key: String, value: String, valueColor: Color = Color.Unspecified, mono: Boolean = false, modifier: Modifier = Modifier) {
    Row(modifier.heightIn(min = 40.dp).padding(horizontal = 16.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(key, Modifier.weight(1f), fontSize = 14.sp, color = colors().onSurfaceVariant)
        if (mono) MonoText(value, color = valueColor, size = 13.5.sp) else Text(value, fontSize = 14.sp, color = valueColor)
    }
}
