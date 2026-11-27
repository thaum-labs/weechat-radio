package com.weechatradio.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.weechatradio.app.radio.Channel
import com.weechatradio.app.radio.ChannelKind
import com.weechatradio.app.radio.Line
import com.weechatradio.app.radio.LineKind
import com.weechatradio.app.radio.LiveRadio
import com.weechatradio.app.radio.Mark
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.radio.Via
import com.weechatradio.app.ui.theme.Console
import com.weechatradio.app.ui.theme.Mono

// Chats tab (design canvas "Android app themes", approved 2026-11-27): the list
// of chats, then one conversation. Stock bubbles, or console lines in modem73.

@Composable
fun ChatsScreen(radio: Radio, onOpen: (String) -> Unit, onNew: () -> Unit) {
    val m73 = console()
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            TopBar("Chats") { if (m73) ConsoleButton("+ NEW", onClick = onNew) }
            LazyColumn(Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(bottom = 88.dp)) {
                items(radio.channels, key = { it.name }) { ch -> if (m73) ConsoleChatRow(ch) { onOpen(ch.name) } else ChatRow(ch) { onOpen(ch.name) } }
            }
        }
        if (!m73) {
            ExtendedFloatingActionButton(
                onClick = onNew, modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp),
                icon = { Icon(Icons.Filled.Edit, null) }, text = { Text("New chat") },
                containerColor = colors().primaryContainer, contentColor = colors().onPrimaryContainer,
            )
        }
        radio.note?.let { Note(it, Modifier.align(Alignment.BottomCenter).padding(bottom = 16.dp)) }
    }
}

@Composable
private fun ChatRow(ch: Channel, onClick: () -> Unit) {
    val room = ch.kind != ChannelKind.Direct
    val unread = ch.unread > 0
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).heightIn(min = 72.dp).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Box(
            Modifier.size(40.dp).clip(CircleShape).background(if (room) colors().secondaryContainer else colors().primaryContainer),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                if (room) "#" else ch.name.replace(Regex("^[A-Z0-9]*[0-9]"), "").take(1).ifEmpty { ch.name.take(1) },
                fontSize = 16.sp, fontWeight = FontWeight.Medium,
                color = if (room) colors().onSecondaryContainer else colors().onPrimaryContainer,
            )
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(ch.name, fontSize = 16.sp, lineHeight = 24.sp, fontFamily = if (room) null else Mono, fontWeight = if (unread) FontWeight.Bold else FontWeight.Normal, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(ch.summary.ifEmpty { "Nothing yet" }, fontSize = 14.sp, lineHeight = 20.sp, color = if (unread) colors().onSurface else colors().onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(ch.time, fontSize = 12.sp, lineHeight = 16.sp, color = colors().onSurfaceVariant)
            if (unread) Badge(containerColor = colors().primary, contentColor = colors().onPrimary) { Text("${ch.unread}") }
        }
    }
}

@Composable
private fun ConsoleChatRow(ch: Channel, onClick: () -> Unit) {
    val room = ch.kind != ChannelKind.Direct
    val unread = ch.unread > 0
    Column(
        Modifier.fillMaxWidth().underline(colors().outline).clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(ch.name, Modifier.weight(1f), fontSize = 14.sp, fontWeight = FontWeight.Bold, color = if (room) colors().onSurface else Console.peer, maxLines = 1)
            Text(ch.time, fontSize = 12.sp, color = colors().onSurfaceVariant)
            if (unread) Text("[${ch.unread}]", fontSize = 12.sp, fontWeight = FontWeight.Bold, color = Console.gold)
        }
        Text(ch.summary.ifEmpty { "nothing yet" }, fontSize = 13.sp, lineHeight = 18.sp, color = if (unread) colors().onSurface else colors().onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** One chat: a room like #bulletin, or a direct chat with a callsign. */
@Composable
fun ConversationScreen(radio: Radio, name: String, onBack: () -> Unit) {
    LaunchedEffect(name) { if (radio.channel != name) radio.openChannel(name) }
    val s = radio.status
    val m73 = console()
    val room = name.startsWith("#")
    // Files go in a direct chat on the real radio: a picture is made small enough for the air first.
    val live = radio as? LiveRadio
    val attach = if (live != null && !room && !name.startsWith("&")) rememberFilePicker { f -> sendChatFile(live, name, f) } else null
    val save = rememberFileSaver()
    val list = rememberLazyListState()
    val how = if (s.linked) "by radio" else "over the hub"
    val sub = if (m73) (if (room) "everyone · $how" else "direct · $how") else if (room) "Everyone can read · $how" else "Direct chat · $how"
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            TopBar(name, sub, lead = { BackLead(onBack) })
            if (m73) Box(Modifier.fillMaxWidth().underline(colors().outline))
            val lines = radio.lines
            // The newest line is at the bottom: follow it when a line comes in or changes.
            LaunchedEffect(lines.size, lines.lastOrNull()?.id) { if (lines.isNotEmpty()) list.animateScrollToItem(0) }
            if (lines.isEmpty()) {
                Box(Modifier.weight(1f).fillMaxWidth().padding(24.dp), contentAlignment = if (m73) Alignment.CenterStart else Alignment.Center) {
                    Text(
                        if (room) "Nothing in $name yet. What you send here goes out to everyone listening." else "No messages with $name yet.",
                        fontSize = if (m73) 13.sp else 14.sp, lineHeight = 20.sp, color = colors().onSurfaceVariant, textAlign = if (m73) TextAlign.Start else TextAlign.Center,
                    )
                }
            } else {
                LazyColumn(
                    Modifier.weight(1f).fillMaxWidth(), state = list, reverseLayout = true,
                    contentPadding = PaddingValues(start = if (m73) 16.dp else 12.dp, end = if (m73) 16.dp else 12.dp, top = 8.dp, bottom = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(if (m73) 14.dp else 10.dp),
                ) {
                    items(lines.asReversed(), key = { it.id }) {
                        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            if (m73) ConsoleLine(it, s.call) else LineRow(it)
                            if (it.filePath.isNotEmpty() || it.progress.isNotEmpty()) FileUnder(it, m73, save)
                        }
                    }
                }
            }
            if (m73) ConsoleComposer("message $name", attach, radio::send) else Composer("Message $name", attach, radio::send)
        }
        radio.note?.let { Note(it, Modifier.align(Alignment.BottomCenter).padding(bottom = 72.dp)) }
    }
}

@Composable
private fun LineRow(l: Line) {
    when (l.kind) {
        LineKind.Message -> Bubble(l)
        else -> Text(l.text, Modifier.fillMaxWidth(), fontSize = 12.sp, color = colors().onSurfaceVariant, textAlign = TextAlign.Center)
    }
}

/** What happened to a line you sent, in words, with its colour. */
@Composable
private fun sentStatus(l: Line): Pair<String, Color> = when {
    l.sendingPiece > 0 -> "sending ${l.sendingPiece} of ${l.pieces}" to radioColors().tx
    else -> when (l.mark) {
        Mark.Queued -> "waiting to send" to radioColors().warn
        Mark.Sent -> (if (l.keyed > 1) "sent ×${l.keyed}" else "sent") to colors().onPrimaryContainer
        Mark.Relayed -> "heard again" to colors().onPrimaryContainer
        Mark.Gateway -> "at the gateway" to colors().onPrimaryContainer
        Mark.Delivered -> "delivered" to radioColors().ok
        Mark.AllMembers -> "everyone got it" to radioColors().ok
        Mark.Refused -> "not delivered" to colors().error
        null -> "" to colors().onPrimaryContainer
    }
}

@Composable
private fun Bubble(l: Line) {
    if (!l.mine) {
        Row(Modifier.fillMaxWidth()) {
            Column(
                Modifier.fillMaxWidth(0.82f).wrapContentWidth(Alignment.Start).clip(RoundedCornerShape(4.dp, 18.dp, 18.dp, 18.dp))
                    .background(colors().surfaceContainerHigh).padding(start = 12.dp, end = 12.dp, top = 8.dp, bottom = 6.dp),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                MonoText(l.from, color = radioColors().otherCall, size = 12.sp, weight = FontWeight.Medium)
                Text(l.text, fontSize = 15.sp, lineHeight = 21.sp)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    Icon(if (l.via == Via.Internet) Icons.Filled.Cloud else Icons.Filled.SettingsInputAntenna, null, Modifier.size(12.dp), tint = colors().onSurfaceVariant)
                    Text(listOfNotNull(l.time, l.snrDb?.let { "$it dB" }, if (l.pieces > 1) "${l.piecesHeard} of ${l.pieces} parts" else null).joinToString(" · "), fontSize = 11.sp, lineHeight = 16.sp, color = colors().onSurfaceVariant)
                }
            }
        }
        return
    }
    val (status, tone) = sentStatus(l)
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
        Column(
            Modifier.fillMaxWidth(0.82f).wrapContentWidth(Alignment.End).clip(RoundedCornerShape(18.dp, 4.dp, 18.dp, 18.dp))
                .background(colors().primaryContainer).padding(start = 12.dp, end = 12.dp, top = 8.dp, bottom = 6.dp),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Text(l.text, fontSize = 15.sp, lineHeight = 21.sp, color = colors().onPrimaryContainer)
            if (l.sendingPiece > 0 && l.pieces > 1) {
                Row(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    repeat(l.pieces) { i ->
                        Box(Modifier.weight(1f).height(4.dp).clip(RoundedCornerShape(2.dp)).background(if (i < l.sendingPiece) radioColors().ok else colors().onPrimaryContainer.copy(alpha = 0.3f)))
                    }
                }
            }
            Row(Modifier.align(Alignment.End), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(listOf(l.time, status).filter { it.isNotEmpty() }.joinToString(" · "), fontSize = 11.sp, lineHeight = 16.sp, fontWeight = FontWeight.Medium, color = tone)
                when (l.mark) {
                    Mark.Sent -> Icon(Icons.Filled.Done, null, Modifier.size(14.dp), tint = tone)
                    Mark.Delivered, Mark.AllMembers -> Icon(Icons.Filled.DoneAll, null, Modifier.size(16.dp), tint = tone)
                    else -> {}
                }
            }
        }
    }
}

/** A console line: "22:41 [XX0BBB] [rf] -4dB", then the text. */
@Composable
private fun ConsoleLine(l: Line, me: String) {
    if (l.kind != LineKind.Message) {
        Text("-- ${l.text} --", fontSize = 12.sp, color = colors().onSurfaceVariant)
        return
    }
    val (tag, tagColor, textColor) = when {
        !l.mine -> Triple(
            if (l.via == Via.Internet) "[net]" else "[rf]" + (l.snrDb?.let { " ${it}dB" } ?: ""),
            if (l.via == Via.Internet) Console.cyan else Console.gold, colors().onSurface,
        )
        l.sendingPiece > 0 -> Triple("[>>] part ${l.sendingPiece}/${l.pieces}", Console.orange, Console.pending)
        else -> when (l.mark) {
            Mark.Queued -> Triple("[..] waiting", Console.pending, Console.pending)
            Mark.Sent -> Triple(if (l.keyed > 1) "[tx] sent x${l.keyed}" else "[tx] sent", Console.pending, colors().onSurface)
            Mark.Relayed -> Triple("[rl] heard again", Console.pending, colors().onSurface)
            Mark.Gateway -> Triple("[gw] at gateway", Console.cyan, colors().onSurface)
            Mark.Delivered -> Triple("[ok]", Console.green, colors().onSurface)
            Mark.AllMembers -> Triple("[all]", Console.green, colors().onSurface)
            Mark.Refused -> Triple("[no] not delivered", Console.red, colors().onSurface)
            null -> Triple("", Console.pending, colors().onSurface)
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(l.time, fontSize = 12.sp, color = colors().onSurfaceVariant)
            Text("[${if (l.mine) me else l.from}]", Modifier.weight(1f), fontSize = 13.sp, fontWeight = FontWeight.Bold, color = if (l.mine) Console.me else Console.peer, maxLines = 1)
            Text(tag, fontSize = 12.sp, fontWeight = FontWeight.Medium, color = tagColor, maxLines = 1)
        }
        Text(l.text, fontSize = 14.sp, lineHeight = 20.sp, color = textColor)
        if (l.mine && l.sendingPiece > 0 && l.pieces > 1) {
            val bar = (1..l.pieces).joinToString("|") { if (it < l.sendingPiece) "##" else if (it == l.sendingPiece) ">>" else ".." }
            Text("[$bar]", fontSize = 12.sp, letterSpacing = 1.sp, color = Console.orange)
        }
    }
}

@Composable
private fun Composer(hint: String, onAttach: (() -> Unit)?, onSend: (String) -> Unit) {
    var text by rememberSaveable { mutableStateOf("") }
    val send = { if (text.isNotBlank()) { onSend(text); text = "" } }
    Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, top = 8.dp, bottom = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (onAttach != null) IconButton(onAttach, Modifier.size(40.dp)) { Icon(Icons.Filled.AttachFile, "Send a picture or file", tint = colors().onSurfaceVariant) }
        Box(Modifier.weight(1f).height(48.dp).clip(RoundedCornerShape(24.dp)).background(colors().surfaceContainerHigh).padding(horizontal = 20.dp), contentAlignment = Alignment.CenterStart) {
            if (text.isEmpty()) Text(hint, color = colors().onSurfaceVariant, fontSize = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            BasicTextField(
                text, { text = it }, Modifier.fillMaxWidth(), singleLine = true,
                textStyle = TextStyle(fontSize = 16.sp, color = colors().onSurface),
                cursorBrush = SolidColor(colors().primary),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                keyboardActions = KeyboardActions(onSend = { send() }),
            )
        }
        FilledIconButton(send, Modifier.size(48.dp)) { Icon(Icons.AutoMirrored.Filled.Send, "Send") }
    }
}

/** The console prompt: "$ [message] SEND". */
@Composable
private fun ConsoleComposer(hint: String, onAttach: (() -> Unit)?, onSend: (String) -> Unit) {
    var text by rememberSaveable { mutableStateOf("") }
    val send = { if (text.isNotBlank()) { onSend(text); text = "" } }
    val line = colors().outline
    Row(
        Modifier.fillMaxWidth().drawBehind { drawLine(line, Offset(0f, 0f), Offset(size.width, 0f), 1.dp.toPx()) }
            .padding(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (onAttach != null) Text("+F", Modifier.clickable(onClick = onAttach).padding(vertical = 8.dp), fontSize = 14.sp, fontWeight = FontWeight.Bold, color = Console.gold)
        else Text("$", fontSize = 16.sp, fontWeight = FontWeight.Bold, color = Console.green)
        Box(Modifier.weight(1f).height(40.dp).border(1.dp, line).background(colors().surfaceContainer).padding(horizontal = 10.dp), contentAlignment = Alignment.CenterStart) {
            if (text.isEmpty()) Text(hint, color = colors().onSurfaceVariant, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            BasicTextField(
                text, { text = it }, Modifier.fillMaxWidth(), singleLine = true,
                textStyle = TextStyle(fontFamily = Mono, fontSize = 14.sp, color = colors().onSurface),
                cursorBrush = SolidColor(Console.green),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                keyboardActions = KeyboardActions(onSend = { send() }),
            )
        }
        ConsoleButton("SEND", Modifier.height(40.dp), color = Console.green, border = Console.green, onClick = send)
    }
}

/** The biggest file a chat carries (the station's own limit). */
private const val CHAT_FILE_MAX = 8192

/** Send a picked file in a direct chat; a picture is shrunk to fit first. */
private fun sendChatFile(radio: LiveRadio, to: String, f: Picked) {
    if (f.data.size <= CHAT_FILE_MAX) return radio.sendFile(to, f.name, f.data)
    if (!isPicture(f.name)) return radio.say("${f.name} is ${com.weechatradio.app.radio.sizeText(f.data.size)}. A chat carries files up to 8 KB: send it by mail instead.")
    val small = shrinkPicture(f.data, CHAT_FILE_MAX) ?: return radio.say("Could not make ${f.name} small enough for the air.")
    radio.sendFile(to, f.name.substringBeforeLast('.') + ".jpg", small)
}

/** Under a file line: the picture itself, Save, or how far it has come. */
@Composable
private fun FileUnder(l: Line, m73: Boolean, save: (String) -> Unit) {
    val side = if (l.mine) Alignment.End else Alignment.Start
    Column(Modifier.fillMaxWidth(), horizontalAlignment = if (m73) Alignment.Start else side, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (l.progress.isNotEmpty()) Text(l.progress, fontSize = 12.sp, fontFamily = Mono, color = if (m73) Console.orange else colors().onSurfaceVariant)
        if (l.filePath.isEmpty()) return@Column
        if (isPicture(l.filePath)) {
            val pic = remember(l.filePath) { runCatching { android.graphics.BitmapFactory.decodeFile(l.filePath)?.asImageBitmap() }.getOrNull() }
            if (pic != null) androidx.compose.foundation.Image(pic, l.text, Modifier.widthIn(max = 240.dp).heightIn(max = 240.dp).clip(RoundedCornerShape(if (m73) 0.dp else 12.dp)))
        }
        if (m73) Text("[SAVE]", Modifier.clickable { save(l.filePath) }.padding(vertical = 4.dp), fontSize = 12.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp, color = Console.green)
        else TextButton({ save(l.filePath) }) { Icon(Icons.Filled.Download, null, Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Save a copy") }
    }
}

/** New chat: type a callsign, or tap a station heard by radio. */
@Composable
fun NewChatSheet(radio: Radio, onClose: () -> Unit, onOpen: (String) -> Unit) {
    var call by rememberSaveable { mutableStateOf("") }
    val open = { if (call.isNotBlank()) onOpen(call.trim().uppercase()) }
    Sheet(onClose) {
        Text("New chat", Modifier.padding(start = 24.dp, end = 24.dp, bottom = 8.dp), fontSize = 22.sp, lineHeight = 28.sp)
        Row(Modifier.padding(horizontal = 24.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
                call, { call = it.uppercase() }, Modifier.weight(1f), label = { Text("Callsign") }, singleLine = true,
                textStyle = TextStyle(fontFamily = Mono, fontSize = 15.sp),
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Characters, imeAction = ImeAction.Go),
                keyboardActions = KeyboardActions(onGo = { open() }),
            )
            Button(open) { Text("Open") }
        }
        if (radio.heard.isNotEmpty()) {
            Text("Heard by radio", Modifier.padding(start = 24.dp, end = 24.dp, top = 16.dp, bottom = 4.dp), fontSize = 14.sp, fontWeight = FontWeight.Medium, color = colors().onSurfaceVariant)
            Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                radio.heard.forEach { h ->
                    Chip(h.call, mono = true, content = radioColors().otherCall, trail = { MonoText("${h.snrDb} dB", color = colors().onSurfaceVariant, size = 12.sp) }, onClick = { onOpen(h.call) })
                }
            }
        }
    }
}

/** A 1 dp line under a row. */
fun Modifier.underline(color: Color) = drawBehind { drawLine(color, Offset(0f, size.height), Offset(size.width, size.height), 1.dp.toPx()) }
