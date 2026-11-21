@file:OptIn(ExperimentalMaterial3Api::class)

package com.weechatradio.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Bluetooth
import androidx.compose.material.icons.filled.Cloud
import androidx.compose.material.icons.filled.Email
import androidx.compose.material.icons.filled.Palette
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.SettingsInputAntenna
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.weechatradio.app.core.Core
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.ui.theme.ThemeChoice

@Composable
fun SettingsScreen(radio: Radio, theme: ThemeChoice, onTheme: (ThemeChoice) -> Unit, back: () -> Unit) {
    val s = radio.status
    var editingCall by rememberSaveable { mutableStateOf(false) }
    var pickingPreset by rememberSaveable { mutableStateOf(false) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = {
                    IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back") }
                },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState())) {
            SectionTitle("Station", Modifier.padding(start = 16.dp, top = 8.dp))
            Setting(Icons.Filled.Person, "Callsign", "${s.callsign} · grid ${s.grid}", onClick = { editingCall = true })
            Setting(Icons.Filled.Bluetooth, "Radio", "${s.radioName} · Bluetooth", arrow = true)
            Setting(
                Icons.Filled.SettingsInputAntenna, "Channel and mode",
                "${s.frequencyMhz} · ${s.mode} · ${s.preset}", arrow = true, onClick = { pickingPreset = true },
            )
            ListItem(
                leadingContent = { Icon(Icons.Filled.Tune, contentDescription = null) },
                headlineContent = { Text("Transmit level") },
                supportingContent = {
                    Column {
                        Text("${s.txLevelDb} dB · set radio squelch to 0")
                        Slider(
                            value = s.txLevelDb.toFloat(),
                            onValueChange = { radio.setTxLevel(it.toInt()) },
                            valueRange = -40f..0f,
                        )
                    }
                },
                trailingContent = { TextButton(onClick = {}) { Text("Test tone") } },
            )
            HorizontalDivider()

            SectionTitle("Mail and internet", Modifier.padding(start = 16.dp, top = 12.dp))
            Setting(Icons.Filled.Email, "Mail gateway", "Auto · nearest heard on air", arrow = true)
            ListItem(
                leadingContent = { Icon(Icons.Filled.Cloud, contentDescription = null) },
                headlineContent = { Text("Use the internet hub") },
                supportingContent = { Text("When the phone has signal") },
                trailingContent = { Switch(checked = s.useInternetHub, onCheckedChange = radio::setUseInternetHub) },
            )
            HorizontalDivider()

            SectionTitle("Look", Modifier.padding(start = 16.dp, top = 12.dp))
            ListItem(
                leadingContent = { Icon(Icons.Filled.Palette, contentDescription = null) },
                headlineContent = { Text("Theme") },
                supportingContent = {
                    Column {
                        Text("Light or dark follows your phone")
                        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(top = 8.dp)) {
                            ThemeChoice.entries.forEachIndexed { i, t ->
                                SegmentedButton(
                                    selected = theme == t,
                                    onClick = { onTheme(t) },
                                    shape = SegmentedButtonDefaults.itemShape(i, ThemeChoice.entries.size),
                                ) { Text(t.label) }
                            }
                        }
                    }
                },
            )
            Text(
                "wcr core " + (Core.version ?: "not loaded"),
                modifier = Modifier.padding(16.dp),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }

    if (editingCall) CallsignDialog(s.callsign, onDismiss = { editingCall = false }) {
        radio.setCallsign(it)
        editingCall = false
    }
    if (pickingPreset) PresetDialog(s.preset, onDismiss = { pickingPreset = false }) {
        radio.setPreset(it)
        pickingPreset = false
    }
}

@Composable
private fun Setting(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    title: String,
    detail: String,
    arrow: Boolean = false,
    onClick: () -> Unit = {},
) {
    ListItem(
        modifier = Modifier.clickable(onClick = onClick),
        leadingContent = { Icon(icon, contentDescription = null) },
        headlineContent = { Text(title) },
        supportingContent = { Text(detail) },
        trailingContent = { if (arrow) Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null) },
    )
}

@Composable
private fun CallsignDialog(current: String, onDismiss: () -> Unit, onSave: (String) -> Unit) {
    var text by rememberSaveable { mutableStateOf(current) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Callsign") },
        text = { OutlinedTextField(text, { text = it }, singleLine = true, label = { Text("Callsign") }) },
        confirmButton = { TextButton(onClick = { onSave(text.trim()) }, enabled = text.isNotBlank()) { Text("Save") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/** The preset list comes straight from the wcr core, so it always matches the desktop app. */
@Composable
private fun PresetDialog(current: String, onDismiss: () -> Unit, onPick: (String) -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Mode") },
        text = {
            val presets = Core.presets()
            if (presets.isEmpty()) {
                Text("The wcr core is not loaded in this build.")
            } else {
                LazyColumn {
                    items(presets, key = { it.name }) { p ->
                        ListItem(
                            modifier = Modifier.clickable { onPick(p.name) },
                            leadingContent = { RadioButton(selected = p.name == current, onClick = { onPick(p.name) }) },
                            headlineContent = { Text(p.name) },
                            supportingContent = { Text(p.description) },
                        )
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Close") } },
    )
}
