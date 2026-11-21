package com.weechatradio.app.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Chat
import androidx.compose.material.icons.filled.Email
import androidx.compose.material.icons.filled.Sensors
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.ui.theme.ThemeChoice

enum class Tab(val label: String) { Chats("Chats"), Mail("Mail"), Radio("Radio") }

/** A screen opened on top of the tabs. Back closes it. */
sealed interface Page {
    data class Conversation(val name: String) : Page
    data class ReadMail(val id: Long) : Page
    data class NewMail(val to: String = "", val subject: String = "") : Page
    data object Settings : Page
}

@Composable
fun App(radio: Radio, theme: ThemeChoice, onTheme: (ThemeChoice) -> Unit) {
    if (!radio.status.connected) {
        ConnectRadioScreen(radio)
        return
    }
    var tab by rememberSaveable { mutableStateOf(Tab.Chats) }
    val pages = remember { mutableStateListOf<Page>() }
    val open: (Page) -> Unit = { pages.add(it) }
    val back: () -> Unit = { if (pages.isNotEmpty()) pages.removeAt(pages.lastIndex) }
    BackHandler(enabled = pages.isNotEmpty(), onBack = back)

    when (val page = pages.lastOrNull()) {
        is Page.Conversation -> ConversationScreen(radio, page.name, back)
        is Page.ReadMail -> ReadMailScreen(radio, page.id, back, open)
        is Page.NewMail -> NewMailScreen(radio, page.to, page.subject, back)
        Page.Settings -> SettingsScreen(radio, theme, onTheme, back)
        null -> Scaffold(
            bottomBar = {
                NavigationBar {
                    Tab.entries.forEach { t ->
                        NavigationBarItem(
                            selected = tab == t,
                            onClick = { tab = t },
                            label = { Text(t.label) },
                            icon = { TabIcon(t, radio) },
                        )
                    }
                }
            },
        ) { padding ->
            Box(Modifier.padding(padding).consumeWindowInsets(padding)) {
                when (tab) {
                    Tab.Chats -> ChatsScreen(radio, open)
                    Tab.Mail -> MailScreen(radio, open)
                    Tab.Radio -> RadioScreen(radio, open)
                }
            }
        }
    }
}

@Composable
private fun TabIcon(tab: Tab, radio: Radio) {
    when (tab) {
        Tab.Chats -> {
            val unread = radio.chats.sumOf { it.unread }
            BadgedBox(badge = { if (unread > 0) Badge { Text("$unread") } }) {
                Icon(Icons.AutoMirrored.Filled.Chat, contentDescription = null)
            }
        }
        Tab.Mail -> Icon(Icons.Filled.Email, contentDescription = null)
        Tab.Radio -> Icon(Icons.Filled.Sensors, contentDescription = null)
    }
}
