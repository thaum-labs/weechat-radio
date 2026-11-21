package com.weechatradio.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import com.weechatradio.app.radio.SimulatedRadio
import com.weechatradio.app.ui.App
import com.weechatradio.app.ui.theme.ThemeChoice
import com.weechatradio.app.ui.theme.WeeChatRadioTheme

class MainActivity : ComponentActivity() {
    companion object {
        // One station for the app's whole life, so turning the phone keeps it.
        // The real station will run in a foreground service (Phase 0).
        private val radio by lazy { SimulatedRadio() }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val prefs = getSharedPreferences("settings", MODE_PRIVATE)
        setContent {
            var theme by remember {
                mutableStateOf(
                    runCatching { ThemeChoice.valueOf(prefs.getString("theme", "")!!) }
                        .getOrDefault(ThemeChoice.Standard),
                )
            }
            WeeChatRadioTheme(theme) {
                App(
                    radio = radio,
                    theme = theme,
                    onTheme = {
                        theme = it
                        prefs.edit().putString("theme", it.name).apply()
                    },
                )
            }
        }
    }
}
