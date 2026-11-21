@file:OptIn(ExperimentalMaterial3Api::class)

package com.weechatradio.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.weechatradio.app.radio.RadioLink

/**
 * What to set on a VOX radio before going on air. Same radio settings as the
 * desktop guide (wcr setup::vox_radio_note, QA #365: squelch 1 and VOX 4
 * keyed where squelch 0 and VOX 1 did not), in phone words.
 */
@Composable
fun VoxSetupScreen(link: RadioLink, back: () -> Unit, onConnect: (name: String) -> Unit) {
    var name by rememberSaveable { mutableStateOf("") }
    val headset = link == RadioLink.VoxHeadset

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (headset) "VOX over Bluetooth" else "VOX over a cable") },
                navigationIcon = {
                    IconButton(onClick = back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back") }
                },
            )
        },
        bottomBar = {
            Button(
                onClick = { onConnect(name.trim().ifEmpty { "VOX radio" }) },
                modifier = Modifier.fillMaxWidth().navigationBarsPadding().padding(16.dp),
            ) { Text("Done, go on air") }
        },
    ) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).padding(horizontal = 20.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                label = { Text("Radio name (optional)") },
                placeholder = { Text("UV-36 Pro") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Step(
                "1. Connect",
                if (headset) {
                    "Pair the radio with the phone in Android's Bluetooth settings, as you would a headset. " +
                        "The app sends and hears the radio's audio through that link."
                } else {
                    "Plug the USB-C to K1 lead into the phone and the radio's speaker/mic socket. " +
                        "The app uses it like a wired headset."
                },
            )
            Step(
                "2. On the radio",
                "VOX on, squelch 1 (not 0), VOX level 4. At squelch 0 the radio hears noise all the time " +
                    "and its VOX may not key.\n\nThe cost of squelch 1: the app only hears signals strong enough " +
                    "to open the squelch, so the weakest lines are lost.",
            )
            Step(
                "3. Level",
                "Start with the transmit level low (Settings > Transmit level) and raise it until the radio keys. " +
                    "Too much drive overloads the radio's mic input and makes the signal worse.",
            )
            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh)) {
                Column(Modifier.padding(14.dp)) {
                    Text("Mode", fontWeight = FontWeight.SemiBold)
                    Text(
                        "The app starts on hf-poor, the mode that has worked best over VOX so far. " +
                            (if (headset) "A Bluetooth headset link carries phone-call quality sound, so faster modes may not get through. " else "") +
                            "Try a faster mode in Settings once lines go both ways.",
                        style = MaterialTheme.typography.bodySmall,
                    )
                    Text(
                        "On VOX the app sends no beacons, so it never keys over someone by itself.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(top = 6.dp),
                    )
                }
            }
        }
    }
}

@Composable
private fun Step(title: String, text: String) {
    Column {
        Text(title, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
        Text(text)
    }
}
