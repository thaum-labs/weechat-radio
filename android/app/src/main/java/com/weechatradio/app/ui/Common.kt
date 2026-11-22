package com.weechatradio.app.ui

import android.graphics.Bitmap
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.weechatradio.app.radio.Mark
import com.weechatradio.app.radio.Mode
import com.weechatradio.app.radio.OnAir
import com.weechatradio.app.radio.Radio
import com.weechatradio.app.radio.Status
import com.weechatradio.app.ui.theme.LocalRadioColors
import com.weechatradio.app.ui.theme.MeterColors
import com.weechatradio.app.ui.theme.ModeColors
import com.weechatradio.app.ui.theme.Mono
import com.weechatradio.app.ui.theme.WaterfallColors

// Small building blocks shared by the screens. Sizes follow the Material mockups.

val TxInk = Color(0xFF2A0D00)

@Composable fun colors() = MaterialTheme.colorScheme

@Composable fun radioColors() = LocalRadioColors.current

fun modeColor(m: Mode): Color = when (m) {
    Mode.Internet -> ModeColors.internet
    Mode.InternetRadio -> ModeColors.internetRadio
    Mode.Radio -> ModeColors.radio
    Mode.RadioPlus -> ModeColors.radioPlus
}

/** Monospace text: callsigns, frequencies, grid squares, preset codes, mail addresses. */
@Composable
fun MonoText(
    text: String,
    modifier: Modifier = Modifier,
    color: Color = Color.Unspecified,
    size: TextUnit = 13.sp,
    weight: FontWeight? = null,
) = Text(text, modifier, color = color, fontSize = size, fontFamily = Mono, fontWeight = weight, maxLines = 1, overflow = TextOverflow.Ellipsis)

@Composable
fun Small(text: String, modifier: Modifier = Modifier, color: Color = colors().onSurfaceVariant, size: TextUnit = 12.sp, lineHeight: TextUnit = 17.sp) =
    Text(text, modifier, color = color, fontSize = size, lineHeight = lineHeight)

/** The weechat-radio mark: a prompt chevron and radio waves in the mode colour. */
@Composable
fun BrandMark(size: Dp = 26.dp, wave: Color = ModeColors.radioPlus, fg: Color = colors().onSurface) {
    Canvas(Modifier.size(size, size * 60f / 64f)) {
        val k = this.size.width / 64f
        fun p(x: Float, y: Float) = Offset(x * k, y * k)
        val chevron = Path().apply { moveTo(8 * k, 20 * k); lineTo(20 * k, 30 * k); lineTo(8 * k, 40 * k) }
        drawPath(chevron, fg, style = Stroke(5 * k, cap = StrokeCap.Round, join = StrokeJoin.Round))
        drawRect(wave, p(25f, 36f), Size(15 * k, 5 * k))
        listOf(7f to 1f, 13f to 0.7f, 19f to 0.4f).forEach { (rad, a) ->
            drawArc(
                wave.copy(alpha = a), startAngle = 180f, sweepAngle = 90f, useCenter = false,
                topLeft = p(40f - rad, 36f - rad), size = Size(2 * rad * k, 2 * rad * k),
                style = Stroke(4 * k, cap = StrokeCap.Round),
            )
        }
    }
}

@Composable
fun IconBtn(icon: ImageVector, label: String, tonal: Boolean = false, tint: Color = colors().onSurfaceVariant, onClick: () -> Unit) {
    if (tonal) {
        FilledTonalIconButton(onClick, Modifier.padding(horizontal = 4.dp).size(40.dp)) { Icon(icon, label) }
    } else {
        IconButton(onClick, Modifier.size(48.dp)) { Icon(icon, label, tint = tint) }
    }
}

/** The 64 dp top bar: a lead (logo or back), title and subtitle, actions. */
@Composable
fun TopBar(
    title: String,
    sub: String? = null,
    subMono: Boolean = false,
    titleSize: TextUnit = 22.sp,
    lead: (@Composable () -> Unit)? = null,
    onTitle: (() -> Unit)? = null,
    titleTrail: (@Composable () -> Unit)? = null,
    actions: @Composable RowScope.() -> Unit = {},
) {
    Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        if (lead != null) lead() else Spacer(Modifier.width(12.dp))
        Column(
            Modifier.weight(1f).padding(start = 8.dp)
                .then(if (onTitle != null) Modifier.clickable(onClick = onTitle) else Modifier),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(title, fontSize = titleSize, lineHeight = 28.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                titleTrail?.invoke()
            }
            if (sub != null) {
                if (subMono) MonoText(sub, color = colors().onSurfaceVariant, size = 11.sp)
                else Text(sub, fontSize = 12.sp, lineHeight = 16.sp, color = colors().onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        actions()
    }
}

@Composable
fun LogoLead() = Box(Modifier.width(48.dp), contentAlignment = Alignment.Center) { BrandMark() }

@Composable
fun BackLead(onBack: () -> Unit) = IconBtn(Icons.AutoMirrored.Filled.ArrowBack, "Back", onClick = onBack)

/** A Material-style chip drawn by hand so every look in the mockups fits one helper. */
@Composable
fun Chip(
    text: String,
    modifier: Modifier = Modifier,
    selected: Boolean = false,
    small: Boolean = false,
    mono: Boolean = false,
    container: Color? = null,
    content: Color? = null,
    border: Color? = null,
    icon: ImageVector? = null,
    lead: (@Composable () -> Unit)? = null,
    trail: (@Composable () -> Unit)? = null,
    onClick: (() -> Unit)? = null,
) {
    val bg = container ?: if (selected) colors().secondaryContainer else Color.Transparent
    val fg = content ?: if (selected) colors().onSecondaryContainer else colors().onSurfaceVariant
    val line = border ?: if (selected || container != null) Color.Transparent else colors().outlineVariant
    val shape = RoundedCornerShape(if (small) 6.dp else 8.dp)
    Row(
        modifier.height(if (small) 24.dp else 32.dp).clip(shape).background(bg).border(1.dp, line, shape)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = if (small) 8.dp else 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(if (small) 4.dp else 6.dp),
    ) {
        if (selected && icon == null && lead == null && !small) Icon(Icons.Filled.Check, null, Modifier.size(16.dp), tint = fg)
        if (icon != null) Icon(icon, null, Modifier.size(if (small) 14.dp else 18.dp), tint = fg)
        lead?.invoke()
        if (mono) MonoText(text, color = fg, size = if (small) 12.sp else 13.sp)
        else Text(text, color = fg, fontSize = if (small) 12.sp else 14.sp, fontWeight = FontWeight.Medium, maxLines = 1)
        trail?.invoke()
    }
}

@Composable
fun Dot(color: Color, size: Dp = 8.dp) = Box(Modifier.size(size).clip(CircleShape).background(color))

/** IN / OUT meter with the desktop zones: green to 66 %, purple to 88 %, orange to the top. */
@Composable
fun Meter(level: Float, modifier: Modifier = Modifier, height: Dp = 10.dp) {
    val track = colors().surfaceContainerHighest
    Canvas(modifier.fillMaxWidth().height(height).clip(RoundedCornerShape(height / 2)).background(track)) {
        val w = size.width
        listOf(Triple(0f, 0.66f, MeterColors.green), Triple(0.66f, 0.88f, MeterColors.purple), Triple(0.88f, 1f, MeterColors.orange))
            .forEach { (a, b, col) ->
                val e = minOf(level, b)
                if (e > a) drawRect(col, Offset(a * w, 0f), Size((e - a) * w, size.height))
            }
    }
}

@Composable
fun MeterRow(label: String, level: Float, db: Int) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Label(label, Modifier.width(30.dp))
        Meter(level, Modifier.weight(1f))
        MonoText("$db dB", Modifier.width(56.dp), color = radioColors().ok, size = 13.sp)
    }
}

/** Small caps-style label ("IN", "JOURNEY · LOCAL TIME"). */
@Composable
fun Label(text: String, modifier: Modifier = Modifier, color: Color = colors().onSurfaceVariant) =
    Text(text, modifier, fontSize = 12.sp, lineHeight = 16.sp, fontWeight = FontWeight.Medium, letterSpacing = 0.5.sp, color = color)

/** Section heading in lists and Settings. */
@Composable
fun Section(text: String, top: Dp = 16.dp) =
    Text(text, Modifier.padding(start = 16.dp, end = 16.dp, top = top, bottom = 8.dp), fontSize = 14.sp, fontWeight = FontWeight.Medium, color = colors().primary)

@Composable
fun Card(modifier: Modifier = Modifier, color: Color = colors().surfaceContainerHigh, padding: PaddingValues = PaddingValues(16.dp), content: @Composable ColumnScope.() -> Unit) =
    Column(modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(color).padding(padding), content = content)

/** The receipt mark of a line, as in the "What the marks mean" sheet. */
@Composable
fun MarkIcon(mark: Mark, size: Dp = 15.dp) {
    val (icon, tint) = when (mark) {
        Mark.Queued -> Icons.Filled.Schedule to colors().onSurfaceVariant
        Mark.Sent -> Icons.Filled.Done to colors().onSurfaceVariant
        Mark.Relayed -> Icons.Filled.Repeat to colors().onSurfaceVariant
        Mark.Gateway -> Icons.Filled.DeviceHub to colors().onSurfaceVariant
        Mark.Delivered, Mark.AllMembers -> Icons.Filled.DoneAll to radioColors().ok
        Mark.Refused -> Icons.Filled.Error to colors().error
    }
    Icon(icon, mark.name, Modifier.size(size), tint = tint)
}

/** The station strip under the chat bar: call, rig, PTT, IN and preset. Turns TX-coloured while keying. */
@Composable
fun StationStrip(s: Status, tx: Boolean) {
    val bg = if (tx) radioColors().txContainer else colors().surfaceContainerHigh
    val fg = if (tx) radioColors().onTxContainer else colors().onSurface
    Row(
        Modifier.padding(start = 12.dp, end = 12.dp, bottom = 6.dp).fillMaxWidth().height(40.dp)
            .clip(RoundedCornerShape(12.dp)).background(bg).padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        MonoText(s.call, color = fg, weight = FontWeight.Medium)
        if (s.linked) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                Dot(radioColors().ok); Text(s.rig, color = fg, fontSize = 13.sp, maxLines = 1)
            }
            if (tx) Text("PTT TX", color = radioColors().tx, fontSize = 13.sp, fontWeight = FontWeight.Bold)
            else Row { Text("PTT ", color = colors().onSurfaceVariant, fontSize = 13.sp); Text("RX", color = fg, fontSize = 13.sp, fontWeight = FontWeight.Medium) }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                Text("IN", color = colors().onSurfaceVariant, fontSize = 13.sp)
                Meter(if (tx) 0.2f else s.inLevel, Modifier.width(40.dp), height = 6.dp)
            }
        } else {
            Text("internet only", color = colors().onSurfaceVariant, fontSize = 13.sp)
        }
        Spacer(Modifier.weight(1f))
        MonoText(if (s.linked) s.preset else "hub", color = if (tx) radioColors().onTxContainer else colors().primary, size = 12.sp)
    }
}

/** Orange "On air" banner with Stop, shown while a message is keyed. */
@Composable
fun OnAirBanner(s: Status, air: OnAir, onStop: () -> Unit) {
    val how = if (s.link == com.weechatradio.app.radio.Link.Bluetooth) "${s.rig} Bluetooth" else "${s.rig.substringBefore(' ')} ${s.ptt}"
    Box(Modifier.padding(start = 12.dp, end = 12.dp, bottom = 6.dp).fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(radioColors().txContainer)) {
        Row(
            Modifier.padding(start = 16.dp, end = 12.dp, top = 12.dp, bottom = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Box(
                Modifier.size(52.dp).clip(CircleShape).background(radioColors().tx.copy(alpha = 0.22f)).padding(6.dp).clip(CircleShape).background(radioColors().tx),
                contentAlignment = Alignment.Center,
            ) { Icon(Icons.Filled.SettingsInputAntenna, null, tint = TxInk) }
            Column(Modifier.weight(1f)) {
                Text("On air · $how", color = radioColors().onTxContainer, fontSize = 16.sp, lineHeight = 22.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    androidx.compose.ui.text.buildAnnotatedString {
                        pushStyle(androidx.compose.ui.text.SpanStyle(fontFamily = Mono)); append(s.freq); pop()
                        append(" ${s.emission} · piece ${air.piece} of ${air.of} · 00:%02d".format(air.seconds))
                    },
                    color = radioColors().onTxContainer.copy(alpha = 0.85f), fontSize = 13.sp, maxLines = 1,
                )
            }
            Button(onStop, colors = ButtonDefaults.buttonColors(containerColor = radioColors().tx, contentColor = TxInk), contentPadding = PaddingValues(horizontal = 16.dp), modifier = Modifier.height(36.dp)) {
                Text("Stop")
            }
        }
        Box(Modifier.align(Alignment.BottomStart).fillMaxWidth(air.progress.coerceIn(0f, 1f)).height(4.dp).background(radioColors().tx))
    }
}

/** A bottom sheet drawn inside the app window (scrim + rounded panel), so it also shows in test pictures. */
@Composable
fun Sheet(onClose: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    Box(Modifier.fillMaxSize()) {
        Box(
            Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.52f))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClose),
        )
        Column(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth()
                .clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp)).background(colors().surfaceContainerLow)
                .clickable(remember { MutableInteractionSource() }, indication = null) {}
                .padding(bottom = 24.dp),
        ) {
            Box(Modifier.padding(top = 16.dp, bottom = 8.dp).align(Alignment.CenterHorizontally).size(32.dp, 4.dp).clip(CircleShape).background(colors().onSurfaceVariant.copy(alpha = 0.4f)))
            content()
        }
    }
}

/** Snackbar-style note with an optional action. */
@Composable
fun Note(text: String, modifier: Modifier = Modifier, action: String? = null, onAction: () -> Unit = {}) {
    Row(
        modifier.padding(horizontal = 8.dp).fillMaxWidth().heightIn(min = 48.dp).clip(RoundedCornerShape(4.dp))
            .background(colors().inverseSurface).padding(start = 16.dp, end = 8.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, Modifier.weight(1f), color = colors().inverseOnSurface, fontSize = 14.sp)
        if (action != null) TextButton(onAction) { Text(action, color = colors().inversePrimary) }
    }
}

/** Two to three choices in one outlined pill, like the Material segmented button. */
@Composable
fun Segmented(options: List<String>, selected: Int, onSelect: (Int) -> Unit) {
    val shape = RoundedCornerShape(20.dp)
    Row(Modifier.fillMaxWidth().height(40.dp).clip(shape).border(1.dp, colors().outline, shape)) {
        options.forEachIndexed { i, label ->
            if (i > 0) Box(Modifier.fillMaxHeight().width(1.dp).background(colors().outline))
            val on = i == selected
            Row(
                Modifier.weight(1f).fillMaxHeight().background(if (on) colors().secondaryContainer else Color.Transparent).clickable { onSelect(i) },
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (on) Icon(Icons.Filled.Check, null, Modifier.size(18.dp).padding(end = 2.dp), tint = colors().onSecondaryContainer)
                Text(label, fontSize = 13.sp, fontWeight = FontWeight.Medium, maxLines = 1, color = if (on) colors().onSecondaryContainer else colors().onSurface)
            }
        }
    }
}

/** Big orange frequency, as on the Radio tab and the planner. */
@Composable
fun BigFreq(freq: String, sub: String, size: TextUnit = 36.sp) {
    Column {
        Row(verticalAlignment = Alignment.Bottom) {
            Text(freq, fontFamily = Mono, fontSize = size, lineHeight = size, fontWeight = FontWeight.Medium, letterSpacing = (-1).sp, color = radioColors().tx)
            Text(" MHz", fontSize = 15.sp, color = radioColors().tx, modifier = Modifier.padding(bottom = 4.dp))
        }
        Text(sub, fontSize = 13.sp, color = radioColors().tx)
    }
}

/** Band chips with a works / maybe / won't-work dot. */
data class BandChip(val label: String, val verdict: Verdict, val freq: String, val emission: String, val preset: String)

enum class Verdict { Works, Maybe, No }

@Composable
fun verdictColor(v: Verdict) = when (v) {
    Verdict.Works -> radioColors().ok
    Verdict.Maybe -> radioColors().warn
    Verdict.No -> colors().outline
}

@Composable
fun BandChips(bands: List<BandChip>, selected: String, onPick: (BandChip) -> Unit) {
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        bands.forEach { b ->
            Chip(b.label, selected = b.label == selected, lead = { Dot(verdictColor(b.verdict), 10.dp) }, onClick = { onPick(b) })
        }
    }
}

@Composable
fun BandLegend() {
    Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
        listOf(radioColors().ok to "works", radioColors().warn to "skips, may or may not work", colors().outline to "won't work").forEach { (col, t) ->
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                Dot(col); Text(t, fontSize = 12.sp, color = colors().onSurfaceVariant, maxLines = 1)
            }
        }
    }
}

/** A warning-coloured card: icon, bold lead, text. */
@Composable
fun WarnCard(lead: String, text: String, ok: Boolean = false, icon: ImageVector = Icons.Filled.Warning) {
    val bg = if (ok) radioColors().okContainer else radioColors().warnContainer
    val fg = if (ok) colors().onSurface else radioColors().onWarnContainer
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(bg).padding(horizontal = 16.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Icon(icon, null, Modifier.size(22.dp), tint = fg)
        Text(
            androidx.compose.ui.text.buildAnnotatedString {
                pushStyle(androidx.compose.ui.text.SpanStyle(fontWeight = FontWeight.Medium)); append(lead); pop()
                append(" "); append(text)
            },
            color = fg, fontSize = 14.sp, lineHeight = 20.sp,
        )
    }
}

/** The frequencies the QSY menu and band chips offer. PMR446 is included (Tobias, 2026-11-22). */
val Bands = listOf(
    BandChip("70cm", Verdict.Maybe, "433.625", "NFM", "vhf-fast"),
    BandChip("2m", Verdict.Works, "144.950", "NFM", "vhf-fm"),
    BandChip("PMR446", Verdict.Works, "446.00625", "NFM", "vhf-fm"),
    BandChip("80m", Verdict.Maybe, "3.580", "USB", "hf-poor"),
    BandChip("40m", Verdict.Maybe, "7.040", "USB", "hf-poor"),
    BandChip("20m", Verdict.No, "14.105", "USB", "hf-good"),
    BandChip("10m", Verdict.No, "28.120", "USB", "hf-good"),
)

/** Waterfall card: newest row at the top, the desktop palette, a sample of the last 30 s. */
@Composable
fun WaterfallCard(radio: Radio, height: Dp, tx: Boolean, compact: Boolean = false) {
    Card(padding = PaddingValues(start = 12.dp, end = 12.dp, top = 12.dp, bottom = 10.dp)) {
        Row(Modifier.fillMaxWidth().padding(bottom = 8.dp), verticalAlignment = Alignment.Bottom) {
            Text("Waterfall", fontSize = 14.sp, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f))
            Small("modem73 audio · 30 s")
        }
        Waterfall(radio.tick, height, tx)
        Row(Modifier.fillMaxWidth().padding(top = 3.dp), horizontalArrangement = Arrangement.SpaceBetween) {
            listOf("0.3k", "1.2k", "2.1k", "3.0 kHz").forEach { MonoText(it, color = colors().onSurfaceVariant, size = 10.5.sp) }
        }
        if (!compact) {
            Row(Modifier.fillMaxWidth().padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterHorizontally)) {
                listOf(WaterfallColors.quiet to "quiet", WaterfallColors.noise to "noise", WaterfallColors.signal to "signal", WaterfallColors.yourTx to "your TX").forEach { (col, t) ->
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                        Box(Modifier.size(8.dp).clip(RoundedCornerShape(2.dp)).background(col)); Text(t, fontSize = 11.sp, color = colors().onSurfaceVariant)
                    }
                }
            }
        }
    }
}

@Composable
private fun Waterfall(tick: Int, height: Dp, tx: Boolean) {
    val image = remember(tx) { waterfallImage(tx).asImageBitmap() }
    Canvas(Modifier.fillMaxWidth().height(height).clip(RoundedCornerShape(12.dp)).background(WaterfallColors.quiet)) {
        // Scroll: newest at the top, so the picture moves down one row per tick.
        val rows = image.height
        val shift = tick % rows
        val w = size.width.toInt()
        val h = size.height.toInt()
        val rowPx = h.toFloat() / rows
        val split = (shift * rowPx).toInt()
        if (shift > 0) drawImage(image, IntOffset(0, rows - shift), IntSize(image.width, shift), IntOffset(0, 0), IntSize(w, split), filterQuality = FilterQuality.None)
        drawImage(image, IntOffset(0, 0), IntSize(image.width, rows - shift), IntOffset(0, split), IntSize(w, h - split), filterQuality = FilterQuality.None)
    }
}

/** A made-up 30 s of traffic: bursts of signal, one of them ours (orange) when [tx]. Port of the mockup's wf.py. */
private fun waterfallImage(tx: Boolean): Bitmap {
    val w = 182
    val h = 92
    var seed = 7
    fun rnd(): Float {
        seed += 0x6D2B79F5
        var t = seed
        t = (t xor (t ushr 15)) * (1 or t)
        t = (t + ((t xor (t ushr 7)) * (61 or t))) xor t
        return ((t xor (t ushr 14)).toLong() and 0xFFFFFFFFL).toFloat() / 4294967296f
    }
    data class B(val y0: Int, val y1: Int, val x0: Float, val x1: Float, val lvl: Float, val tx: Boolean)
    val bursts = listOf(
        B(2, 7, .15f, .85f, 1f, false), B(13, 22, .14f, .86f, .98f, tx), B(29, 38, .16f, .84f, 1f, false),
        B(42, 45, .18f, .82f, .8f, false), B(52, 62, .14f, .82f, 1f, false), B(70, 74, .2f, .8f, .7f, false), B(83, 92, .16f, .84f, 1f, false),
    ).filter { it.tx || it.y0 != 13 || tx }
    val stops = listOf(0f to intArrayOf(11, 18, 48), .3f to intArrayOf(22, 30, 96), .52f to intArrayOf(140, 60, 200), .68f to intArrayOf(236, 170, 90), .82f to intArrayOf(255, 214, 70), 1f to intArrayOf(255, 235, 110))
    fun ramp(v: Float): Int {
        for (i in 1 until stops.size) {
            if (v <= stops[i].first) {
                val (a0, a) = stops[i - 1]; val (b0, b) = stops[i]
                val t = (v - a0) / (b0 - a0)
                val rgb = IntArray(3) { k -> (a[k] + (b[k] - a[k]) * t).toInt() }
                return (0xFF shl 24) or (rgb[0] shl 16) or (rgb[1] shl 8) or rgb[2]
            }
        }
        return 0xFFFFEB6E.toInt()
    }
    val px = IntArray(w * h)
    for (y in 0 until h) {
        val here = bursts.filter { y >= it.y0 && y < it.y1 }
        for (x in 0 until w) {
            var v = .10f + .07f * rnd() + .04f * kotlin.math.sin(x / 10.5f) + .05f * kotlin.math.sin(x / 4.5f + y / 2.5f)
            var color: Int? = null
            for (b in here) {
                if (x >= b.x0 * w && x < b.x1 * w) {
                    if (b.tx) {
                        val s = .7f + .3f * rnd()
                        color = (0xFF shl 24) or ((255 * s).toInt() shl 16) or ((122 * s + 20).toInt() shl 8) or (61 * s).toInt()
                    } else {
                        val u = (x - b.x0 * w) / ((b.x1 - b.x0) * w)
                        val edge = minOf(u, 1 - u) / .06f
                        val sub = .7f + .2f * kotlin.math.sin(u * 38 + b.y0) + .12f * kotlin.math.sin(u * 97 + b.y0 * 3)
                        val row = .74f + .16f * kotlin.math.sin((y - b.y0) * 3.4f) + .16f * (rnd() - .5f)
                        v = maxOf(v, b.lvl * maxOf(.25f, sub) * row * edge.coerceIn(0f, 1f) * .92f + .06f * rnd())
                    }
                }
            }
            px[y * w + x] = color ?: ramp(v.coerceIn(0f, 1f))
        }
    }
    return Bitmap.createBitmap(px, w, h, Bitmap.Config.ARGB_8888)
}

/** Outline-bordered text button that keeps the mockup's 40 dp pill. */
@Composable
fun Outlined(text: String, modifier: Modifier = Modifier, icon: ImageVector? = null, color: Color = colors().primary, borderColor: Color = colors().outline, onClick: () -> Unit) {
    OutlinedButton(onClick, modifier, border = BorderStroke(1.dp, borderColor), colors = ButtonDefaults.outlinedButtonColors(contentColor = color), contentPadding = PaddingValues(horizontal = 14.dp)) {
        if (icon != null) { Icon(icon, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)) }
        Text(text, maxLines = 1)
    }
}
