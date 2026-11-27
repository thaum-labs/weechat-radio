package com.weechatradio.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Reply
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.weechatradio.app.radio.HeardRow
import com.weechatradio.app.radio.LiveMail
import com.weechatradio.app.radio.LiveRadio
import com.weechatradio.app.radio.TrafficRow
import com.weechatradio.app.radio.WaitingMail
import com.weechatradio.app.radio.agoText
import com.weechatradio.app.radio.sizeText
import com.weechatradio.app.radio.whenText
import com.weechatradio.app.ui.theme.Console
import com.weechatradio.app.ui.theme.Mono

// Mail and On air on the real radio: what the station itself has, nothing
// made up. Same layout in both looks; the modem73 console draws it with lines
// and brackets.

/** The colour for a mail's state, from the station's own key. */
@Composable
private fun mailTone(key: String): Color = when (key) {
    "delivered", "sent", "received" -> radioColors().ok
    "failed", "bounced", "spam", "refused", "rejected" -> colors().error
    "tx", "keyed" -> radioColors().tx
    else -> radioColors().warn
}

@Composable
fun LiveMailScreen(radio: LiveRadio, onOpen: (String) -> Unit, onCompose: () -> Unit) {
    val m73 = console()
    val tab = radio.mailTab
    var sent by rememberSaveable { mutableStateOf(false) }
    val rows = if (sent) radio.sentMail else radio.inbox
    val waiting = if (sent) emptyList() else radio.waitingMail
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            TopBar("Mail", tab.address.ifEmpty { radio.status.mailAddress }, subMono = true) {
                if (m73) ConsoleButton("CHECK", color = Console.orange, border = Console.orange, onClick = radio::checkMail)
                else IconBtn(Icons.Filled.Sync, "Check mail", onClick = radio::checkMail)
            }
            MailState(radio)
            FolderTabs(sent, tab.unread) { sent = it }
            LazyColumn(Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(bottom = if (m73) 8.dp else 88.dp)) {
                items(waiting, key = { "w" + it.id }) { w -> WaitingRow(w) { radio.getWaiting(listOf(w.id)) } }
                items(rows, key = { it.id }) { m -> LiveMailRow(m) { onOpen(m.id) } }
                if (rows.isEmpty() && waiting.isEmpty()) item {
                    Text(
                        if (sent) "Nothing sent yet." else "No mail yet. Check mail asks the gateway over the radio what is waiting for you.",
                        Modifier.padding(24.dp), fontSize = if (m73) 13.sp else 14.sp, lineHeight = 20.sp, color = colors().onSurfaceVariant,
                    )
                }
            }
            if (m73) ConsoleButton("NEW MAIL", Modifier.fillMaxWidth().padding(16.dp).height(48.dp), onClick = onCompose)
        }
        if (!m73) {
            ExtendedFloatingActionButton(
                onClick = onCompose, modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp),
                icon = { Icon(Icons.Filled.Edit, null) }, text = { Text("New mail") },
                containerColor = colors().primaryContainer, contentColor = colors().onPrimaryContainer,
            )
        }
        radio.note?.let { Note(it, Modifier.align(Alignment.BottomCenter).padding(bottom = 88.dp)) }
    }
}

/** One line on where mail stands: why it cannot go, a Get under way, or the gateway in use. */
@Composable
private fun MailState(radio: LiveRadio) {
    val t = radio.mailTab
    val (text, tone) = when {
        t.block.isNotEmpty() -> t.block to radioColors().warn
        t.getState == "asking" -> "Asking the gateway for your mail…" to radioColors().tx
        t.getState == "receiving" -> "Receiving mail: ${t.getGot} of ${t.getTotal} parts" to radioColors().tx
        t.getState == "failed" -> t.getDetail.ifEmpty { "Getting mail did not finish." } to colors().error
        t.gatewayLine.isNotEmpty() -> "Mail gateway: ${t.gatewayLine}" to colors().onSurfaceVariant
        else -> return
    }
    Text(text, Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp), fontSize = 13.sp, lineHeight = 18.sp, color = tone)
}

@Composable
private fun FolderTabs(sent: Boolean, unread: Int, onPick: (Boolean) -> Unit) {
    val m73 = console()
    if (m73) {
        Row(Modifier.fillMaxWidth().underline(colors().outline).padding(horizontal = 16.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(false to (if (unread > 0) "Inbox $unread" else "Inbox"), true to "Sent").forEach { (isSent, label) ->
                val on = isSent == sent
                Text(
                    label, Modifier.then(if (on) Modifier.border(1.dp, colors().outline).background(colors().surfaceContainer) else Modifier).clickable { onPick(isSent) }.padding(horizontal = 10.dp, vertical = 6.dp),
                    fontSize = 14.sp, fontWeight = if (on) FontWeight.Bold else FontWeight.Normal, color = if (on) colors().onSurface else colors().onSurfaceVariant,
                )
            }
        }
        return
    }
    Row(Modifier.fillMaxWidth()) {
        listOf(false to "Inbox", true to "Sent").forEach { (isSent, label) ->
            val on = isSent == sent
            Box(Modifier.weight(1f).height(48.dp).clickable { onPick(isSent) }, contentAlignment = Alignment.Center) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(label, fontSize = 14.sp, fontWeight = FontWeight.Medium, color = if (on) colors().primary else colors().onSurfaceVariant)
                    if (!isSent && unread > 0) Badge(Modifier.padding(start = 6.dp)) { Text("$unread") }
                }
                if (on) Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth(0.6f).height(3.dp).clip(RoundedCornerShape(topStart = 3.dp, topEnd = 3.dp)).background(colors().primary))
            }
        }
    }
    HorizontalDivider(color = colors().outlineVariant)
}

/** A mail whose header came over the air: Get pulls the body through the gateway. */
@Composable
private fun WaitingRow(w: WaitingMail, onGet: () -> Unit) {
    val m73 = console()
    Row(
        Modifier.fillMaxWidth().then(if (m73) Modifier.underline(colors().outline) else Modifier).clickable(onClick = onGet).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (!m73) Icon(Icons.Filled.CloudDownload, null, tint = colors().primary)
        Column(Modifier.weight(1f)) {
            Text(w.from, fontSize = 15.sp, fontWeight = FontWeight.Medium, fontFamily = if (m73) null else Mono, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(w.subject.ifEmpty { "(no subject)" }, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("Waiting at the gateway · ${sizeText(w.bytes)} · tap to get it", fontSize = 12.sp, color = colors().onSurfaceVariant)
        }
        if (m73) Text("[GET]", fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Console.gold)
        else FilledTonalButton(onGet, contentPadding = PaddingValues(horizontal = 16.dp)) { Text("Get") }
    }
}

@Composable
private fun LiveMailRow(m: LiveMail, onClick: () -> Unit) {
    val m73 = console()
    val bold = if (m.unread) FontWeight.Bold else FontWeight.Normal
    Row(
        Modifier.fillMaxWidth().then(if (m73) Modifier.underline(colors().outline) else Modifier).clickable(onClick = onClick).heightIn(min = 72.dp).padding(horizontal = 16.dp, vertical = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (!m73) {
            Box(Modifier.size(40.dp).clip(CircleShape).background(colors().primaryContainer), contentAlignment = Alignment.Center) {
                Text(m.peer.take(1).uppercase(), color = colors().onPrimaryContainer, fontSize = 16.sp, fontWeight = FontWeight.Medium)
            }
        }
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (m.unread) Dot(if (m73) Console.cyan else radioColors().tx)
                Text(m.peer, Modifier.weight(1f), fontSize = 15.sp, fontWeight = bold, color = if (m73 && !m.peer.contains('@')) Console.peer else Color.Unspecified, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(whenText(m.ts), fontSize = 12.sp, color = colors().onSurfaceVariant)
            }
            Text(m.subject.ifEmpty { "(no subject)" }, fontSize = 14.sp, fontWeight = bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(m.body.lineSequence().firstOrNull { it.isNotBlank() }.orEmpty(), Modifier.weight(1f), fontSize = 13.sp, color = colors().onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (m.files.isNotEmpty()) {
                    if (m73) Text("+file", fontSize = 12.sp, color = colors().onSurfaceVariant)
                    else Icon(Icons.Filled.AttachFile, "has a file", Modifier.size(16.dp), tint = colors().onSurfaceVariant)
                }
                if (!m.inbox && m.label.isNotEmpty()) Text(if (m73) m.mark.ifEmpty { m.label } else m.label, fontSize = 12.sp, fontWeight = FontWeight.Medium, color = mailTone(m.key))
            }
        }
    }
}

/** One mail, read: who, when, where it stands (sent mail), the text and its files. */
@Composable
fun LiveMailOpenScreen(radio: LiveRadio, id: String, onBack: () -> Unit, onReply: (to: String, subject: String) -> Unit) {
    val m73 = console()
    var m by remember(id) { mutableStateOf((radio.inbox + radio.sentMail).firstOrNull { it.id == id }) }
    LaunchedEffect(id) { radio.openMail(id) { got -> if (got != null) m = got } }
    // Sent mail moves on (queued, on air, delivered): follow the list.
    val mail = (radio.inbox + radio.sentMail).firstOrNull { it.id == id } ?: m
    val save = rememberFileSaver()
    Column(Modifier.fillMaxSize()) {
        TopBar(if (mail?.inbox != false) "Inbox" else "Sent", lead = { BackLead(onBack) }) {
            if (mail != null) {
                if (m73) {
                    if (mail.inbox) ConsoleButton("REPLY", Modifier.padding(end = 8.dp)) { onReply(mail.from, "Re: " + mail.subject.removePrefix("Re: ")) }
                    ConsoleButton("DEL", color = Console.red, border = Console.red) { radio.deleteLiveMail(id); onBack() }
                } else {
                    if (mail.inbox) IconBtn(Icons.AutoMirrored.Filled.Reply, "Reply") { onReply(mail.from, "Re: " + mail.subject.removePrefix("Re: ")) }
                    IconBtn(Icons.Filled.Delete, "Delete") { radio.deleteLiveMail(id); onBack() }
                }
            }
        }
        if (mail == null) {
            Text("Opening…", Modifier.padding(16.dp), color = colors().onSurfaceVariant)
            return@Column
        }
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(mail.subject.ifEmpty { "(no subject)" }, fontSize = if (m73) 18.sp else 22.sp, lineHeight = if (m73) 24.sp else 28.sp, fontWeight = if (m73) FontWeight.Bold else FontWeight.Normal)
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text((if (mail.inbox) "From " else "To ") + mail.peer, fontSize = 14.sp, fontFamily = Mono)
                Text(whenText(mail.ts), fontSize = 13.sp, color = colors().onSurfaceVariant)
            }
            if (!mail.inbox) {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(if (m73) "${mail.mark} ${mail.label}".trim() else mail.label, fontSize = 14.sp, fontWeight = FontWeight.Bold, color = mailTone(mail.key))
                    if (mail.reason.isNotEmpty()) Text(mail.reason, fontSize = 13.sp, lineHeight = 18.sp, color = colors().onSurfaceVariant)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (mail.retry) { if (m73) ConsoleButton("RETRY", onClick = { radio.retryMail(id) }) else Outlined("Try again", icon = Icons.Filled.Refresh) { radio.retryMail(id) } }
                        if (mail.key == "queued" || mail.key == "locating" || mail.key == "delayed") {
                            if (m73) ConsoleButton("STOP", onClick = { radio.cancelMail(id) }) else Outlined("Stop sending", icon = Icons.Filled.Close) { radio.cancelMail(id) }
                        }
                    }
                }
            }
            if (m73) Box(Modifier.fillMaxWidth().underline(colors().outline)) else HorizontalDivider(color = colors().outlineVariant)
            Text(mail.body, fontSize = 15.sp, lineHeight = 22.sp)
            mail.files.forEach { f ->
                Row(
                    Modifier.fillMaxWidth().then(if (m73) Modifier.border(1.dp, colors().outline) else Modifier.clip(RoundedCornerShape(12.dp)).background(colors().surfaceContainerHigh))
                        .padding(start = 12.dp, end = 4.dp, top = 6.dp, bottom = 6.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    if (!m73) Icon(if (isPicture(f.name)) Icons.Filled.Image else Icons.Filled.Description, null, tint = colors().primary)
                    Column(Modifier.weight(1f)) {
                        Text(f.name, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(sizeText(f.size), fontSize = 12.sp, color = colors().onSurfaceVariant)
                    }
                    if (f.path.isNotEmpty()) {
                        if (m73) Text("[SAVE]", Modifier.clickable { save(f.path) }.padding(8.dp), fontSize = 12.sp, fontWeight = FontWeight.Bold, color = Console.green)
                        else TextButton({ save(f.path) }) { Text("Save") }
                    }
                }
            }
        }
    }
}

/** Write a mail. The station checks the address, shrinks pictures and says if it cannot go. */
@Composable
fun LiveComposeScreen(radio: LiveRadio, to: String, subject: String, onClose: () -> Unit) {
    val m73 = console()
    var toText by rememberSaveable { mutableStateOf(to) }
    var subj by rememberSaveable { mutableStateOf(subject) }
    var body by rememberSaveable { mutableStateOf("") }
    var files by remember { mutableStateOf(listOf<Picked>()) }
    var sending by remember { mutableStateOf(false) }
    val pick = rememberFilePicker { f -> files = files + f }
    val tab = radio.mailTab
    val send = {
        sending = true
        radio.composeMail(toText, subj, body, files.map { it.name to it.data }) { ok -> sending = false; if (ok) onClose() }
    }
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            TopBar("New mail", lead = { if (m73) BackLead(onClose) else IconBtn(Icons.Filled.Close, "Close", onClick = onClose) })
            Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(
                    toText, { toText = it.trim() }, Modifier.fillMaxWidth(), label = { Text("To (email address)") }, singleLine = true,
                    textStyle = TextStyle(fontFamily = Mono, fontSize = 15.sp), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
                )
                OutlinedTextField(subj, { subj = it }, Modifier.fillMaxWidth(), label = { Text("Subject") }, singleLine = true)
                OutlinedTextField(body, { body = it }, Modifier.fillMaxWidth().heightIn(min = 140.dp), label = { Text("Message") })
                files.forEach { f ->
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Icon(if (isPicture(f.name)) Icons.Filled.Image else Icons.Filled.Description, null, Modifier.size(18.dp), tint = colors().primary)
                        Text("${f.name} · ${sizeText(f.data.size)}", Modifier.weight(1f), fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        IconButton({ files = files - f }) { Icon(Icons.Filled.Close, "Remove") }
                    }
                }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    if (m73) ConsoleButton("+ FILE", onClick = pick) else Outlined("Attach", icon = Icons.Filled.AttachFile, onClick = pick)
                    Text(tab.attachLine.ifEmpty { "Pictures are made smaller to fit the air." }, Modifier.weight(1f), fontSize = 12.sp, lineHeight = 16.sp, color = colors().onSurfaceVariant)
                }
                if (tab.block.isNotEmpty()) Text(tab.block, fontSize = 13.sp, lineHeight = 18.sp, color = radioColors().warn)
            }
            val ready = toText.contains('@') && !sending
            if (m73) {
                ConsoleButton(if (sending) "QUEUING…" else "SEND", Modifier.fillMaxWidth().padding(16.dp).height(48.dp), color = if (ready) Console.green else colors().onSurfaceVariant, border = if (ready) Console.green else colors().outline) { if (ready) send() }
            } else {
                Row(Modifier.fillMaxWidth().padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    OutlinedButton(onClose, Modifier.weight(1f)) { Text("Cancel") }
                    Button(send, Modifier.weight(1.4f), enabled = ready) {
                        Icon(Icons.AutoMirrored.Filled.Send, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text(if (sending) "Queuing…" else "Send")
                    }
                }
            }
        }
        radio.note?.let { Note(it, Modifier.align(Alignment.BottomCenter).padding(bottom = 88.dp)) }
    }
}

/**
 * On the air: every frame the station heard or sent, newest first, and every
 * station heard. Read only; tap a station to chat with it.
 */
@Composable
fun OnAirScreen(radio: LiveRadio, onBack: () -> Unit, onChat: (String) -> Unit) {
    val m73 = console()
    var stations by rememberSaveable { mutableStateOf(false) }
    Column(Modifier.fillMaxSize()) {
        TopBar("On the air", if (stations) "${radio.heardAll.size} stations" else "${radio.traffic.size} frames", lead = { BackLead(onBack) })
        if (m73) {
            Row(Modifier.fillMaxWidth().underline(colors().outline).padding(horizontal = 16.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(false to "Frames", true to "Stations").forEach { (s, label) ->
                    val on = s == stations
                    Text(
                        label, Modifier.then(if (on) Modifier.border(1.dp, colors().outline).background(colors().surfaceContainer) else Modifier).clickable { stations = s }.padding(horizontal = 10.dp, vertical = 6.dp),
                        fontSize = 14.sp, fontWeight = if (on) FontWeight.Bold else FontWeight.Normal, color = if (on) colors().onSurface else colors().onSurfaceVariant,
                    )
                }
            }
        } else {
            Box(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) { Segmented(listOf("Frames", "Stations"), if (stations) 1 else 0) { stations = it == 1 } }
        }
        LazyColumn(Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(bottom = 16.dp)) {
            if (stations) {
                items(radio.heardAll) { h -> HeardLine(h) { onChat(h.call) } }
                if (radio.heardAll.isEmpty()) item { Empty("No stations heard yet. They show here as soon as the radio hears one.") }
            } else {
                items(radio.traffic, key = { it.seq }) { f -> FrameLine(f, radio.status.call, onChat) }
                if (radio.traffic.isEmpty()) item { Empty("Nothing on the air yet. Every frame the station hears or sends shows here, newest first.") }
            }
        }
    }
}

@Composable
private fun Empty(text: String) = Text(text, Modifier.padding(24.dp), fontSize = 14.sp, lineHeight = 20.sp, color = colors().onSurfaceVariant)

/** "21:33 bulletin 2E0XYZ        [rf] 1 dB", then the text. */
@Composable
private fun FrameLine(f: TrafficRow, me: String, onChat: (String) -> Unit) {
    val m73 = console()
    val other = if (f.sent) f.to else f.from
    val canChat = other.isNotEmpty() && !other.startsWith("#") && !other.startsWith("&") && !other.equals(me, true) && !other.contains('@')
    val kindColor = when (f.kind) {
        "receipt" -> radioColors().ok
        "mail" -> radioColors().warn
        "beacon" -> colors().onSurfaceVariant
        else -> if (m73) Console.lilac else colors().primary
    }
    Column(
        Modifier.fillMaxWidth().then(if (m73) Modifier.underline(colors().outline) else Modifier)
            .then(if (canChat) Modifier.clickable { onChat(other) } else Modifier).padding(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(whenText(f.ts), fontSize = 12.sp, fontFamily = Mono, color = colors().onSurfaceVariant)
            Text(f.kind, fontSize = 13.sp, color = kindColor)
            Text(
                listOf(f.from, f.to).filter { it.isNotEmpty() }.joinToString(" → "), Modifier.weight(1f),
                fontSize = 13.sp, fontFamily = Mono, color = if (m73) Console.peer else radioColors().otherCall, maxLines = 1, overflow = TextOverflow.Ellipsis,
            )
            if (f.sent) Text("sent", fontSize = 12.sp, fontWeight = FontWeight.Medium, color = radioColors().tx)
            else Text(
                (if (f.via == "rf") "[rf]" else "[${f.via}]") + (f.snr?.let { " ${it.toInt()} dB" } ?: ""),
                fontSize = 12.sp, fontFamily = Mono, color = if (f.via == "rf") (if (m73) Console.gold else radioColors().tx) else colors().onSurfaceVariant,
            )
        }
        if (f.text.isNotEmpty()) Text(f.text, fontSize = 14.sp, lineHeight = 19.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun HeardLine(h: HeardRow, onChat: () -> Unit) {
    val m73 = console()
    Row(
        Modifier.fillMaxWidth().then(if (m73) Modifier.underline(colors().outline) else Modifier).clickable(onClick = onChat).heightIn(min = 56.dp).padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (!m73) Icon(if (h.medium == "rf") Icons.Filled.SettingsInputAntenna else Icons.Filled.Cloud, null, Modifier.size(20.dp), tint = if (h.medium == "rf") radioColors().tx else colors().primary)
        Column(Modifier.weight(1f)) {
            Text(h.call, fontSize = 15.sp, fontFamily = Mono, fontWeight = FontWeight.Medium, color = if (m73) Console.peer else radioColors().otherCall)
            Text(
                listOf(if (h.medium == "rf") "by radio" else "over the hub", h.grid, h.band, h.mode).filter { it.isNotEmpty() }.joinToString(" · "),
                fontSize = 12.sp, color = colors().onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis,
            )
        }
        Column(horizontalAlignment = Alignment.End) {
            Text(agoText(h.lastHeard), fontSize = 12.sp, color = colors().onSurfaceVariant)
            h.snr?.let { Text("${it.toInt()} dB", fontSize = 12.sp, fontFamily = Mono, color = colors().onSurfaceVariant) }
        }
    }
}
