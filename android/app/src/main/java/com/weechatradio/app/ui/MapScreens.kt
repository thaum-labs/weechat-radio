package com.weechatradio.app.ui

import android.graphics.BitmapFactory
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.weechatradio.app.core.Core
import com.weechatradio.app.radio.Radio
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL
import kotlin.math.PI
import kotlin.math.atan
import kotlin.math.cos
import kotlin.math.floor
import kotlin.math.ln
import kotlin.math.pow
import kotlin.math.sinh
import kotlin.math.tan

// Design screens 10–12: map, VHF/UHF plan, HF sky path.
// The plan figures (verdict, profile, hops) are sample data until the planner
// maths from web/guides/band.js is ported into the Rust core (next step).

data class LatLon(val lat: Double, val lon: Double)

private val Frome = LatLon(51.2308, -2.3207)
private val Bath = LatLon(51.3811, -2.3590)

/** Stations on the map (sample: XX0… are made-up). */
private val MapStations = listOf(
    "XX0BBB" to Bath,
    "XX0AAA" to LatLon(51.2290, -2.3050),
    "XX0CCC" to LatLon(51.2050, -2.1800),
    "XX0DDD" to LatLon(51.3190, -2.2080),
)

private const val TILE = 256f

private fun worldX(lon: Double, z: Int) = (lon + 180.0) / 360.0 * 2.0.pow(z) * TILE
private fun worldY(lat: Double, z: Int): Double {
    val r = lat * PI / 180.0
    return (1.0 - ln(tan(r) + 1.0 / cos(r)) / PI) / 2.0 * 2.0.pow(z) * TILE
}
private fun lonOf(x: Double, z: Int) = x / (2.0.pow(z) * TILE) * 360.0 - 180.0
private fun latOf(y: Double, z: Int) = atan(sinh(PI * (1.0 - 2.0 * y / (2.0.pow(z) * TILE)))) * 180.0 / PI

/** Map tiles kept for the app's life (CARTO basemaps, OpenStreetMap data). */
private object Tiles {
    val cache = mutableStateMapOf<String, ImageBitmap>()
    fun url(dark: Boolean, z: Int, x: Int, y: Int) =
        "https://basemaps.cartocdn.com/${if (dark) "dark_all" else "light_all"}/$z/$x/$y@2x.png"
}

/**
 * A small slippy map: OpenStreetMap tiles from CARTO, dragged with a finger,
 * with [overlay] drawn on top using [project] to turn a place into a point.
 * With [tiles] false (tests, no network) it draws a plain grid instead.
 */
@Composable
fun TileMap(
    center: LatLon,
    zoom: Int,
    onCenter: (LatLon) -> Unit,
    modifier: Modifier = Modifier,
    tiles: Boolean = true,
    overlay: androidx.compose.ui.graphics.drawscope.DrawScope.(project: (LatLon) -> Offset) -> Unit = {},
) {
    val dark = colors().surface.red < 0.5f
    val density = LocalDensity.current.density
    BoxWithConstraints(modifier.clip(RoundedCornerShape(0.dp)).background(if (dark) Color(0xFF131B27) else Color(0xFFE9E7E2))) {
        val wPx = constraints.maxWidth.toDouble()
        val hPx = constraints.maxHeight.toDouble()
        val scale = density.toDouble() // one tile = 256 dp
        val cx = worldX(center.lon, zoom)
        val cy = worldY(center.lat, zoom)
        val left = cx - wPx / 2 / scale
        val top = cy - hPx / 2 / scale
        val x0 = floor(left / TILE).toInt()
        val y0 = floor(top / TILE).toInt()
        val x1 = floor((left + wPx / scale) / TILE).toInt()
        val y1 = floor((top + hPx / scale) / TILE).toInt()
        val needed = buildList { for (x in x0..x1) for (y in y0..y1) add(Triple(x, y, Tiles.url(dark, zoom, x, y))) }
        if (tiles) {
            LaunchedEffect(needed.map { it.third }) {
                for ((_, _, url) in needed) {
                    if (url in Tiles.cache) continue
                    val img = withContext(Dispatchers.IO) {
                        runCatching {
                            val conn = URL(url).openConnection() as HttpURLConnection
                            conn.setRequestProperty("User-Agent", "weechat-radio-android")
                            conn.connectTimeout = 8_000
                            conn.readTimeout = 8_000
                            conn.inputStream.use { BitmapFactory.decodeStream(it) }?.asImageBitmap()
                        }.getOrNull()
                    }
                    if (img != null) {
                        if (Tiles.cache.size > 300) Tiles.cache.clear()
                        Tiles.cache[url] = img
                    }
                }
            }
        }
        Canvas(
            Modifier.fillMaxSize().pointerInput(zoom) {
                var at = Offset(cx.toFloat(), cy.toFloat())
                detectDragGestures(onDragStart = { at = Offset(worldX(center.lon, zoom).toFloat(), worldY(center.lat, zoom).toFloat()) }) { _, d ->
                    at -= d / scale.toFloat()
                    onCenter(LatLon(latOf(at.y.toDouble(), zoom), lonOf(at.x.toDouble(), zoom)))
                }
            },
        ) {
            val sz = (TILE * scale).toInt()
            for ((x, y, url) in needed) {
                val dx = ((x * TILE - left) * scale).toInt()
                val dy = ((y * TILE - top) * scale).toInt()
                val img = if (tiles) Tiles.cache[url] else null
                if (img != null) {
                    drawImage(img, IntOffset.Zero, IntSize(img.width, img.height), IntOffset(dx, dy), IntSize(sz, sz))
                } else {
                    drawRect(Color.Gray.copy(alpha = 0.12f), Offset(dx.toFloat(), dy.toFloat()), Size(sz.toFloat(), sz.toFloat()), style = Stroke(1f))
                }
            }
            overlay { p -> Offset(((worldX(p.lon, zoom) - left) * scale).toFloat(), ((worldY(p.lat, zoom) - top) * scale).toFloat()) }
        }
        Text(
            "© OpenStreetMap © CARTO", Modifier.align(Alignment.BottomStart).padding(4.dp),
            fontSize = 9.sp, color = colors().onSurfaceVariant.copy(alpha = 0.8f),
        )
    }
}

/** Path colour of the plan line: clear, marginal, blocked (as the website's planner). */
private val PathClear = Color(0xFF7D9BFF)
private val PathMarginal = Color(0xFFFFD60A)
private val PathBlocked = Color(0xFFFF3B6B)

/** Design 10: the map with the plan to the picked station in a panel below. */
@Composable
fun MapScreen(radio: Radio, tiles: Boolean, onPlan: () -> Unit, onSettings: () -> Unit) {
    var center by remember { mutableStateOf(LatLon(51.300, -2.300)) }
    var zoom by remember { mutableIntStateOf(11) }
    var band by remember { mutableStateOf(Bands.first()) }
    val tx = radioColors().tx
    val other = radioColors().otherCall
    Box(Modifier.fillMaxSize()) {
        TileMap(center, zoom, { center = it }, Modifier.fillMaxWidth().fillMaxHeight(0.62f), tiles) { project ->
            val a = project(Frome)
            val b = project(Bath)
            drawLine(PathMarginal, a, b, strokeWidth = 4.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(18f, 10f)))
            MapStations.forEach { (_, p) -> val o = project(p); drawCircle(Color.Black.copy(alpha = 0.5f), 8.dp.toPx(), o); drawCircle(other, 6.dp.toPx(), o) }
            drawCircle(tx.copy(alpha = 0.3f), 13.dp.toPx(), a)
            drawCircle(tx, 8.dp.toPx(), a)
        }
        // Search bar
        Row(
            Modifier.padding(start = 12.dp, end = 12.dp, top = 8.dp).fillMaxWidth().height(56.dp).shadow(4.dp, RoundedCornerShape(28.dp))
                .clip(RoundedCornerShape(28.dp)).background(colors().surfaceContainerHigh).padding(start = 16.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(Icons.Filled.Search, null, tint = colors().onSurfaceVariant)
            Text("Callsign, city or grid", Modifier.weight(1f), fontSize = 15.sp, color = colors().onSurfaceVariant)
            Chip("on air")
            IconBtn(Icons.Filled.Settings, "Settings", onClick = onSettings)
        }
        Row(Modifier.padding(start = 12.dp, top = 74.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            listOf(PathClear to "clear", PathMarginal to "marginal", PathBlocked to "blocked").forEach { (col, t) ->
                Chip(t, small = true, container = colors().surfaceContainerHigh, lead = { Box(Modifier.size(12.dp, 4.dp).clip(RoundedCornerShape(2.dp)).background(col)) })
            }
        }
        Column(Modifier.align(Alignment.TopEnd).padding(end = 12.dp, top = 78.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SmallFloatingActionButton({ zoom = (zoom + 1).coerceAtMost(16) }, containerColor = colors().surfaceContainerHigh, contentColor = colors().primary) { Icon(Icons.Filled.Add, "Zoom in") }
            SmallFloatingActionButton({ zoom = (zoom - 1).coerceAtLeast(5) }, containerColor = colors().surfaceContainerHigh, contentColor = colors().primary) { Icon(Icons.Filled.Remove, "Zoom out") }
            SmallFloatingActionButton({ center = Frome }, Modifier.padding(top = 6.dp)) { Icon(Icons.Filled.MyLocation, "My location") }
        }
        // Plan panel
        Column(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth().fillMaxHeight(0.44f).shadow(8.dp, RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp))
                .clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp)).background(colors().surfaceContainerLow)
                .verticalScroll(rememberScrollState()).padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Box(Modifier.padding(top = 10.dp).align(Alignment.CenterHorizontally).size(32.dp, 4.dp).clip(CircleShape).background(colors().onSurfaceVariant.copy(alpha = 0.4f)))
            PlanVerdict(band)
            Row(verticalAlignment = Alignment.Bottom) {
                Box(Modifier.weight(1f)) { BigFreq(band.freq, "${band.label} · ${band.emission} · ${band.preset}", 34.sp) }
                Text("17 km\n5 W each end", fontSize = 13.sp, lineHeight = 19.sp, color = colors().onSurfaceVariant, textAlign = androidx.compose.ui.text.style.TextAlign.End)
            }
            BandChips(Bands, band.label) { band = it }
            BandLegend()
            Row(Modifier.padding(bottom = 12.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Outlined("Plan details", Modifier.weight(1f), icon = Icons.Filled.KeyboardArrowUp, onClick = onPlan)
                TuneButton(Modifier.weight(1.2f), "Tune radio") { radio.tune(band.freq, band.emission, band.preset) }
            }
        }
        radio.note?.let { Note(it, Modifier.align(Alignment.BottomCenter).padding(bottom = 16.dp)) }
    }
}

/** The verdict card for a band; PMR446 also shows the core's licence-free warning. */
@Composable
private fun PlanVerdict(band: BandChip) {
    val warning = Core.frequencyWarning(band.freq)
        ?: if (band.label == "PMR446") "PMR446 is licence-free: only handhelds with a fixed antenna are allowed there. Check your rules before you transmit." else null
    when {
        band.label == "PMR446" -> WarnCard("PMR446.", warning ?: "")
        band.verdict == Verdict.Works -> WarnCard("Clear.", "The path is open on ${band.label} with 5 W each end.", ok = true, icon = Icons.Filled.CheckCircle)
        band.verdict == Verdict.No -> WarnCard("Unlikely.", "${band.label} does not reach 17 km this hour.")
        else -> WarnCard("Marginal.", "A ridge 6 km from Frome clips the line. Raise the antennas or add power for a solid link.")
    }
}

@Composable
fun TuneButton(modifier: Modifier, label: String, onClick: () -> Unit) {
    Button(onClick, modifier, colors = ButtonDefaults.buttonColors(containerColor = radioColors().tx, contentColor = TxInk), contentPadding = PaddingValues(horizontal = 14.dp)) {
        Icon(Icons.Filled.Tune, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text(label, maxLines = 1)
    }
}

@Composable
private fun PlanTop(sub: String, onBack: () -> Unit) {
    Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        BackLead(onBack)
        Column(Modifier.weight(1f).padding(start = 4.dp)) {
            Text("Frome → Bath", fontSize = 20.sp, lineHeight = 26.sp)
            Row { MonoText("IO81UF → IO81TJ", color = colors().onSurfaceVariant, size = 11.sp); Text(" · $sub", fontSize = 12.sp, color = colors().onSurfaceVariant, maxLines = 1) }
        }
        IconBtn(Icons.Filled.SwapHoriz, "Swap ends") {}
    }
}

@Composable
private fun StationCard(label: String, labelColor: Color, antenna: String, height: String, power: String, modifier: Modifier, extra: (@Composable () -> Unit)? = null) {
    Column(Modifier.then(modifier).clip(RoundedCornerShape(12.dp)).background(Color.Transparent).border1().padding(12.dp)) {
        MonoText(label, color = labelColor, size = 12.sp, weight = FontWeight.Medium)
        Text(antenna, Modifier.padding(top = 3.dp), fontSize = 14.sp, lineHeight = 19.sp)
        Small("$height · $power", Modifier.padding(top = 2.dp))
        extra?.invoke()
    }
}

@Composable
private fun Modifier.border1(): Modifier = this.then(Modifier.border(1.dp, colors().outlineVariant, RoundedCornerShape(12.dp)))

@Composable
private fun KeyValues(rows: List<Pair<String, String>>, preset: String) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        rows.forEach { (k, v) ->
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(k, Modifier.width(48.dp), fontSize = 13.sp, color = colors().onSurfaceVariant)
                Text(v, fontSize = 13.sp)
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("preset", Modifier.width(48.dp), fontSize = 13.sp, color = colors().onSurfaceVariant)
            MonoText(preset, color = radioColors().otherCall, size = 12.5.sp)
        }
    }
}

/** The command and Tune radio button at the foot of both plan screens. */
@Composable
private fun PlanFoot(cmd: String, label: String, onTune: () -> Unit) {
    val clip = LocalClipboardManager.current
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(topStart = 16.dp, topEnd = 16.dp)).background(colors().surfaceContainer).padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 14.dp)) {
        Row(Modifier.padding(bottom = 10.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(Modifier.weight(1f).height(44.dp).clip(RoundedCornerShape(12.dp)).background(colors().surfaceContainerHighest).padding(horizontal = 14.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                MonoText("$", color = colors().onSurfaceVariant); MonoText(cmd)
            }
            IconBtn(Icons.Filled.ContentCopy, "Copy", tonal = true) { clip.setText(AnnotatedString(cmd)) }
        }
        TuneButton(Modifier.fillMaxWidth().height(48.dp), label, onTune)
    }
}

/** Design 11: VHF/UHF line-of-sight plan between two stations. */
@Composable
fun PlanVhfScreen(radio: Radio, onBack: () -> Unit, onSky: () -> Unit) {
    var band by remember { mutableStateOf(Bands.first()) }
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            PlanTop("17 km · XX0BBB", onBack)
            Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    StationCard("HOME", radioColors().tx, "Handheld rubber duck", "1.5 m", "5 W", Modifier.weight(1f))
                    StationCard("RX · XX0BBB", colors().primary, "Mobile whip", "1.8 m car roof", "5 W", Modifier.weight(1f)) {
                        Chip("Mobile antenna", Modifier.padding(top = 6.dp), small = true, container = colors().primaryContainer, content = colors().onPrimaryContainer)
                    }
                }
                Card(color = colors().surfaceContainer, padding = PaddingValues(top = 8.dp)) { TerrainProfile() }
                BandChips(Bands, band.label) { if (it.freq.toDouble() < 30) onSky() else { band = it } }
                BandLegend()
                Row {
                    Box(Modifier.weight(1f)) { BigFreq(band.freq, "${band.label} · ${band.emission}") }
                    KeyValues(listOf("power" to "5 W each end", "sure" to "4 km each end"), band.preset)
                }
                PlanVerdict(band)
                Text(
                    androidx.compose.ui.text.buildAnnotatedString {
                        append("Narrow FM on ${band.freq}, low power indoors. Handhelds: rest between TX blocks. ")
                        pushStyle(androidx.compose.ui.text.SpanStyle(color = radioColors().warn)); append("Check your band plan before you transmit."); pop()
                    },
                    fontSize = 13.sp, lineHeight = 19.sp, color = colors().onSurfaceVariant,
                )
            }
            PlanFoot("/radio qsy ${band.freq}", "Tune radio: ${band.freq} ${band.emission} + ${band.preset}") { radio.tune(band.freq, band.emission, band.preset) }
        }
        radio.note?.let { Note(it, Modifier.align(Alignment.BottomCenter).padding(bottom = 130.dp)) }
    }
}

/** Sample terrain between Frome and Bath with the line of sight clipped by a ridge. */
@Composable
private fun TerrainProfile() {
    val ground = colors().surfaceVariant
    val line = PathMarginal
    val tx = radioColors().tx
    val other = radioColors().otherCall
    val heights = floatArrayOf(95f, 100f, 112f, 130f, 150f, 175f, 196f, 205f, 198f, 180f, 160f, 140f, 120f, 105f, 92f, 80f, 70f, 62f, 58f, 55f, 52f, 50f, 48f, 47f)
    Canvas(Modifier.fillMaxWidth().height(112.dp)) {
        val w = size.width
        val h = size.height
        val max = 230f
        fun y(m: Float) = h - m / max * h
        val step = w / (heights.size - 1)
        val path = Path().apply {
            moveTo(0f, h)
            heights.forEachIndexed { i, m -> lineTo(i * step, y(m)) }
            lineTo(w, h); close()
        }
        drawPath(path, ground)
        val a = Offset(0f + 6f, y(heights.first() + 60f))
        val b = Offset(w - 6f, y(heights.last() + 70f))
        drawLine(line, a, b, strokeWidth = 2.dp.toPx())
        drawLine(line.copy(alpha = 0.5f), a, b, strokeWidth = 18.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 6f)), alpha = 0.25f)
        drawCircle(tx, 5.dp.toPx(), a)
        drawCircle(other, 5.dp.toPx(), b)
    }
}

/** Design 12: HF sky path (NVIS) with the next 24 hours. */
@Composable
fun PlanHfScreen(radio: Radio, onBack: () -> Unit) {
    val hf = Bands.filter { it.freq.toDouble() < 30 }
    var band by remember { mutableStateOf(hf.first()) }
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            PlanTop("HF sky path · 23:00 UTC", onBack)
            Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Card(color = colors().surfaceContainer, padding = PaddingValues(0.dp)) { SkyPath() }
                BandChips(hf, band.label) { band = it }
                BandLegend()
                Row {
                    Box(Modifier.weight(1f)) { BigFreq(band.freq, "${band.label} · ${band.emission}") }
                    KeyValues(listOf("SSB" to "on", "power" to "1 W each end", "path" to "17 km"), band.preset)
                }
                if (band.verdict == Verdict.No) WarnCard("Unlikely this hour.", "${band.label} skips over 17 km at night.")
                else WarnCard("Likely this hour.", "${band.label} is open on this path and 1 W is enough. It stays open all day.")
                Text("1 hop · ~20 km each · take-off 89° · MUF 3.5 MHz", fontSize = 13.sp, color = colors().primary)
                Column {
                    Label("NEXT 24 H · ${band.label.uppercase()}", Modifier.padding(bottom = 6.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                        repeat(24) { k ->
                            val col = if (band.verdict == Verdict.No) colors().outline else if (k == 19) radioColors().ok else radioColors().warn
                            Box(
                                Modifier.weight(1f).height(18.dp)
                                    .then(if (k == 0) Modifier.border(2.dp, colors().onSurface, RoundedCornerShape(3.dp)) else Modifier)
                                    .padding(if (k == 0) 2.dp else 0.dp).clip(RoundedCornerShape(3.dp)).background(col),
                            )
                        }
                    }
                }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(Icons.Filled.Warning, null, Modifier.size(18.dp), tint = radioColors().warn)
                    Text("${band.label} needs an amateur licence.", fontSize = 13.sp, color = radioColors().warn)
                }
            }
            PlanFoot("/radio qsy ${band.freq} · preset ${band.preset}", "Tune radio: ${band.freq} ${band.emission} + ${band.preset}") { radio.tune(band.freq, band.emission, band.preset) }
        }
        radio.note?.let { Note(it, Modifier.align(Alignment.BottomCenter).padding(bottom = 130.dp)) }
    }
}

/** Sample sky picture: ground, the ionosphere and one near-vertical hop. */
@Composable
private fun SkyPath() {
    val ok = radioColors().ok
    val layer = colors().primary
    val ground = colors().surfaceVariant
    val tx = radioColors().tx
    val other = radioColors().otherCall
    Canvas(Modifier.fillMaxWidth().height(130.dp)) {
        val w = size.width
        val h = size.height
        drawRect(layer.copy(alpha = 0.12f), Offset(0f, h * 0.10f), Size(w, h * 0.22f))
        drawRect(ground, Offset(0f, h - 14.dp.toPx()), Size(w, 14.dp.toPx()))
        val a = Offset(w * 0.22f, h - 14.dp.toPx())
        val b = Offset(w * 0.78f, h - 14.dp.toPx())
        val top = Offset(w * 0.5f, h * 0.2f)
        val p = Path().apply { moveTo(a.x, a.y); quadraticTo(w * 0.36f, h * 0.1f, top.x, top.y); quadraticTo(w * 0.64f, h * 0.1f, b.x, b.y) }
        drawPath(p, ok, style = Stroke(2.5.dp.toPx()))
        drawCircle(tx, 5.dp.toPx(), a)
        drawCircle(other, 5.dp.toPx(), b)
    }
}
