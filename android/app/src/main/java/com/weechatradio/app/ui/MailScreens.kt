@file:OptIn(ExperimentalMaterial3Api::class)

package com.weechatradio.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Reply
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.AttachFile
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.weechatradio.app.core.Core
import com.weechatradio.app.radio.Mail
import com.weechatradio.app.radio.MailFolder
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.ui.theme.LocalStatusColors

@Composable
fun MailScreen(radio: Radio, open: (Page) -> Unit) {
    var folder by rememberSaveable { mutableStateOf(MailFolder.Inbox) }
    Scaffold(
        contentWindowInsets = WindowInsets(0, 0, 0, 0),
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text("Mail")
                        Text(radio.mailAddress, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                },
                actions = {
                    IconButton(onClick = {}) { Icon(Icons.Filled.Refresh, contentDescription = "Check for mail") }
                    IconButton(onClick = {}) { Icon(Icons.Filled.Search, contentDescription = "Search") }
                },
            )
        },
        floatingActionButton = {
            ExtendedFloatingActionButton(
                onClick = { open(Page.NewMail()) },
                icon = { Icon(Icons.Filled.Edit, contentDescription = null) },
                text = { Text("New mail") },
            )
        },
    ) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding)) {
            item {
                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
                    MailFolder.entries.forEachIndexed { i, f ->
                        SegmentedButton(
                            selected = folder == f,
                            onClick = { folder = f },
                            shape = SegmentedButtonDefaults.itemShape(i, MailFolder.entries.size),
                        ) { Text(f.name) }
                    }
                }
            }
            items(radio.mail.filter { it.folder == folder }, key = { it.id }) { m ->
                MailRow(m) { open(Page.ReadMail(m.id)) }
            }
            item { GatewayCard() }
        }
    }
}

@Composable
private fun MailRow(m: Mail, onClick: () -> Unit) {
    ListItem(
        modifier = Modifier.clickable(onClick = onClick),
        leadingContent = { Avatar(m.peer) },
        headlineContent = { Text(m.peer, fontWeight = FontWeight.SemiBold) },
        supportingContent = {
            Column {
                Text(m.subject, color = MaterialTheme.colorScheme.onSurface)
                Text(
                    if (m.downloaded) m.body.replace('\n', ' ') else "Not downloaded yet · tap to get it",
                    maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
            }
        },
        trailingContent = {
            Column(horizontalAlignment = Alignment.End) {
                Text(m.time, style = MaterialTheme.typography.labelSmall)
                if (!m.downloaded) Tag("Waiting")
                if (m.folder == MailFolder.Outbox) Tag("Sending")
                if (m.attachment != null) Icon(Icons.Filled.AttachFile, contentDescription = "Attachment", modifier = Modifier.size(16.dp))
            }
        },
    )
}

@Composable
private fun Tag(text: String) {
    val c = LocalStatusColors.current
    Card(colors = CardDefaults.cardColors(containerColor = c.warnContainer, contentColor = c.warn)) {
        Text(text, Modifier.padding(horizontal = 6.dp, vertical = 2.dp), style = MaterialTheme.typography.labelSmall)
    }
}

@Composable
private fun GatewayCard() {
    Card(
        Modifier.fillMaxWidth().padding(16.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
    ) {
        Column(Modifier.padding(14.dp)) {
            Row {
                Text("● ", color = LocalStatusColors.current.good)
                Text("Mail gateway: Auto", fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                Text("G4XYZ-1", style = MaterialTheme.typography.labelSmall)
            }
            Text(
                "Mail goes out through the nearest gateway on air, or the internet when you have signal.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
fun ReadMailScreen(radio: Radio, id: Long, back: () -> Unit, open: (Page) -> Unit) {
    val m = radio.mail.find { it.id == id }
    if (m == null) {
        // Deleted: go back to the list.
        LaunchedEffect(id) { back() }
        return
    }
    var getting by rememberSaveable { mutableStateOf(false) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {},
                navigationIcon = {
                    IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back") }
                },
                actions = {
                    if (m.downloaded) {
                        IconButton(onClick = { radio.deleteMail(m.id) }) { Icon(Icons.Filled.Delete, contentDescription = "Delete") }
                    }
                },
            )
        },
        bottomBar = {
            if (m.downloaded && m.folder == MailFolder.Inbox) {
                FilledTonalButton(
                    onClick = { open(Page.NewMail(to = m.peer, subject = "Re: ${m.subject}")) },
                    modifier = Modifier.fillMaxWidth().navigationBarsPadding().padding(16.dp),
                ) {
                    Icon(Icons.AutoMirrored.Filled.Reply, contentDescription = null)
                    Spacer(Modifier.width(8.dp))
                    Text("Reply")
                }
            }
        },
    ) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).padding(horizontal = 20.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Text(m.subject, style = MaterialTheme.typography.headlineSmall)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Avatar(m.peer)
                Spacer(Modifier.width(12.dp))
                Column {
                    Text(m.peer, fontWeight = FontWeight.SemiBold)
                    Text(
                        (if (m.folder == MailFolder.Inbox) "to me" else "from me") + " · ${m.time}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            if (m.downloaded) {
                // Monospace keeps lists like repeater tables lined up.
                Text(m.body, fontFamily = if (m.body.contains("  ") || m.body.contains(" tone ")) FontFamily.Monospace else null)
                m.attachment?.let { a ->
                    OutlinedCard(Modifier.fillMaxWidth()) {
                        ListItem(
                            leadingContent = { Icon(Icons.Filled.Image, contentDescription = null) },
                            headlineContent = { Text(a) },
                            trailingContent = { TextButton(onClick = {}) { Text("Save") } },
                        )
                    }
                }
            } else {
                Text(
                    "Only the header has come over the air. Get pulls the rest through the gateway.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Button(
                    onClick = {
                        getting = true
                        radio.getMail(m.id)
                    },
                    enabled = !getting,
                ) { Text(if (getting) "Getting…" else "Get") }
            }
        }
    }
}

@Composable
fun NewMailScreen(radio: Radio, to: String, subject: String, back: () -> Unit) {
    var toField by rememberSaveable { mutableStateOf(to) }
    var subjectField by rememberSaveable { mutableStateOf(subject) }
    var body by rememberSaveable { mutableStateOf("") }
    val bytes = (toField + subjectField + body).toByteArray().size
    val secs = Core.airtimeSeconds(radio.status.preset, bytes)

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("New mail") },
                navigationIcon = {
                    IconButton(onClick = back) { Icon(Icons.Filled.Close, contentDescription = "Close") }
                },
                actions = {
                    IconButton(onClick = {}) { Icon(Icons.Filled.AttachFile, contentDescription = "Attach") }
                    Button(
                        onClick = {
                            radio.sendMail(toField.trim(), subjectField.trim(), body)
                            back()
                        },
                        enabled = toField.isNotBlank() && body.isNotBlank(),
                        modifier = Modifier.padding(end = 8.dp),
                    ) {
                        Icon(Icons.AutoMirrored.Filled.Send, contentDescription = null, modifier = Modifier.size(18.dp))
                        Spacer(Modifier.width(6.dp))
                        Text("Send")
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).imePadding().padding(horizontal = 16.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            OutlinedTextField(toField, { toField = it }, label = { Text("To") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(subjectField, { subjectField = it }, label = { Text("Subject") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(body, { body = it }, label = { Text("Message") }, modifier = Modifier.fillMaxWidth().height(240.dp))
            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh)) {
                ListItem(
                    leadingContent = { Icon(Icons.Filled.Schedule, contentDescription = null) },
                    headlineContent = { Text(secs?.let { "About %.1f s on air".format(it) } ?: "Airtime unknown") },
                    supportingContent = { Text("$bytes bytes · through gateway G4XYZ-1") },
                )
            }
            Text(
                "If no gateway hears it, it waits in Outbox and tries again. Pictures shrink to 4 KB.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
