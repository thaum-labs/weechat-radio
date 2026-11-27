package com.weechatradio.app.radio

import org.json.JSONArray
import org.json.JSONObject
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

// What the real station reports for Mail, On air and the Map, read from its
// own loopback HTTP (the same calls the desktop app's Email tab and traffic
// list make). Plain data; LiveRadio fetches it, the screens draw it.

/** A file in a mail: where the station saved it (received mail) or just its name (sent). */
data class MailFile(val name: String, val size: Int, val path: String)

/** One mail in the station's store. */
data class LiveMail(
    val id: String,
    /** "inbox", "outbox" or "sent". */
    val folder: String,
    val from: String,
    val to: String,
    val subject: String,
    val body: String,
    /** Unix seconds. */
    val ts: Long,
    val unread: Boolean,
    /** The station's own words for where it is: Queued, On air, At gateway, Sent, Delivered… */
    val label: String,
    /** queued, tx, keyed, gateway, sent, delivered, delayed, bounced, spam, failed, received… */
    val key: String,
    /** "[..]", "[tx]", "[gw]", "[ok]", "[no]". */
    val mark: String,
    /** The whole story when something went wrong or is still open. */
    val reason: String,
    /** Failed: the station offers Retry. */
    val retry: Boolean,
    val files: List<MailFile>,
) {
    val inbox: Boolean get() = folder == "inbox"
    /** Who it is from (inbox) or to (sent). */
    val peer: String get() = if (inbox) from else to
}

/** A mail whose header came over the air; Get pulls the body through the gateway. */
data class WaitingMail(val id: String, val from: String, val subject: String, val bytes: Int)

/** The Mail tab's state line: address, why mail cannot go, the gateway, a Get in progress. */
data class MailTab(
    val address: String = "",
    val unread: Int = 0,
    /** Why mail cannot go now; empty when it can. */
    val block: String = "",
    /** The Radio+ gateway in use, in words. */
    val gatewayLine: String = "",
    /** "", "asking", "receiving", "fetched" or "failed". */
    val getState: String = "",
    val getGot: Int = 0,
    val getTotal: Int = 0,
    val getDetail: String = "",
    /** What can be attached, e.g. "Files up to 512 KB in all". */
    val attachLine: String = "",
)

/** One frame heard or sent, for On air. */
data class TrafficRow(
    val seq: Long,
    val ts: Long,
    val sent: Boolean,
    /** "rf", "inet" or "lan". */
    val via: String,
    /** dm, bulletin, group, receipt, beacon, mail, … */
    val kind: String,
    val from: String,
    val to: String,
    val text: String,
    val snr: Double?,
    val bytes: Int,
)

/** A station the radio (or the hub) has heard. */
data class HeardRow(
    val call: String,
    val band: String,
    val medium: String,
    /** Unix seconds. */
    val lastHeard: Long,
    val snr: Double?,
    val grid: String,
    val mode: String,
)

object LiveJson {
    fun mail(o: JSONObject) = LiveMail(
        id = o.optString("id"),
        folder = o.optString("folder"),
        from = o.optString("from_addr"),
        to = o.optString("to_addr"),
        subject = o.optString("subject"),
        body = o.optString("body"),
        ts = o.optLong("ts"),
        unread = o.optBoolean("unread"),
        label = o.optJSONObject("status")?.optString("label").orEmpty(),
        key = o.optJSONObject("status")?.optString("key").orEmpty(),
        mark = o.optJSONObject("status")?.optString("mark").orEmpty(),
        reason = o.optJSONObject("status")?.optString("reason").orEmpty(),
        retry = o.optJSONObject("status")?.optBoolean("retry") ?: false,
        files = o.optJSONArray("files").objects().map { MailFile(it.optString("name"), it.optInt("size"), it.optString("path")) },
    )

    fun waiting(o: JSONObject) = WaitingMail(o.optString("id"), o.optString("from_addr"), o.optString("subject"), o.optInt("bytes"))

    fun tab(o: JSONObject): MailTab {
        val get = o.optJSONObject("get")
        return MailTab(
            address = o.optString("address"),
            unread = o.optInt("unread"),
            block = if (o.isNull("block")) "" else o.optString("block"),
            gatewayLine = o.optString("gateway_line"),
            getState = get?.optString("state").orEmpty(),
            getGot = get?.optInt("got") ?: 0,
            getTotal = get?.optInt("total") ?: 0,
            getDetail = get?.optString("detail").orEmpty(),
            attachLine = o.optJSONObject("attach")?.optString("line").orEmpty(),
        )
    }

    fun traffic(o: JSONObject) = TrafficRow(
        seq = o.optLong("seq"),
        ts = o.optLong("ts"),
        sent = o.optString("dir") == "sent",
        via = o.optString("via"),
        kind = o.optString("kind"),
        from = o.optString("from"),
        to = o.optString("to"),
        text = o.optString("text"),
        snr = o.optDouble("snr").takeIf { !it.isNaN() && !o.isNull("snr") },
        bytes = o.optInt("bytes"),
    )

    fun heard(o: JSONObject) = HeardRow(
        call = o.optString("callsign"),
        band = o.optString("band"),
        medium = o.optString("medium"),
        lastHeard = o.optLong("last_heard"),
        snr = o.optDouble("snr").takeIf { !it.isNaN() && !o.isNull("snr") },
        grid = o.optString("grid"),
        mode = o.optString("mode"),
    )
}

fun JSONArray?.objects(): List<JSONObject> =
    if (this == null) emptyList() else (0 until length()).mapNotNull { optJSONObject(it) }

private val hhmm = DateTimeFormatter.ofPattern("HH:mm", Locale.UK)
private val dayHhmm = DateTimeFormatter.ofPattern("EEE HH:mm", Locale.UK)
private val dateHhmm = DateTimeFormatter.ofPattern("d MMM HH:mm", Locale.UK)

/** "22:40" today, "Fri 22:40" this week, "12 Nov 22:40" before. */
fun whenText(unix: Long): String {
    if (unix <= 0) return ""
    val t = Instant.ofEpochSecond(unix).atZone(ZoneId.systemDefault())
    val days = java.time.temporal.ChronoUnit.DAYS.between(t.toLocalDate(), LocalDate.now())
    return when {
        days == 0L -> t.format(hhmm)
        days in 1..6 -> t.format(dayHhmm)
        else -> t.format(dateHhmm)
    }
}

/** "just now", "4 min ago", "2 h ago", "3 d ago". */
fun agoText(unix: Long): String {
    if (unix <= 0) return ""
    val s = System.currentTimeMillis() / 1000 - unix
    return when {
        s < 60 -> "just now"
        s < 3600 -> "${s / 60} min ago"
        s < 86400 -> "${s / 3600} h ago"
        else -> "${s / 86400} d ago"
    }
}

/** "12 KB", "800 B". */
fun sizeText(bytes: Int): String = if (bytes >= 1024) "${(bytes + 512) / 1024} KB" else "$bytes B"

/** The centre of a Maidenhead grid square ("IO81UF"), or null if it is not one. */
fun gridToLatLon(grid: String): Pair<Double, Double>? {
    val g = grid.trim().uppercase()
    if (g.length < 4 || g.length % 2 != 0) return null
    if (g[0] !in 'A'..'R' || g[1] !in 'A'..'R' || !g[2].isDigit() || !g[3].isDigit()) return null
    var lon = (g[0] - 'A') * 20.0 - 180 + (g[2] - '0') * 2.0
    var lat = (g[1] - 'A') * 10.0 - 90 + (g[3] - '0') * 1.0
    var w = 2.0
    var h = 1.0
    if (g.length >= 6) {
        if (g[4] !in 'A'..'X' || g[5] !in 'A'..'X') return null
        w = 2.0 / 24
        h = 1.0 / 24
        lon += (g[4] - 'A') * w
        lat += (g[5] - 'A') * h
    }
    return (lat + h / 2) to (lon + w / 2)
}
