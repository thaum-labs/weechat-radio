@file:OptIn(ExperimentalMaterial3Api::class)

package com.weechatradio.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Badge
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.weechatradio.app.core.Core
import com.weechatradio.app.radio.Chat
import com.weechatradio.app.radio.Radio

private enum class ChatFilter(val label: String) { All("All"), Rooms("Rooms"), People("People") }

@Composable
fun ChatsScreen(radio: Radio, open: (Page) -> Unit) {
    var filter by rememberSaveable { mutableStateOf(ChatFilter.All) }
    var asking by rememberSaveable { mutableStateOf(false) }
    val band = Core.describeFrequency(radio.status.frequencyMhz)
        ?.substringAfter('(', "")?.substringBefore(')').orEmpty()

    Scaffold(
        contentWindowInsets = WindowInsets(0, 0, 0, 0),
        topBar = {
            TopAppBar(
                title = { Text("Chats") },
                actions = {
                    AssistChip(
                        onClick = {},
                        label = { Text(listOf(radio.status.radioName, band).filter { it.isNotEmpty() }.joinToString(" · ")) },
                    )
                    IconButton(onClick = {}) { Icon(Icons.Filled.Search, contentDescription = "Search") }
                },
            )
        },
        floatingActionButton = {
            ExtendedFloatingActionButton(
                onClick = { asking = true },
                icon = { Icon(Icons.Filled.Edit, contentDescription = null) },
                text = { Text("New chat") },
            )
        },
    ) { padding ->
        val shown = radio.chats.filter {
            when (filter) {
                ChatFilter.All -> true
                ChatFilter.Rooms -> it.isRoom
                ChatFilter.People -> !it.isRoom
            }
        }
        LazyColumn(Modifier.fillMaxSize().padding(padding)) {
            item {
                Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    ChatFilter.entries.forEach { f ->
                        FilterChip(selected = filter == f, onClick = { filter = f }, label = { Text(f.label) })
                    }
                }
            }
            items(shown, key = { it.name }) { chat ->
                ChatRow(chat) {
                    radio.openChat(chat.name)
                    open(Page.Conversation(chat.name))
                }
            }
        }
    }

    if (asking) NewChatDialog(onDismiss = { asking = false }) { name ->
        asking = false
        open(Page.Conversation(name))
    }
}

@Composable
private fun ChatRow(chat: Chat, onClick: () -> Unit) {
    val last = chat.messages.lastOrNull()
    ListItem(
        modifier = Modifier.clickable(onClick = onClick),
        leadingContent = { Avatar(chat.name) },
        headlineContent = {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(chat.name, fontWeight = FontWeight.SemiBold)
                if (chat.viaInternet) {
                    Text("internet", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.tertiary)
                }
            }
        },
        supportingContent = {
            val text = when {
                last == null -> ""
                last.mine -> "You: ${last.text}"
                chat.isRoom -> "${last.from}: ${last.text}"
                else -> last.text
            }
            Text(text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        },
        trailingContent = {
            Column(horizontalAlignment = Alignment.End) {
                Text(last?.time.orEmpty(), style = MaterialTheme.typography.labelSmall)
                if (chat.unread > 0) Badge(containerColor = MaterialTheme.colorScheme.primary) { Text("${chat.unread}") }
            }
        },
    )
}

/** A room starts with "#"; anything else is a callsign. */
@Composable
private fun NewChatDialog(onDismiss: () -> Unit, onOpen: (String) -> Unit) {
    var name by rememberSaveable { mutableStateOf("") }
    val clean = name.trim().let { if (it.startsWith("#")) it.lowercase() else it.uppercase() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("New chat") },
        text = {
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                label = { Text("Callsign, or #room") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
        },
        confirmButton = { TextButton(onClick = { onOpen(clean) }, enabled = clean.length >= 2) { Text("Open") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
