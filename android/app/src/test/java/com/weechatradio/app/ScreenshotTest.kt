package com.weechatradio.app

import androidx.compose.runtime.Composable
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import com.github.takahirom.roborazzi.captureRoboImage
import com.weechatradio.app.radio.RadioLink
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
import com.weechatradio.app.ui.VoxSetupScreen
import com.weechatradio.app.ui.theme.ThemeChoice
import com.weechatradio.app.ui.theme.WeeChatRadioTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * Pictures of every screen in each look, rendered on the computer (no phone).
 * CI uploads app/build/screens as the "android-screens" artifact. They are for
 * looking at, not compared against saved copies.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w411dp-h891dp-xxhdpi")
class ScreenshotTest(private val look: String) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun looks() = listOf("standard-light", "standard-dark", "modem73")
    }

    @get:Rule
    val compose = createComposeRule()

    private val radio = SimulatedRadio(
        RadioStatus(connected = true, hearing = 0.62f, channelBusy = 0.04f, snrDb = 7, sentToday = 14, heardToday = 31),
    )
    private val theme = if (look == "modem73") ThemeChoice.Modem73 else ThemeChoice.Standard

    private fun shot(name: String, content: @Composable () -> Unit) {
        compose.setContent { WeeChatRadioTheme(theme, dark = look != "standard-light") { content() } }
        compose.onRoot().captureRoboImage("build/screens/$look/$name.png")
    }

    @Test fun connect() = shot("1-connect") { ConnectRadioScreen(SimulatedRadio()) }
    @Test fun chats() = shot("2-chats") { ChatsScreen(radio) {} }
    @Test fun conversation() = shot("3-conversation") { ConversationScreen(radio, "M0ABC") {} }
    @Test fun mail() = shot("4-mail") { MailScreen(radio) {} }
    @Test fun readMail() = shot("5-read-mail") { ReadMailScreen(radio, 12, {}, {}) }
    @Test fun newMail() = shot("6-new-mail") { NewMailScreen(radio, "sam@example.com", "Re: Weekend plans") {} }
    @Test fun radioTab() = shot("7-radio") { RadioScreen(radio) {} }
    @Test fun settings() = shot("8-settings") { SettingsScreen(radio, theme, {}) {} }

    // VOX radios (any radio, keyed by its own VOX).
    private val voxRadio = SimulatedRadio(
        RadioStatus(
            connected = true, radioName = "UV-36 Pro", link = RadioLink.VoxHeadset, preset = "hf-poor",
            hearing = 0.55f, channelBusy = 0.02f, snrDb = 6, sentToday = 3, heardToday = 9,
        ),
    )
    @Test fun voxHeadsetSetup() = shot("9-vox-headset-setup") { VoxSetupScreen(RadioLink.VoxHeadset, {}) {} }
    @Test fun voxCableSetup() = shot("10-vox-cable-setup") { VoxSetupScreen(RadioLink.VoxCable, {}) {} }
    @Test fun voxRadioTab() = shot("11-radio-vox") { RadioScreen(voxRadio) {} }
    @Test fun voxSettings() = shot("12-settings-vox") { SettingsScreen(voxRadio, theme, {}) {} }
}
