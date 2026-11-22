package com.weechatradio.app

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import com.github.takahirom.roborazzi.captureRoboImage
import com.weechatradio.app.radio.Attachment
import com.weechatradio.app.radio.BtAudio
import com.weechatradio.app.radio.Folder
import com.weechatradio.app.radio.Link
import com.weechatradio.app.radio.SimulatedRadio
import com.weechatradio.app.radio.Status
import com.weechatradio.app.ui.App
import com.weechatradio.app.ui.FullScreen
import com.weechatradio.app.ui.HomeTab
import com.weechatradio.app.ui.LocalTilesOn
import com.weechatradio.app.ui.Page
import com.weechatradio.app.ui.SetupStep
import com.weechatradio.app.ui.SheetKind
import com.weechatradio.app.ui.Start
import com.weechatradio.app.ui.theme.WeechatTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * Pictures of every design screen (docs/design/android/material), dark and light,
 * rendered on the computer with the simulated radio. Names follow the design PNGs.
 * CI uploads app/build/screens as the "android-screens" artifact. They are for
 * looking at, not compared against saved copies. The map draws without tiles here
 * (no network in tests). Design 22, the notification, is drawn by Android itself.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w412dp-h880dp-xxhdpi")
class ScreenshotTest(private val look: String) {
    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun looks() = listOf("dark", "light")
    }

    @get:Rule
    val compose = createComposeRule()

    private val usb = Status()
    private val bluetooth = Status(link = Link.Bluetooth, rig = "VR-N76", preset = "vhf-fm", inLevel = 0.5f, inDb = -19)
    private val headset = bluetooth.copy(btAudio = BtAudio.HeadsetVox, rig = "UV-36 Pro", preset = "hf-poor")

    private fun shot(name: String, start: Start, status: Status = usb, onAir: Boolean = false) {
        val radio = SimulatedRadio(status, startOnAir = onAir, live = false)
        compose.setContent {
            CompositionLocalProvider(LocalTilesOn provides false) {
                WeechatTheme(dark = look == "dark") { App(radio, start) }
            }
        }
        compose.onRoot().captureRoboImage("build/screens/$look/$name.png")
    }

    @Test fun chat() = shot("01-chat", Start())
    @Test fun chatOnAir() = shot("02-chat-tx", Start(), onAir = true)
    @Test fun chatRooms() = shot("03-chat-rooms", Start(sheet = SheetKind.Channels))
    @Test fun chatKey() = shot("04-chat-key", Start(sheet = SheetKind.Key))
    @Test fun mailList() = shot("05-mail-list", Start(HomeTab.Mail))
    @Test fun mailOpen() = shot("06-mail-open", Start(HomeTab.Mail, Page.MailOpen(3)))
    @Test fun mailOpenSent() = shot("07-mail-open-sent", Start(HomeTab.Mail, Page.MailOpen(7), folder = Folder.Sent))
    private val draft = Page.Compose(
        "wcr-qa@mail.grokbot.com", "Phone attachment test",
        "Sending qa-32k.bin from the phone. Please confirm it arrives intact. 73 de M7TJF", Attachment("qa-32k.bin", "32.0 KB"),
    )
    @Test fun mailCompose() = shot("08-mail-compose", Start(HomeTab.Mail, draft))
    @Test fun mailComposeBluetooth() = shot("08b-mail-compose-bluetooth", Start(HomeTab.Mail, draft), bluetooth)
    @Test fun mailSettings() = shot("09-mail-settings", Start(HomeTab.Mail, sheet = SheetKind.MailSettings))
    @Test fun map() = shot("10-map", Start(HomeTab.Map))
    @Test fun planVhf() = shot("11-plan-vhf", Start(HomeTab.Map, Page.PlanVhf))
    @Test fun planHf() = shot("12-plan-hf-sky", Start(HomeTab.Map, Page.PlanHf))
    @Test fun radioUsb() = shot("13-radio-usb", Start(HomeTab.Radio))
    @Test fun radioBluetooth() = shot("14-radio-bluetooth", Start(HomeTab.Radio), bluetooth)
    @Test fun radioHeadsetVox() = shot("14b-radio-headset-vox", Start(HomeTab.Radio), headset)
    @Test fun station() = shot("15-radio-stats-on-air", Start(HomeTab.Radio, Page.Station))
    @Test fun setupRadio() = shot("16-onboarding-1-radio", Start(full = FullScreen.Setup(SetupStep.Radio)))
    @Test fun setupBluetooth() = shot("17-onboarding-1b-bluetooth", Start(full = FullScreen.Setup(SetupStep.Bluetooth)), bluetooth)
    @Test fun setupHeadsetVox() = shot("17b-onboarding-1b-headset-vox", Start(full = FullScreen.Setup(SetupStep.Bluetooth)), headset)
    @Test fun setupYou() = shot("18-onboarding-2-you", Start(full = FullScreen.Setup(SetupStep.You)))
    @Test fun setupLevels() = shot("19-onboarding-3-levels", Start(full = FullScreen.Setup(SetupStep.Levels)))
    @Test fun setupPermissions() = shot("20-onboarding-4-permissions", Start(full = FullScreen.Setup(SetupStep.Permissions)))
    @Test fun settings() = shot("21-settings", Start(full = FullScreen.Settings))
}
