package com.weechatradio.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Bluetooth
import androidx.compose.material.icons.filled.SettingsRemote
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.weechatradio.app.radio.Radio

/** First run: pick the radio. Simulated for now; Phase 0 makes it real Bluetooth. */
@Composable
fun ConnectRadioScreen(radio: Radio) {
    Surface(Modifier.fillMaxSize()) {
        Column(
            Modifier.safeDrawingPadding().padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("Connect your radio", style = MaterialTheme.typography.headlineMedium)
            Text(
                "One time only. The app remembers it.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Card(
                Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
            ) {
                Column(
                    Modifier.fillMaxWidth().padding(24.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Icon(
                        Icons.Filled.SettingsRemote, contentDescription = null,
                        modifier = Modifier.size(96.dp), tint = MaterialTheme.colorScheme.primary,
                    )
                    Spacer(Modifier.height(12.dp))
                    Text("Looking for radios…", style = MaterialTheme.typography.titleMedium)
                    Text(
                        "Turn the radio on and close the radio maker's app.",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.bodySmall,
                        textAlign = TextAlign.Center,
                    )
                }
            }
            SectionTitle("Found nearby", Modifier.padding(top = 8.dp))
            RadioRow("VR-N76", "Paired · signal strong") {
                Button(onClick = radio::connect) { Text("Connect") }
            }
            RadioRow("UV-PRO", "Not paired · signal weak") {
                OutlinedButton(onClick = {}) { Text("Pair") }
            }
            Spacer(Modifier.weight(1f))
            Text(
                "Simulated radio: real Bluetooth comes when the phone arrives.",
                modifier = Modifier.fillMaxWidth(),
                textAlign = TextAlign.Center,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun RadioRow(name: String, detail: String, action: @Composable () -> Unit) {
    OutlinedCard(Modifier.fillMaxWidth()) {
        ListItem(
            headlineContent = { Text(name) },
            supportingContent = { Text(detail) },
            leadingContent = { Icon(Icons.Filled.Bluetooth, contentDescription = null) },
            trailingContent = action,
        )
    }
}
