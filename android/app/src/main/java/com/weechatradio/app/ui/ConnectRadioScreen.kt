package com.weechatradio.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Bluetooth
import androidx.compose.material.icons.filled.Cable
import androidx.compose.material.icons.filled.Headset
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.radio.RadioLink

/**
 * First run: pick the radio. Bluetooth radios (VR-N76, UV-PRO) carry audio and
 * PTT themselves; any other radio works on VOX, over a Bluetooth headset link
 * or a USB-C audio lead. Simulated for now; Phase 0 makes it real.
 */
@Composable
fun ConnectRadioScreen(radio: Radio) {
    var vox by rememberSaveable { mutableStateOf<RadioLink?>(null) }
    vox?.let { link ->
        VoxSetupScreen(link, back = { vox = null }) { radio.connect(it, link) }
        return
    }
    Surface(Modifier.fillMaxSize()) {
        Column(
            Modifier.safeDrawingPadding().verticalScroll(rememberScrollState()).padding(20.dp),
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
                    Modifier.fillMaxWidth().padding(20.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Icon(
                        Icons.Filled.SettingsRemote, contentDescription = null,
                        modifier = Modifier.size(72.dp), tint = MaterialTheme.colorScheme.primary,
                    )
                    Spacer(Modifier.height(8.dp))
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
                Button(onClick = { radio.connect("VR-N76", RadioLink.BluetoothRadio) }) { Text("Connect") }
            }
            RadioRow("UV-PRO", "Not paired · signal weak") {
                OutlinedButton(onClick = {}) { Text("Pair") }
            }

            SectionTitle("Any other radio, on VOX", Modifier.padding(top = 8.dp))
            Text(
                "For radios like the Baofeng UV-36 Pro. The radio's VOX keys it when the phone sends sound.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            VoxRow(RadioLink.VoxHeadset, "Paired to the phone as a headset") { vox = it }
            VoxRow(RadioLink.VoxCable, "USB-C to K1 audio lead") { vox = it }

            Text(
                "Simulated radio: real Bluetooth and audio come when the phone arrives.",
                modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
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

@Composable
private fun VoxRow(link: RadioLink, detail: String, onPick: (RadioLink) -> Unit) {
    OutlinedCard(Modifier.fillMaxWidth().clickable { onPick(link) }) {
        ListItem(
            headlineContent = { Text(if (link == RadioLink.VoxHeadset) "Bluetooth headset" else "USB-C cable") },
            supportingContent = { Text(detail) },
            leadingContent = {
                Icon(if (link == RadioLink.VoxHeadset) Icons.Filled.Headset else Icons.Filled.Cable, contentDescription = null)
            },
            trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null) },
        )
    }
}
