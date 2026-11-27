package com.weechatradio.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Help
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.weechatradio.app.radio.ChannelKind
import com.weechatradio.app.radio.Line
import com.weechatradio.app.radio.LineKind
import com.weechatradio.app.radio.Mark
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.radio.Via
import com.weechatradio.app.ui.theme.Mono

// Design screens 01–04: chat, chat on air, channels sheet, marks key sheet.

@Composable
fun ChatScreen(radio: Radio, onChannels: () -> Unit, onKey: () -> Unit, onSettings: () -> Unit) {
    val s = radio.status
    val air = radio.onAir
    val ch = radio.channels.firstOrNull { it.name == radio.channel }
    val sub = when (ch?.kind) {
        ChannelKind.Closed -> "closed room · [all] receipts"
        ChannelKind.Direct -> "direct · ${if (s.linked) "radio + hub" else "hub"}"
        else -> "everyone can read · ${if (s.linked) "radio + hub" else "hub"}"
    }
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            TopBar(
                radio.channel, sub, lead = { LogoLead() }, onTitle = onChannels,
                titleTrail = { Icon(Icons.Filled.ArrowDropDown, "Channels", tint = colors().onSurfaceVariant) },
            ) {
                IconBtn(Icons.AutoMirrored.Filled.Help, "What the marks mean", onClick = onKey)
                IconBtn(Icons.Filled.Settings, "Settings", onClick = onSettings)
            }
            if (air != null) OnAirBanner(s, air, radio::stopTx)
            StationStrip(s, tx = air != null)
            val lines = radio.lines
            LazyColumn(Modifier.weight(1f).fillMaxWidth(), reverseLayout = true, contentPadding = PaddingValues(bottom = 6.dp)) {
                items(lines.asReversed(), key = { it.id }) { LineRow(it, s.showSnr) }
            }
            Composer(if (radio.channel.startsWith("#")) "Message ${radio.channel} or /command" else "Message ${radio.channel}", radio::send)
        }
        radio.note?.let { Note(it, Modifier.align(Alignment.BottomCenter).padding(bottom = 72.dp), "UNDO".takeIf { _ -> it.startsWith("Queued") }) { radio.undoQueued() } }
    }
}

@Composable
private fun LineRow(l: Line, showSnr: Boolean) {
    when (l.kind) {
        LineKind.Day -> Box(Modifier.fillMaxWidth().padding(top = 10.dp, bottom = 2.dp), contentAlignment = Alignment.Center) {
            Chip(l.text, small = true, container = colors().surfaceContainerHigh)
        }
        LineKind.Note -> Text(l.text, Modifier.fillMaxWidth().padding(top = 8.dp), fontSize = 12.sp, color = colors().onSurfaceVariant, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
        LineKind.Rest -> Box(Modifier.fillMaxWidth().padding(top = 8.dp), contentAlignment = Alignment.Center) {
            Chip(l.text, small = true, icon = Icons.Filled.Schedule, container = colors().surfaceContainerHigh)
        }
        LineKind.Message -> Bubble(l, showSnr)
    }
}

@Composable
private fun Bubble(l: Line, showSnr: Boolean) {
    val bg = if (l.mine) colors().primaryContainer else colors().surfaceContainerHigh
    val fg = if (l.mine) colors().onPrimaryContainer else colors().onSurface
    val shape = if (l.mine) RoundedCornerShape(18.dp, 18.dp, 4.dp, 18.dp) else RoundedCornerShape(18.dp, 18.dp, 18.dp, 4.dp)
    Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, top = 6.dp), horizontalArrangement = if (l.mine) Arrangement.End else Arrangement.Start) {
        Column(Modifier.widthIn(max = 330.dp).clip(shape).background(bg).padding(start = 12.dp, end = 12.dp, top = 8.dp, bottom = 6.dp)) {
            if (!l.mine) MonoText(l.from, Modifier.padding(bottom = 2.dp), color = radioColors().otherCall, size = 12.sp, weight = FontWeight.Medium)
            Text(l.text, color = fg, fontSize = 14.5.sp, lineHeight = 20.sp)
            if (l.sendingPiece > 0) {
                Row(Modifier.padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(3.dp)) {
                        repeat(l.pieces) { i ->
                            Box(Modifier.weight(1f).height(4.dp).clip(RoundedCornerShape(2.dp)).background(if (i < l.sendingPiece) radioColors().tx else fg.copy(alpha = 0.18f)))
                        }
                    }
                    Text("sending ${l.sendingPiece} of ${l.pieces}", fontSize = 11.sp, color = radioColors().tx, fontWeight = FontWeight.Medium)
                }
            }
            Row(Modifier.align(Alignment.End).padding(top = 3.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(l.time, fontSize = 11.sp, color = colors().onSurfaceVariant)
                when (l.via) {
                    Via.Radio -> Icon(Icons.Filled.SettingsInputAntenna, "by radio", Modifier.size(14.dp), tint = radioColors().tx)
                    Via.Internet -> Icon(Icons.Filled.Cloud, "over the internet", Modifier.size(14.dp), tint = colors().primary)
                    null -> {}
                }
                if (showSnr && l.snrDb != null) MonoText("${l.snrDb} dB", color = colors().onSurfaceVariant, size = 10.5.sp)
                if (l.pieces > 1 && !l.mine) PiecesChip(l.piecesHeard, l.pieces)
                if (l.mine) MonoText(if (l.mark == Mark.Queued && l.keyed == 0) "queued" else "×${l.keyed}", color = colors().onSurfaceVariant, size = 10.5.sp)
                l.mark?.let { MarkIcon(it) }
            }
        }
    }
}

/** "3 of 3": pieces of one long message that arrived. */
@Composable
fun PiecesChip(heard: Int, of: Int, text: String = "$heard of $of") {
    Chip(
        text, small = true, container = colors().surfaceContainerHighest,
        content = if (heard >= of) radioColors().ok else radioColors().warn, icon = Icons.Filled.Group,
    )
}

@Composable
private fun Composer(hint: String, onSend: (String) -> Unit) {
    var text by rememberSaveable { mutableStateOf("") }
    val send = { onSend(text); text = "" }
    Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, top = 8.dp, bottom = 10.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(Modifier.weight(1f).height(48.dp).clip(RoundedCornerShape(24.dp)).background(colors().surfaceContainerHigh).padding(horizontal = 16.dp), contentAlignment = Alignment.CenterStart) {
            if (text.isEmpty()) Text(hint, color = colors().onSurfaceVariant, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            BasicTextField(
                text, { text = it }, Modifier.fillMaxWidth(), singleLine = true,
                textStyle = TextStyle(fontSize = 15.sp, color = colors().onSurface),
                cursorBrush = SolidColor(colors().primary),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                keyboardActions = KeyboardActions(onSend = { send() }),
            )
        }
        FilledIconButton(send, Modifier.size(48.dp)) { Icon(Icons.AutoMirrored.Filled.Send, "Send") }
    }
}

/** Design 03: channels sheet with rooms, direct chats and stations heard. */
@Composable
fun ChannelsSheet(radio: Radio, onClose: () -> Unit) {
    Sheet(onClose) {
        Row(Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Channels", fontSize = 22.sp, lineHeight = 28.sp, modifier = Modifier.weight(1f))
            Chip(radio.status.mode.id, small = true, container = colors().primaryContainer, content = colors().onPrimaryContainer)
        }
        val pick = { name: String -> radio.openChannel(name); onClose() }
        radio.channels.filter { it.kind != ChannelKind.Direct }.forEach { ch ->
            ChannelRow(if (ch.kind == ChannelKind.Room) "#" else "group", ch.name, ch.summary, ch.time, ch.unread, ch.name == radio.channel) { pick(ch.name) }
        }
        Section("Direct", top = 12.dp)
        radio.channels.filter { it.kind == ChannelKind.Direct }.forEach { ch ->
            ChannelRow(ch.name.take(1), ch.name, ch.summary, ch.time, ch.unread, ch.name == radio.channel) { pick(ch.name) }
        }
        Label("Heard in the last 30 min", Modifier.padding(start = 24.dp, end = 24.dp, top = 10.dp, bottom = 2.dp))
        Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            radio.heard.forEach { h ->
                val col = when {
                    h.snrDb >= 8 -> radioColors().ok
                    h.snrDb >= 4 -> radioColors().warn
                    else -> colors().error
                }
                Chip(h.call, mono = true, content = radioColors().otherCall, trail = { MonoText("${h.snrDb} dB", color = col, size = 12.sp) }, onClick = { pick(h.call) })
            }
        }
        var call by rememberSaveable { mutableStateOf("") }
        Row(Modifier.padding(start = 24.dp, end = 24.dp, top = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
                call, { call = it.uppercase() }, Modifier.weight(1f), label = { Text("Direct chat") }, placeholder = { Text("Callsign") },
                singleLine = true, textStyle = TextStyle(fontFamily = Mono, fontSize = 15.sp),
            )
            FilledTonalButton({ if (call.isNotBlank()) pick(call.trim()) }) { Text("Open") }
            OutlinedButton({}, contentPadding = PaddingValues(start = 10.dp, end = 14.dp)) {
                Icon(Icons.Filled.Add, null, Modifier.size(18.dp)); Text("Room")
            }
        }
    }
}

@Composable
private fun ChannelRow(lead: String, name: String, summary: String, time: String, unread: Int, selected: Boolean, onClick: () -> Unit) {
    val callsign = !name.startsWith("#")
    Row(
        Modifier.fillMaxWidth().padding(horizontal = if (selected) 8.dp else 0.dp)
            .clip(RoundedCornerShape(28.dp)).background(if (selected) colors().secondaryContainer else Color.Transparent)
            .clickable(onClick = onClick).heightIn(min = 64.dp).padding(horizontal = if (selected) 16.dp else 24.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Box(Modifier.size(40.dp).clip(CircleShape).background(colors().primaryContainer), contentAlignment = Alignment.Center) {
            when (lead) {
                "#" -> Icon(Icons.Filled.Tag, null, Modifier.size(22.dp), tint = colors().onPrimaryContainer)
                "group" -> Icon(Icons.Filled.Group, null, Modifier.size(22.dp), tint = colors().onPrimaryContainer)
                else -> Text(lead, color = colors().onPrimaryContainer, fontSize = 16.sp, fontWeight = FontWeight.Medium)
            }
        }
        Column(Modifier.weight(1f)) {
            if (callsign) MonoText(name, color = radioColors().otherCall, size = 16.sp) else Text(name, fontSize = 16.sp, lineHeight = 24.sp)
            Text(summary, fontSize = 14.sp, color = colors().onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(time, fontSize = 12.sp, color = colors().onSurfaceVariant)
            if (unread > 0) Badge { Text("$unread") }
        }
    }
}

/** Design 04: what the receipt marks mean. */
@Composable
fun KeySheet(onClose: () -> Unit) {
    Sheet(onClose) {
        Row(Modifier.fillMaxWidth().padding(start = 24.dp, end = 12.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("What the marks mean", fontSize = 22.sp, lineHeight = 28.sp, modifier = Modifier.weight(1f))
            IconBtn(Icons.Filled.Close, "Close", onClick = onClose)
        }
        Column(Modifier.verticalScrollIfNeeded()) {
            KeyRow({ MarkIcon(Mark.Queued, 22.dp) }, "Waiting to go out")
            KeyRow({ MarkIcon(Mark.Sent, 22.dp) }, "Sent, no receipt yet")
            KeyRow({ MonoText("×2", color = colors().onSurfaceVariant, size = 12.sp) }, "Times it was keyed (×2)")
            KeyRow({ MarkIcon(Mark.Relayed, 22.dp) }, "Heard again on the radio (a relay or an echo), not a receipt")
            KeyRow({ MarkIcon(Mark.Gateway, 22.dp) }, "A gateway put it on the hub")
            KeyRow({ MarkIcon(Mark.Delivered, 22.dp) }, "Delivered: they sent a receipt")
            KeyRow({ MarkIcon(Mark.AllMembers, 22.dp) }, "Every member of the closed room got it")
            KeyRow({ MarkIcon(Mark.Refused, 22.dp) }, "Refused, or no receipt after the last try")
            KeyRow({ Icon(Icons.Filled.SettingsInputAntenna, null, Modifier.size(22.dp), tint = radioColors().tx) }, "Reached you by radio (rf)")
            KeyRow({ Icon(Icons.Filled.Cloud, null, Modifier.size(22.dp), tint = colors().primary) }, "Reached you over the internet (net) or the local network (lan)")
            KeyRow({ MonoText("-4 dB", color = colors().onSurfaceVariant, size = 12.sp) }, "Signal-to-noise of a radio line (/snr off hides it)")
            KeyRow({ PiecesChip(2, 3, "2/3") }, "Pieces of one long message on the air; [1/3] tags are stripped")
        }
    }
}

@Composable
private fun KeyRow(lead: @Composable () -> Unit, text: String) {
    Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 16.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.width(72.dp)) { lead() }
        Text(text, fontSize = 14.sp, lineHeight = 19.sp)
    }
}

@Composable
fun Modifier.verticalScrollIfNeeded(): Modifier = this.then(Modifier.verticalScroll(rememberScrollState()))
