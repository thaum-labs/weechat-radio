@file:OptIn(ExperimentalMaterial3Api::class)

package com.weechatradio.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.weechatradio.app.core.Core
import com.weechatradio.app.radio.ChatMessage
import com.weechatradio.app.radio.Delivery
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.ui.theme.LocalStatusColors

@Composable
fun ConversationScreen(radio: Radio, name: String, back: () -> Unit) {
    val chat = radio.chats.find { it.name == name }
    val messages = chat?.messages.orEmpty()
    var draft by rememberSaveable { mutableStateOf("") }
    val list = rememberLazyListState()
    LaunchedEffect(messages.size) { if (messages.isNotEmpty()) list.animateScrollToItem(messages.lastIndex) }

    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = {
                    IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back") }
                },
                title = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Avatar(name, size = 36.dp)
                        Spacer(Modifier.width(12.dp))
                        Column {
                            Text(name, style = MaterialTheme.typography.titleMedium)
                            val heard = chat?.lastHeard.orEmpty()
                            if (heard.isNotEmpty()) {
                                Text(heard, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                    }
                },
                actions = { IconButton(onClick = {}) { Icon(Icons.Filled.MoreVert, contentDescription = "More") } },
            )
        },
        bottomBar = {
            Column(Modifier.navigationBarsPadding().imePadding().padding(horizontal = 12.dp, vertical = 8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(
                        value = draft,
                        onValueChange = { draft = it },
                        placeholder = { Text("Message") },
                        modifier = Modifier.weight(1f),
                        shape = RoundedCornerShape(28.dp),
                    )
                    Spacer(Modifier.width(8.dp))
                    FilledIconButton(
                        onClick = {
                            radio.sendChat(name, draft.trim())
                            draft = ""
                        },
                        enabled = draft.isNotBlank(),
                        modifier = Modifier.size(52.dp),
                    ) { Icon(Icons.AutoMirrored.Filled.Send, contentDescription = "Send") }
                }
                // The core's own airtime sum for this preset and line length.
                val secs = Core.airtimeSeconds(radio.status.preset, draft.toByteArray().size + 24)
                Text(
                    "Short lines send fastest" + (secs?.let { " · about %.1f s on air".format(it) } ?: ""),
                    modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
                    textAlign = TextAlign.Center,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        },
    ) { padding ->
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize().padding(padding).padding(horizontal = 12.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            items(messages, key = { it.id }) { Bubble(it, showSender = name.startsWith("#")) }
        }
    }
}

@Composable
private fun Bubble(m: ChatMessage, showSender: Boolean) {
    val colors = MaterialTheme.colorScheme
    Column(
        Modifier.fillMaxWidth(),
        horizontalAlignment = if (m.mine) Alignment.End else Alignment.Start,
    ) {
        Column(
            Modifier
                .widthIn(max = 300.dp)
                .clip(RoundedCornerShape(18.dp))
                .background(if (m.mine) colors.primary else colors.surfaceContainerHigh)
                .padding(horizontal = 14.dp, vertical = 9.dp),
        ) {
            if (showSender && !m.mine) {
                Text(m.from, style = MaterialTheme.typography.labelMedium, color = LocalStatusColors.current.accent)
            }
            Text(m.text, color = if (m.mine) colors.onPrimary else colors.onSurface)
        }
        val status = m.delivery
        val statusColor = when (status) {
            Delivery.Received, Delivery.Sent -> LocalStatusColors.current.good
            Delivery.NoReceipt -> colors.error
            Delivery.Waiting -> LocalStatusColors.current.warn
            null -> colors.onSurfaceVariant
        }
        Row(Modifier.padding(top = 2.dp, start = 4.dp, end = 4.dp)) {
            Text(m.time, style = MaterialTheme.typography.labelSmall, color = colors.onSurfaceVariant)
            if (status != null) {
                Text(" · ${status.words}", style = MaterialTheme.typography.labelSmall, color = statusColor)
            }
        }
    }
}
