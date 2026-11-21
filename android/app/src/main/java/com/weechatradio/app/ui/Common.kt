package com.weechatradio.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.weechatradio.app.ui.theme.LocalStatusColors

/** A round badge with "#" for rooms or the first two letters of a callsign or address. */
@Composable
fun Avatar(name: String, size: Dp = 40.dp) {
    val room = name.startsWith("#")
    val accent = LocalStatusColors.current
    val bg = if (room) accent.accentContainer else MaterialTheme.colorScheme.primaryContainer
    val fg = if (room) accent.accent else MaterialTheme.colorScheme.onPrimaryContainer
    val text = when {
        room -> "#"
        name.contains('@') -> name.take(1).uppercase() // an email address
        else -> name.take(2).uppercase() // a callsign
    }
    Box(
        Modifier.size(size).clip(CircleShape).background(bg),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, color = fg, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.SemiBold)
    }
}

/** The small blue section titles ("Found nearby", "Station", "On air just now"). */
@Composable
fun SectionTitle(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        modifier = modifier,
        color = MaterialTheme.colorScheme.primary,
        style = MaterialTheme.typography.titleSmall,
    )
}
