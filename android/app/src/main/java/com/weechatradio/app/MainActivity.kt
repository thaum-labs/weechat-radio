package com.weechatradio.app

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.core.content.ContextCompat
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import com.weechatradio.app.radio.ListeningService
import com.weechatradio.app.radio.Station
import com.weechatradio.app.ui.App
import com.weechatradio.app.ui.theme.Look
import com.weechatradio.app.ui.theme.WeechatTheme

/** The settings key for Settings → Radio → "Use the real radio". */
const val REAL_RADIO = "real_radio"

fun hasBluetooth(context: Context) = Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
    ContextCompat.checkSelfPermission(context, Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val prefs = getSharedPreferences("settings", MODE_PRIVATE)
        // Back on the real radio if it was on last time and Bluetooth is still allowed.
        if (prefs.getBoolean(REAL_RADIO, false) && hasBluetooth(this)) Station.useReal(this, true)
        setContent {
            val radio = Station.radio
            var look by remember { mutableStateOf(runCatching { Look.valueOf(prefs.getString("look", "")!!) }.getOrDefault(Look.Material)) }
            // Background listening (on by default) runs as a foreground service once setup is done.
            val s = radio.status
            val listen = s.setUp && s.listenInBackground && s.listening
            LaunchedEffect(listen) {
                if (listen) ListeningService.start(this@MainActivity) else ListeningService.stop(this@MainActivity)
            }
            // Material: dark first, light when the phone is in light mode. modem73 is always dark.
            WeechatTheme(look = look) {
                App(radio, look = look, onLook = { look = it; prefs.edit().putString("look", it.name).apply() })
            }
        }
    }
}
