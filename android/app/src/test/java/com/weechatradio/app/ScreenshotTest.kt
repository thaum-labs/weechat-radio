package com.weechatradio.app

import androidx.compose.runtime.Composable
import app.cash.paparazzi.DeviceConfig
import app.cash.paparazzi.Paparazzi
import com.weechatradio.app.radio.RadioStatus
import com.weechatradio.app.radio.SimulatedRadio
import com.weechatradio.app.ui.ChatsScreen
import com.weechatradio.app.ui.ConnectRadioScreen
import com.weechatradio.app.ui.ConversationScreen
import com.weechatradio.app.ui.MailScreen
import com.weechatradio.app.ui.NewMailScreen
import com.weechatradio.app.ui.RadioScreen
import com.weechatradio.app.ui.ReadMailScreen
import com.weechatradio.app.ui.SettingsScreen
import com.weechatradio.app.ui.theme.ThemeChoice
import com.weechatradio.app.ui.theme.WeeChatRadioTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

/**
 * Pictures of every screen in each look, rendered on the computer (no phone).
 * CI uploads them as the "android-screens" artifact. They are for looking at,
 * not compared against saved copies.
 */
@RunWith(Parameterized::class)
class ScreenshotTest(private val look: String) {
    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{0}")
        fun looks() = listOf("standard-light", "standard-dark", "modem73")
    }

    @get:Rule
    val paparazzi = Paparazzi(
        deviceConfig = DeviceConfig.PIXEL_5,
        maxPercentDifference = 100.0,
    )

    private val radio = SimulatedRadio(
        RadioStatus(connected = true, hearing = 0.62f, channelBusy = 0.04f, snrDb = 7, sentToday = 14, heardToday = 31),
    )
    private val theme = if (look == "modem73") ThemeChoice.Modem73 else ThemeChoice.Standard

    private fun shot(name: String, content: @Composable () -> Unit) =
        paparazzi.snapshot(name) { WeeChatRadioTheme(theme, dark = look != "standard-light") { content() } }

    @Test fun connect() = shot("1-connect") { ConnectRadioScreen(SimulatedRadio()) }
    @Test fun chats() = shot("2-chats") { ChatsScreen(radio) {} }
    @Test fun conversation() = shot("3-conversation") { ConversationScreen(radio, "M0ABC") {} }
    @Test fun mail() = shot("4-mail") { MailScreen(radio) {} }
    @Test fun readMail() = shot("5-read-mail") { ReadMailScreen(radio, 12, {}, {}) }
    @Test fun newMail() = shot("6-new-mail") { NewMailScreen(radio, "sam@example.com", "Re: Weekend plans") {} }
    @Test fun radioTab() = shot("7-radio") { RadioScreen(radio) {} }
    @Test fun settings() = shot("8-settings") { SettingsScreen(radio, theme, {}) {} }
}
