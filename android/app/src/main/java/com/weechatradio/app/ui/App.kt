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
import androidx.compose.foundation.clickable
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.weechatradio.app.ui.theme.Console
import com.weechatradio.app.radio.Folder
import com.weechatradio.app.radio.Link
import com.weechatradio.app.radio.LiveRadio
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.ui.theme.Look

/** The four tabs of the bottom bar; [console] is the modem73 console's word for it. */
enum class HomeTab(val label: String, val console: String, val icon: ImageVector) {
    Chat("Chats", "CHAT", Icons.AutoMirrored.Filled.Chat),
    Mail("Mail", "MAIL", Icons.Filled.Mail),
    Map("Map", "MAP", Icons.Filled.Map),
    Radio("Radio", "RADIO", Icons.Filled.SettingsInputAntenna),
}

/** A page opened inside a tab; the bottom bar stays. Back closes it. */
sealed interface Page {
    /** One chat, opened from the Chats list. */
    data class Conversation(val name: String) : Page
    data class MailOpen(val id: Long) : Page
    data class Compose(val to: String = "", val subject: String = "", val body: String = "", val file: com.weechatradio.app.radio.Attachment? = null) : Page
    /** Real radio: one mail from the station's store, and a new mail. */
    data class LiveMailOpen(val id: String) : Page
    data class LiveCompose(val to: String = "", val subject: String = "") : Page
    data object PlanVhf : Page
    data object PlanHf : Page
    data object Station : Page
}

/** A sheet over the current screen. */
enum class SheetKind { NewChat, MailSettings }

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
fun App(radio: Radio, start: Start = Start(), look: Look = Look.Material, onLook: (Look) -> Unit = {}) {
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

    val openChat = { name: String -> radio.openChannel(name); tab = HomeTab.Chat; page = Page.Conversation(name) }
    // Surface sets the text colour (onSurface) for everything inside.
    Surface(Modifier.fillMaxSize(), color = colors().surface) {
        Column(Modifier.fillMaxSize().systemBarsPadding().imePadding()) {
            if (look == Look.Modem73) ConsoleHeader(radio)
            Box(Modifier.weight(1f)) {
                when (val f = full) {
                    FullScreen.Settings -> SettingsScreen(radio, look, onLook, { full = null }) { full = FullScreen.Setup(it) }
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
                                is Page.Conversation -> ConversationScreen(radio, p.name) { page = null }
                                is Page.MailOpen -> MailOpenScreen(radio, p.id, { page = null }) { m -> page = Page.Compose(m.peer, "Re: " + m.subject.removePrefix("Re: ")) }
                                is Page.Compose -> ComposeScreen(radio, p.to, p.subject, p.body, p.file, { page = null }) { full = FullScreen.Setup(SetupStep.Radio) }
                                Page.PlanVhf -> PlanVhfScreen(radio, { page = null }) { page = Page.PlanHf }
                                Page.PlanHf -> PlanHfScreen(radio) { page = Page.PlanVhf }
                                is Page.LiveMailOpen -> (radio as? LiveRadio)?.let { live -> LiveMailOpenScreen(live, p.id, { page = null }) { to, subj -> page = Page.LiveCompose(to, subj) } }
                                is Page.LiveCompose -> (radio as? LiveRadio)?.let { live -> LiveComposeScreen(live, p.to, p.subject) { page = null } }
                                Page.Station -> if (radio is LiveRadio) OnAirScreen(radio, { page = null }, openChat) else StationScreen(radio, { page = null }, settings)
                                null -> when (tab) {
                                    HomeTab.Chat -> ChatsScreen(radio, openChat) { sheet = SheetKind.NewChat }
                                    HomeTab.Mail -> if (radio is LiveRadio) LiveMailScreen(radio, { page = Page.LiveMailOpen(it) }) { page = Page.LiveCompose() } else MailListScreen(radio, folder, { folder = it }, { page = Page.MailOpen(it.id) }, { page = Page.Compose() }) { sheet = SheetKind.MailSettings }
                                    HomeTab.Map -> MapScreen(radio, tiles, { page = Page.PlanVhf }, openChat, settings)
                                    HomeTab.Radio -> RadioScreen(radio, { page = Page.Station }, settings) { full = FullScreen.Setup(SetupStep.Radio) }
                                }
                            }
                            when (sheet) {
                                SheetKind.NewChat -> NewChatSheet(radio, { sheet = null }) { sheet = null; openChat(it) }
                                SheetKind.MailSettings -> MailSettingsSheet(radio, { sheet = null }) { sheet = null; full = FullScreen.Settings }
                                null -> {}
                            }
                        }
                        val onTab = { t: HomeTab -> tab = t; page = null; sheet = null }
                        if (look == Look.Modem73) ConsoleBar(radio, tab, onTab) else BottomBar(radio, tab, onTab)
                    }
                }
            }
        }
    }
}

/** The console's top line: "/// WEECHAT RADIO", your callsign and [RX], [TX] or [--]. */
@Composable
private fun ConsoleHeader(radio: Radio) {
    val state = radioState(radio)
    val line = colors().outline
    Row(
        Modifier.fillMaxWidth().height(44.dp)
            .drawBehind { drawLine(line, Offset(0f, size.height), Offset(size.width, size.height), 1.dp.toPx()) }
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            buildAnnotatedString {
                withStyle(SpanStyle(color = Console.me)) { append("/") }
                withStyle(SpanStyle(color = Console.orange)) { append("/") }
                withStyle(SpanStyle(color = Console.lilac)) { append("/") }
            },
            fontSize = 13.sp, fontWeight = FontWeight.Bold,
        )
        Text("WEECHAT RADIO", Modifier.weight(1f), fontSize = 13.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp)
        Text(radio.status.call, fontSize = 13.sp)
        Text(state.tag, fontSize = 13.sp, fontWeight = FontWeight.Bold, color = state.color())
    }
}

/** The console's bottom bar: words, a green line over the open tab, the radio's state as a square. */
@Composable
private fun ConsoleBar(radio: Radio, tab: HomeTab, onTab: (HomeTab) -> Unit) {
    val mailUnread = mailUnread(radio)
    val chatUnread = radio.channels.any { it.unread > 0 }
    val line = colors().outline
    val green = radioColors().ok
    val state = radioState(radio).color()
    Row(
        Modifier.fillMaxWidth().height(56.dp).background(colors().surfaceContainer)
            .drawBehind { drawLine(line, Offset(0f, 0f), Offset(size.width, 0f), 1.dp.toPx()) },
    ) {
        HomeTab.entries.forEach { t ->
            val on = t == tab
            Row(
                Modifier.weight(1f).fillMaxHeight().clickable { onTab(t) }
                    .drawBehind { if (on) drawRect(green, size = Size(size.width, 2.dp.toPx())) },
                horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (t == HomeTab.Radio) Box(Modifier.size(8.dp).background(state))
                Text(t.console, fontSize = 13.sp, letterSpacing = 1.sp, fontWeight = if (on) FontWeight.Bold else FontWeight.Normal, color = if (on) green else colors().onSurfaceVariant)
                if (t == HomeTab.Chat && chatUnread) Text("*", color = Console.gold, fontSize = 13.sp)
                if (t == HomeTab.Mail && mailUnread > 0) Text("$mailUnread", color = Console.gold, fontSize = 13.sp)
            }
        }
    }
}

@Composable
private fun BottomBar(radio: Radio, tab: HomeTab, onTab: (HomeTab) -> Unit) {
    val mailUnread = mailUnread(radio)
    val chatUnread = radio.channels.any { it.unread > 0 }
    val state = radioState(radio).color()
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
                            // The radio's state at a glance: green linked, orange sending, red not linked.
                            t == HomeTab.Radio -> Badge(containerColor = state)
                        }
                    }) { Icon(t.icon, t.label) }
                },
                label = { Text(t.label) },
            )
        }
    }
}

/** Unread mail for the tab badge: the station's own count on the real radio. */
private fun mailUnread(radio: Radio): Int =
    (radio as? LiveRadio)?.mailTab?.unread ?: radio.mails.count { it.folder == Folder.Inbox && it.unread }
