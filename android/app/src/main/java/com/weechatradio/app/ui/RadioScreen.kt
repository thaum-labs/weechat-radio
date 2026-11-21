@file:OptIn(ExperimentalMaterial3Api::class)

package com.weechatradio.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Bluetooth
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.weechatradio.app.core.Core
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.ui.theme.LocalStatusColors

@Composable
fun RadioScreen(radio: Radio, open: (Page) -> Unit) {
    val s = radio.status
    var changing by rememberSaveable { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    val status = LocalStatusColors.current

    Scaffold(
        contentWindowInsets = WindowInsets(0, 0, 0, 0),
        topBar = {
            TopAppBar(
                title = { Text("Radio") },
                actions = {
                    IconButton(onClick = { open(Page.Settings) }) { Icon(Icons.Filled.Settings, contentDescription = "Settings") }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).padding(horizontal = 16.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Panel {
                ListItem(
                    colors = ListItemDefaults.colors(containerColor = Color.Transparent),
                    leadingContent = { Icon(Icons.Filled.Bluetooth, contentDescription = null, tint = colors.primary) },
                    headlineContent = { Text("${s.radioName} connected", fontWeight = FontWeight.SemiBold) },
                    supportingContent = { Text("Bluetooth · listening") },
                    trailingContent = { Text("On air", color = status.good, style = MaterialTheme.typography.labelMedium) },
                )
            }

            Panel {
                Column(Modifier.padding(16.dp)) {
                    Text("Channel", style = MaterialTheme.typography.labelMedium, color = colors.onSurfaceVariant)
                    Row(verticalAlignment = Alignment.Bottom) {
                        Text(s.frequencyMhz, style = MaterialTheme.typography.displaySmall)
                        val band = Core.describeFrequency(s.frequencyMhz)?.substringAfter('(', "")?.substringBefore(')')
                        Text(
                            " MHz" + (band?.let { " · $it FM" } ?: ""),
                            modifier = Modifier.weight(1f).padding(bottom = 6.dp),
                            color = colors.onSurfaceVariant,
                        )
                        TextButton(onClick = { changing = true }) { Text("Change") }
                    }
                    Core.frequencyWarning(s.frequencyMhz)?.let { Text(it, color = status.warn, style = MaterialTheme.typography.bodySmall) }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        AssistChip(onClick = {}, label = { Text(s.mode, color = status.plus) })
                        AssistChip(onClick = {}, label = { Text(s.preset) })
                        AssistChip(onClick = {}, label = { Text("${s.callsign} · ${s.grid}") })
                    }
                }
            }

            // Levels in words first, numbers second (as on the desktop meter).
            Panel {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Level("Hearing", if (s.hearing > 0.5f) "Good" else "Low", s.hearing, status.good, highlight = true)
                    Level(
                        "Channel busy",
                        "${(s.channelBusy * 100).toInt()}% · " + if (s.channelBusy < 0.2f) "clear" else "busy",
                        s.channelBusy, status.warn,
                    )
                    Level("Signal quality", "${s.snrDb} dB", (s.snrDb / 15f).coerceIn(0f, 1f), colors.primary)
                }
            }

            SectionTitle("On air just now")
            radio.onAir.take(6).forEach { e ->
                Row {
                    Text("${e.time}  ${e.what}", modifier = Modifier.weight(1f))
                    Text(
                        e.detail,
                        color = if (e.sent) status.accent else colors.onSurfaceVariant,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
            Text(
                "Today: ${s.sentToday} sent · ${s.heardToday} heard",
                color = colors.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
            )
            Text(
                "Simulated radio · wcr core " + (Core.version ?: "not loaded"),
                modifier = Modifier.padding(bottom = 16.dp),
                color = colors.onSurfaceVariant,
                style = MaterialTheme.typography.labelSmall,
            )
        }
    }

    if (changing) FrequencyDialog(s.frequencyMhz, onDismiss = { changing = false }) {
        radio.setFrequency(it)
        changing = false
    }
}

@Composable
private fun Panel(content: @Composable () -> Unit) {
    Card(
        Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
    ) { content() }
}

@Composable
private fun Level(name: String, words: String, fraction: Float, color: Color, highlight: Boolean = false) {
    Column {
        Row {
            Text(name, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            Text(words, color = if (highlight) color else MaterialTheme.colorScheme.onSurfaceVariant)
        }
        LinearProgressIndicator(
            progress = { fraction },
            modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
            color = color,
            trackColor = MaterialTheme.colorScheme.surfaceContainerHighest,
        )
    }
}

/** Type a frequency; the core names the band and warns about poor picks. */
@Composable
private fun FrequencyDialog(current: String, onDismiss: () -> Unit, onSave: (String) -> Unit) {
    var text by rememberSaveable { mutableStateOf(current) }
    val described = Core.describeFrequency(text)
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Channel") },
        text = {
            Column {
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    label = { Text("Frequency in MHz") },
                    singleLine = true,
                    supportingText = { Text(Core.frequencyWarning(text) ?: described ?: "Not a frequency") },
                )
            }
        },
        confirmButton = { TextButton(onClick = { onSave(text.trim()) }, enabled = described != null) { Text("Save") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
