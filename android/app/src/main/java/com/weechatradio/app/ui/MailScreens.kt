package com.weechatradio.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.weechatradio.app.core.Core
import com.weechatradio.app.radio.Attachment
import com.weechatradio.app.radio.Folder
import com.weechatradio.app.radio.Link
import com.weechatradio.app.radio.Mail
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.ui.theme.Mono

// Design screens 05–09: mail list, open (inbox and sent), compose (USB and Bluetooth), Email settings sheet.

@Composable
private fun avatarColors(peer: String): Pair<Color, Color> = when (peer.first().lowercaseChar()) {
    'w' -> radioColors().okContainer to colors().onSurface
    'x' -> if (peer.startsWith("XX0B")) radioColors().txContainer to radioColors().onTxContainer else colors().secondaryContainer to colors().onSecondaryContainer
    'n' -> colors().secondaryContainer to colors().onSecondaryContainer
    else -> colors().primaryContainer to colors().onPrimaryContainer
}

@Composable
private fun Avatar(peer: String) {
    val (bg, fg) = avatarColors(peer)
    Box(Modifier.size(40.dp).clip(CircleShape).background(bg), contentAlignment = Alignment.Center) {
        Text(peer.take(1).uppercase(), color = fg, fontSize = 16.sp, fontWeight = FontWeight.Medium)
    }
}

@Composable
fun MailListScreen(radio: Radio, folder: Folder, onFolder: (Folder) -> Unit, onOpen: (Mail) -> Unit, onCompose: () -> Unit, onSettings: () -> Unit) {
    val s = radio.status
    if (s.real) return NotOnRealRadio("Email", "Email")
    val inboxUnread = radio.mails.count { it.folder == Folder.Inbox && it.unread }
    val shown = radio.mails.filter { it.folder == folder }
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            TopBar("Email", s.mailAddress, subMono = true) {
                IconBtn(Icons.Filled.Sync, "Check mail") {}
                IconBtn(Icons.Filled.Settings, "Email settings", tonal = true, onClick = onSettings)
            }
            Row(Modifier.fillMaxWidth()) {
                Folder.entries.forEach { f ->
                    val on = f == folder
                    Box(Modifier.weight(1f).height(48.dp).clickable { onFolder(f) }, contentAlignment = Alignment.Center) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(f.name, fontSize = 14.sp, fontWeight = FontWeight.Medium, color = if (on) colors().primary else colors().onSurfaceVariant)
                            if (f == Folder.Inbox && inboxUnread > 0) Badge(Modifier.padding(start = 6.dp)) { Text("$inboxUnread") }
                        }
                        if (on) Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth(0.6f).height(3.dp).clip(RoundedCornerShape(topStart = 3.dp, topEnd = 3.dp)).background(colors().primary))
                    }
                }
            }
            HorizontalDivider(color = colors().outlineVariant)
            val waiting = shown.filter { it.waiting }
            LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(bottom = 88.dp)) {
                if (waiting.isNotEmpty()) item { WaitingCard(waiting, s.gateway) { radio.getMail(waiting.first().id) } }
                items(shown, key = { it.id }) { m -> MailRow(m) { onOpen(m) } }
                if (shown.isEmpty()) item { Small("Nothing here yet.", Modifier.padding(24.dp), size = 14.sp) }
            }
        }
        ExtendedFloatingActionButton(
            onClick = onCompose, modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp),
            icon = { Icon(Icons.Filled.Edit, null) }, text = { Text("Compose") },
        )
        radio.note?.let { Note(it, Modifier.align(Alignment.BottomCenter).padding(bottom = 88.dp)) }
    }
}

@Composable
private fun WaitingCard(waiting: List<Mail>, gateway: String, onGet: () -> Unit) {
    val first = waiting.first()
    Row(
        Modifier.padding(start = 12.dp, end = 12.dp, top = 12.dp, bottom = 4.dp).fillMaxWidth().clip(RoundedCornerShape(16.dp))
            .background(colors().primaryContainer).padding(start = 16.dp, end = 8.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Icon(Icons.Filled.CloudDownload, null, tint = colors().onPrimaryContainer)
        Column(Modifier.weight(1f)) {
            Text("${waiting.size} mail waiting at the gateway", fontSize = 14.sp, fontWeight = FontWeight.Medium, color = colors().onPrimaryContainer)
            Text("${first.peer} · ${first.waitingSize} · via $gateway", fontSize = 14.sp, lineHeight = 19.sp, color = colors().onPrimaryContainer.copy(alpha = 0.85f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Button(onGet, Modifier.height(36.dp), contentPadding = PaddingValues(horizontal = 18.dp)) { Text("Get") }
    }
}

@Composable
private fun MailRow(m: Mail, onClick: () -> Unit) {
    val w = if (m.unread) FontWeight.Medium else FontWeight.Normal
    Row(Modifier.fillMaxWidth().clickable(onClick = onClick).heightIn(min = 72.dp).padding(horizontal = 16.dp, vertical = 10.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        Avatar(m.peer)
        Column(Modifier.weight(1f)) {
            Text(m.peer, fontSize = 15.sp, lineHeight = 24.sp, fontWeight = w, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(m.subject, fontSize = 14.sp, lineHeight = 20.sp, fontWeight = w, color = if (m.unread) colors().onSurface else colors().onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (m.waiting) Chip("Waiting · Get", Modifier.padding(top = 4.dp), small = true, icon = Icons.Filled.CloudDownload, container = colors().primaryContainer, content = colors().onPrimaryContainer)
            if (m.folder == Folder.Outbox) Small("waiting for a clear channel", Modifier.padding(top = 2.dp))
        }
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(m.time, fontSize = 12.sp, color = if (m.unread) colors().onSurface else colors().onSurfaceVariant, fontWeight = w)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (m.attachment != null) Icon(Icons.Filled.AttachFile, "attachment", Modifier.size(16.dp), tint = colors().onSurfaceVariant)
                if (m.unread) Dot(radioColors().tx)
            }
        }
    }
}

/** Design 06 (inbox) and 07 (sent, with its journey). */
@Composable
fun MailOpenScreen(radio: Radio, id: Long, onBack: () -> Unit, onReply: (Mail) -> Unit) {
    val m = radio.mails.firstOrNull { it.id == id }
    if (m == null) {
        LaunchedEffect(id) { onBack() }
        return
    }
    LaunchedEffect(id) { radio.readMail(id) }
    val inFolder = radio.mails.filter { it.folder == m.folder }
    val pos = inFolder.indexOfFirst { it.id == id } + 1
    Column(Modifier.fillMaxSize()) {
        TopBar(m.folder.name, "$pos of ${inFolder.size}", titleSize = 18.sp, lead = { BackLead(onBack) }) {
            if (m.folder == Folder.Inbox) IconBtn(Icons.AutoMirrored.Filled.Reply, "Reply") { onReply(m) }
            IconBtn(Icons.Filled.Delete, "Delete") { radio.deleteMail(id); onBack() }
        }
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(bottom = 16.dp)) {
            Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp)) {
                Text(m.subject, fontSize = 24.sp, lineHeight = 32.sp)
                if (m.folder == Folder.Inbox) {
                    Row(Modifier.padding(top = 14.dp, bottom = 10.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Avatar(m.peer)
                        Column(Modifier.weight(1f)) {
                            MonoText(m.peer, size = 13.sp)
                            Row { Text("to ", fontSize = 13.sp, color = colors().onSurfaceVariant); MonoText(radio.status.mailAddress, color = colors().onSurfaceVariant, size = 12.5.sp) }
                        }
                        Text(m.day, fontSize = 12.sp, color = colors().onSurfaceVariant, textAlign = androidx.compose.ui.text.style.TextAlign.End)
                    }
                } else {
                    Row(Modifier.padding(top = 12.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (m.delivered) Chip("Delivered", icon = Icons.Filled.DoneAll, container = radioColors().okContainer, content = colors().onSurface)
                        else Chip("Waiting", icon = Icons.Filled.Schedule, container = colors().surfaceContainerHigh)
                        Text("via gateway ${radio.status.gateway}", fontSize = 13.sp, color = colors().onSurfaceVariant)
                    }
                    Column(Modifier.padding(top = 8.dp)) {
                        HeaderRow("From") { MonoText(radio.status.mailAddress, size = 12.5.sp) }
                        HeaderRow("To") { MonoText(m.peer, size = 12.5.sp) }
                        HeaderRow("Written") { Text(m.day, fontSize = 14.sp) }
                    }
                }
            }
            if (m.folder == Folder.Inbox) HorizontalDivider(Modifier.padding(horizontal = 16.dp), color = colors().outlineVariant)
            if (m.journey.isNotEmpty()) {
                Card(Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp), color = colors().surfaceContainer) {
                    Label("JOURNEY · LOCAL TIME", Modifier.padding(bottom = 10.dp))
                    m.journey.forEachIndexed { i, (t, what) -> JourneyStep(t, what, last = i == m.journey.lastIndex) }
                }
            }
            if (m.waiting) {
                Card(Modifier.padding(16.dp), color = colors().primaryContainer) {
                    Text("Only the header has come over the air. Get pulls the body through the gateway.", color = colors().onPrimaryContainer, fontSize = 14.sp)
                    Button({ radio.getMail(id) }, Modifier.padding(top = 10.dp)) { Text("Get") }
                }
            } else {
                Text(m.body, Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp), fontSize = 15.sp, lineHeight = 23.sp)
            }
            m.attachment?.let { AttachmentCard(it, open = m.folder == Folder.Inbox) }
        }
    }
}

@Composable
private fun HeaderRow(key: String, value: @Composable () -> Unit) {
    Row(Modifier.padding(vertical = 3.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(key, Modifier.width(64.dp), fontSize = 14.sp, color = colors().onSurfaceVariant)
        value()
    }
}

@Composable
private fun JourneyStep(time: String, what: String, last: Boolean) {
    Row(Modifier.height(IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        Column(Modifier.width(16.dp).fillMaxHeight(), horizontalAlignment = Alignment.CenterHorizontally) {
            Box(Modifier.padding(top = 5.dp).size(12.dp).clip(CircleShape).background(if (last) radioColors().ok else colors().primary))
            if (!last) Box(Modifier.padding(vertical = 3.dp).width(2.dp).weight(1f).background(colors().outlineVariant))
        }
        Row(Modifier.padding(bottom = if (last) 0.dp else 16.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            MonoText(time, Modifier.width(48.dp), color = colors().onSurfaceVariant, size = 12.5.sp)
            Text(what, fontSize = 14.sp, fontWeight = if (last) FontWeight.Medium else FontWeight.Normal)
        }
    }
}

@Composable
private fun AttachmentCard(a: Attachment, open: Boolean) {
    Row(
        Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp))
            .background(colors().surfaceContainerHigh).padding(start = 16.dp, end = 12.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.size(40.dp).clip(RoundedCornerShape(10.dp)).background(radioColors().okContainer), contentAlignment = Alignment.Center) {
            Icon(Icons.Filled.Description, null, tint = colors().onSurface)
        }
        Column(Modifier.weight(1f)) {
            Text(a.name, fontSize = 15.sp, lineHeight = 20.sp)
            Text(a.size, fontSize = 13.sp, color = colors().onSurfaceVariant)
        }
        if (open) FilledTonalButton({}, Modifier.height(36.dp), contentPadding = PaddingValues(horizontal = 18.dp)) { Text("Open") }
    }
}

/** Design 08 (USB) and 08b (Bluetooth: files stay, with a note that USB is the reliable way). */
@Composable
fun ComposeScreen(radio: Radio, to: String, subject: String, startBody: String, startFile: Attachment?, onClose: () -> Unit, onSetUpUsb: () -> Unit) {
    val s = radio.status
    val bt = s.link == Link.Bluetooth
    var toText by rememberSaveable { mutableStateOf(to) }
    var subj by rememberSaveable { mutableStateOf(subject) }
    var body by rememberSaveable { mutableStateOf(startBody) }
    var file by remember { mutableStateOf(startFile) }
    var triedBt by rememberSaveable { mutableStateOf(false) }
    Column(Modifier.fillMaxSize()) {
        TopBar("New mail", titleSize = 20.sp, lead = { IconBtn(Icons.Filled.Close, "Close", onClick = onClose) })
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 16.dp)) {
            Column(Modifier.padding(top = 12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(toText, { toText = it }, Modifier.fillMaxWidth(), label = { Text("To") }, singleLine = true, textStyle = TextStyle(fontFamily = Mono, fontSize = 15.sp))
                OutlinedTextField(subj, { subj = it }, Modifier.fillMaxWidth(), label = { Text("Subject") }, singleLine = true)
                OutlinedTextField(body, { body = it }, Modifier.fillMaxWidth().heightIn(min = 104.dp), label = { Text("Message") })
            }
            file?.let { f ->
                Chip(
                    "${f.name} · ${f.size}", Modifier.padding(top = 14.dp).height(36.dp), container = Color.Transparent, content = colors().onSurface,
                    border = colors().outlineVariant, lead = { Icon(Icons.Filled.Description, null, Modifier.size(18.dp), tint = colors().primary) },
                    trail = { Icon(Icons.Filled.Close, "Remove", Modifier.size(18.dp).clickable { file = null }) },
                )
            }
            if (!bt || triedBt) {
                Row(Modifier.padding(top = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Outlined("Attach file", icon = Icons.Filled.AttachFile) { file = Attachment("qa-32k.bin", "32.0 KB") }
                    Small("Files up to 512 KB in all on ${s.preset}; pictures are shrunk to fit.")
                }
            }
            if (bt) {
                Column(Modifier.padding(top = 14.dp).fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(radioColors().warnContainer).padding(16.dp, 14.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Icon(Icons.Filled.Usb, null, tint = radioColors().onWarnContainer)
                        Text("Files travel best over USB", fontSize = 15.sp, fontWeight = FontWeight.Medium, color = radioColors().onWarnContainer)
                    }
                    Text(
                        "Mail files go on the radio at vhf-fast. That preset is untested on Bluetooth: you can try it, but a Digirig over USB is the reliable way to send files.",
                        Modifier.padding(top = 6.dp), fontSize = 13.5.sp, lineHeight = 19.sp, color = radioColors().onWarnContainer.copy(alpha = 0.92f),
                    )
                    Row(Modifier.padding(top = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Outlined("Try on Bluetooth", Modifier.weight(1f), color = radioColors().onWarnContainer, borderColor = radioColors().onWarnContainer) {
                            triedBt = true; file = Attachment("qa-32k.bin", "32.0 KB")
                        }
                        Button(
                            onSetUpUsb, Modifier.weight(1f),
                            colors = ButtonDefaults.buttonColors(containerColor = radioColors().onWarnContainer, contentColor = radioColors().warnContainer),
                            contentPadding = PaddingValues(horizontal = 8.dp),
                        ) { Icon(Icons.Filled.Usb, null, Modifier.size(16.dp)); Spacer(Modifier.width(6.dp)); Text("Set up USB") }
                    }
                }
            }
        }
        val bytes = body.length + (if (file != null) 32_768 else 0) + 433
        val secs = Core.airtimeSeconds("vhf-fast", bytes)
        val air = if (secs != null) "~${secs.toInt() / 60} min ${secs.toInt() % 60} s" else "~1 min 45 s"
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(topStart = 16.dp, topEnd = 16.dp)).background(colors().surfaceContainer).padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 14.dp)) {
            Small("$bytes B · $air on vhf-fast · via gateway ${s.gateway}", Modifier.padding(bottom = 10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedButton(onClose, Modifier.weight(1f)) { Text("Cancel") }
                Button({ radio.sendMail(toText, subj, body, file); onClose() }, Modifier.weight(1.4f), enabled = toText.contains('@')) {
                    Icon(Icons.AutoMirrored.Filled.Send, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Send")
                }
            }
        }
    }
}

/** Design 09: Email settings sheet (copy-to, gateway). */
@Composable
fun MailSettingsSheet(radio: Radio, onClose: () -> Unit, onAllSettings: () -> Unit) {
    val s = radio.status
    var copy by rememberSaveable { mutableStateOf(s.copyTo) }
    var code by rememberSaveable { mutableStateOf("") }
    Sheet(onClose) {
        Row(Modifier.padding(start = 24.dp, end = 24.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Icon(Icons.Filled.Settings, null, tint = colors().primary)
            Text("Email settings", fontSize = 22.sp, lineHeight = 28.sp)
        }
        Column(Modifier.padding(start = 24.dp, end = 24.dp, top = 6.dp)) {
            Label("COPY-TO · a copy of every mail you send and get", Modifier.padding(bottom = 8.dp))
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(copy, { copy = it }, Modifier.weight(1f), label = { Text("Copy-to address") }, singleLine = true)
                FilledTonalButton({ radio.update { it.copy(copyTo = copy, copyToConfirmed = false) } }) { Text("Save") }
            }
            if (!s.copyToConfirmed) {
                Row(Modifier.padding(top = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(code, { code = it }, Modifier.weight(1f), label = { Text("Confirm code") }, singleLine = true, textStyle = TextStyle(fontFamily = Mono, letterSpacing = 4.sp))
                    Button({ radio.update { it.copy(copyToConfirmed = true) } }) { Text("Confirm") }
                }
                Small("Waiting for the code sent to ${s.copyTo}", Modifier.padding(start = 16.dp, top = 6.dp))
            } else {
                Small("Confirmed: ${s.copyTo}", Modifier.padding(start = 16.dp, top = 6.dp), color = radioColors().ok)
            }
        }
        Section("Mail gateway", top = 18.dp)
        Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            Box(Modifier.size(40.dp).clip(CircleShape).background(colors().primaryContainer), contentAlignment = Alignment.Center) {
                Icon(Icons.Filled.DeviceHub, null, Modifier.size(22.dp), tint = colors().onPrimaryContainer)
            }
            Column {
                Row { MonoText(s.gateway, size = 15.sp); Text(" · ${s.gatewayPlace}", fontSize = 16.sp) }
                Text("heard 10 dB", fontSize = 14.sp, color = colors().onSurfaceVariant)
            }
        }
        Row(Modifier.padding(start = 24.dp, end = 24.dp, top = 2.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Label("MAIL GW", Modifier.width(68.dp))
            GatewayBars(s.mailGwBars, Modifier.weight(1f))
        }
        HorizontalDivider(Modifier.padding(top = 12.dp), color = colors().outlineVariant)
        Row(Modifier.fillMaxWidth().clickable(onClick = onAllSettings).padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text("All app settings", fontSize = 15.sp, color = colors().primary)
                Text("Change the gateway in Settings → Email", fontSize = 12.sp, color = colors().onSurfaceVariant)
            }
            Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, tint = colors().onSurfaceVariant)
        }
    }
}

@Composable
fun GatewayBars(bars: Int, modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        repeat(3) { i ->
            Box(Modifier.weight(1f).height(6.dp).clip(RoundedCornerShape(3.dp)).background(if (i < bars) radioColors().ok else colors().surfaceContainerHighest))
        }
    }
}
