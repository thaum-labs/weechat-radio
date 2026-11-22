package com.weechatradio.app.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Chat
import androidx.compose.material.icons.filled.Mail
import androidx.compose.material.icons.filled.Map
import androidx.compose.material.icons.filled.SettingsInputAntenna
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import com.weechatradio.app.radio.Folder
import com.weechatradio.app.radio.Link
import com.weechatradio.app.radio.Radio

/** The four tabs of the bottom bar. */
enum class HomeTab(val label: String, val icon: ImageVector) {
    Chat("Chat", Icons.AutoMirrored.Filled.Chat),
    Mail("Mail", Icons.Filled.Mail),
    Map("Map", Icons.Filled.Map),
    Radio("Radio", Icons.Filled.SettingsInputAntenna),
}

/** A page opened inside a tab; the bottom bar stays. Back closes it. */
sealed interface Page {
    data class MailOpen(val id: Long) : Page
    data class Compose(val to: String = "", val subject: String = "", val body: String = "", val file: com.weechatradio.app.radio.Attachment? = null) : Page
    data object PlanVhf : Page
    data object PlanHf : Page
    data object Station : Page
}

/** A sheet over the current screen. */
enum class SheetKind { Channels, Key, MailSettings }

/** Full-screen places with no bottom bar. */
sealed interface FullScreen {
    data object Settings : FullScreen
    data class Setup(val step: SetupStep) : FullScreen
}

/** Where the app opens; tests use it to show each design screen. */
data class Start(
    val tab: HomeTab = HomeTab.Chat,
    val page: Page? = null,
    val sheet: SheetKind? = null,
    val full: FullScreen? = null,
    val folder: Folder = Folder.Inbox,
)

@Composable
fun App(radio: Radio, start: Start = Start()) {
    var tab by remember { mutableStateOf(start.tab) }
    var page by remember { mutableStateOf(start.page) }
    var sheet by remember { mutableStateOf(start.sheet) }
    var full by remember { mutableStateOf(start.full ?: if (!radio.status.setUp) FullScreen.Setup(SetupStep.Radio) else null) }
    var folder by remember { mutableStateOf(start.folder) }
    val tiles = LocalTilesOn.current

    BackHandler(enabled = sheet != null || page != null || full != null) {
        when {
            sheet != null -> sheet = null
            full != null -> if (radio.status.setUp) full = null
            page == Page.PlanHf -> page = Page.PlanVhf
            else -> page = null
        }
    }

    // Surface sets the text colour (onSurface) for everything inside.
    Surface(Modifier.fillMaxSize(), color = colors().surface) {
        Box(Modifier.fillMaxSize().systemBarsPadding().imePadding()) {
            when (val f = full) {
                FullScreen.Settings -> SettingsScreen(radio, { full = null }) { full = FullScreen.Setup(it) }
                is FullScreen.Setup -> SetupScreen(
                    radio, f.step,
                    onStep = { full = FullScreen.Setup(it) },
                    onClose = { full = if (radio.status.setUp) null else f },
                    onDone = { radio.update { it.copy(setUp = true, listening = true) }; full = null; tab = HomeTab.Chat },
                )
                null -> Column(Modifier.fillMaxSize()) {
                    Box(Modifier.weight(1f)) {
                        val settings = { full = FullScreen.Settings }
                        when (val p = page) {
                            is Page.MailOpen -> MailOpenScreen(radio, p.id, { page = null }) { m -> page = Page.Compose(m.peer, "Re: " + m.subject.removePrefix("Re: ")) }
                            is Page.Compose -> ComposeScreen(radio, p.to, p.subject, p.body, p.file, { page = null }) { full = FullScreen.Setup(SetupStep.Radio) }
                            Page.PlanVhf -> PlanVhfScreen(radio, { page = null }) { page = Page.PlanHf }
                            Page.PlanHf -> PlanHfScreen(radio) { page = Page.PlanVhf }
                            Page.Station -> StationScreen(radio, { page = null }, settings)
                            null -> when (tab) {
                                HomeTab.Chat -> ChatScreen(radio, { sheet = SheetKind.Channels }, { sheet = SheetKind.Key }, settings)
                                HomeTab.Mail -> MailListScreen(radio, folder, { folder = it }, { page = Page.MailOpen(it.id) }, { page = Page.Compose() }) { sheet = SheetKind.MailSettings }
                                HomeTab.Map -> MapScreen(radio, tiles, { page = Page.PlanVhf }, settings)
                                HomeTab.Radio -> RadioScreen(radio, { page = Page.Station }, settings) { full = FullScreen.Setup(SetupStep.Radio) }
                            }
                        }
                        when (sheet) {
                            SheetKind.Channels -> ChannelsSheet(radio) { sheet = null }
                            SheetKind.Key -> KeySheet { sheet = null }
                            SheetKind.MailSettings -> MailSettingsSheet(radio, { sheet = null }) { sheet = null; full = FullScreen.Settings }
                            null -> {}
                        }
                    }
                    BottomBar(radio, tab) { tab = it; page = null; sheet = null }
                }
            }
        }
}
}

@Composable
private fun BottomBar(radio: Radio, tab: HomeTab, onTab: (HomeTab) -> Unit) {
    val mailUnread = radio.mails.count { it.folder == Folder.Inbox && it.unread }
    val chatUnread = radio.channels.any { it.unread > 0 }
    NavigationBar(containerColor = colors().surfaceContainer, windowInsets = WindowInsets(0)) {
        HomeTab.entries.forEach { t ->
            NavigationBarItem(
                selected = t == tab,
                onClick = { onTab(t) },
                icon = {
                    BadgedBox(badge = {
                        when {
                            t == HomeTab.Mail && mailUnread > 0 -> Badge { Text("$mailUnread") }
                            t == HomeTab.Chat && chatUnread -> Badge()
                            t == HomeTab.Radio && radio.status.link == Link.InternetOnly -> {}
                        }
                    }) { Icon(t.icon, t.label) }
                },
                label = { Text(t.label) },
            )
        }
    }
}
