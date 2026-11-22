package com.weechatradio.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.LaunchedEffect
import com.weechatradio.app.radio.ListeningService
import com.weechatradio.app.radio.Station
import com.weechatradio.app.ui.App
import com.weechatradio.app.ui.theme.WeechatTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val radio = Station.radio
        setContent {
            // Background listening (on by default) runs as a foreground service once setup is done.
            val s = radio.status
            val listen = s.setUp && s.listenInBackground && s.listening
            LaunchedEffect(listen) {
                if (listen) ListeningService.start(this@MainActivity) else ListeningService.stop(this@MainActivity)
            }
            // Dark first, light when the phone is in light mode.
            WeechatTheme { App(radio) }
        }
    }
}
