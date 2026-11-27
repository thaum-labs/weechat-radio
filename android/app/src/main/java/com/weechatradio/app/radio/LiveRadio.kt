package com.weechatradio.app.radio

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONArray
import org.json.JSONObject
import uniffi.wcr_mobile.StationSetup
import uniffi.wcr_mobile.chatSendFile
import uniffi.wcr_mobile.stationCall
import uniffi.wcr_mobile.chatSend
import uniffi.wcr_mobile.chatSnapshot
import uniffi.wcr_mobile.radioSetting
import uniffi.wcr_mobile.startStation
import uniffi.wcr_mobile.stationStatus
import kotlin.concurrent.thread

/**
 * The real station (Phase 0): the wcr node and modem73 run on the phone and
 * reach the radio over [RadioLink]. Only real things show: radio status,
 * chat, mail, the frames on the air and the stations heard, all from the
 * station itself. Nothing here is sample data.
 */
class LiveRadio(
    private val context: Context,
    start: Status,
    private val sample: Radio = SimulatedRadio(start, live = false),
) : Radio by sample {
    override var status by mutableStateOf(start.copy(link = Link.Bluetooth, btAudio = BtAudio.Modem73, real = true, radioUp = false, radioNote = "starting…"))
        private set
    override var channel by mutableStateOf("#bulletin")
        private set
    override var channels by mutableStateOf(listOf(Channel("#bulletin", ChannelKind.Room, "", "")))
        private set
    override var heard by mutableStateOf(emptyList<Heard>())
        private set
    override var onAir by mutableStateOf<OnAir?>(null)
        private set
    override var note by mutableStateOf<String?>(null)
        private set
    override var tick by mutableIntStateOf(0)
        private set
    override val mails: List<Mail> = emptyList()
    override val frames: List<Frame> = emptyList()
    /** Us first, then every station heard that sent its grid. */
    override val stations: List<MapStation>
        get() {
            val me = gridToLatLon(status.grid) ?: (51.5 to -0.1)
            val now = System.currentTimeMillis() / 1000
            return listOf(MapStation(status.call, me.first, me.second, status.mode, status.grid, "", null, false)) +
                heardAll.mapNotNull { h ->
                    val (lat, lon) = gridToLatLon(h.grid) ?: return@mapNotNull null
                    MapStation(
                        h.call, lat, lon, Mode.entries.firstOrNull { it.id == h.mode } ?: Mode.Radio, h.grid,
                        whenText(h.lastHeard), h.snr?.toInt(),
                        live = h.medium != "inet" && h.medium != "lan" && now - h.lastHeard < 1800,
                    )
                }
        }

    /** Mail in the station's store, newest first. */
    var inbox by mutableStateOf(emptyList<LiveMail>())
        private set
    /** Mail we sent or are sending (outbox and sent), newest first. */
    var sentMail by mutableStateOf(emptyList<LiveMail>())
        private set
    /** Headers waiting at the gateway (Radio+). */
    var waitingMail by mutableStateOf(emptyList<WaitingMail>())
        private set
    var mailTab by mutableStateOf(MailTab())
        private set
    /** Frames heard and sent, newest first. */
    var traffic by mutableStateOf(emptyList<TrafficRow>())
        private set
    /** Every station heard, newest first. */
    var heardAll by mutableStateOf(emptyList<HeardRow>())
        private set

    /** The big line on the Radio tab and why, from hard evidence only. */
    var headline by mutableStateOf("NOT LINKED" to "starting the station…")
        private set

    /** Everything needed to see why the link does or does not work, as text to paste. */
    fun debugLog(): String = buildString {
        appendLine("weechat-radio Android ${com.weechatradio.app.BuildConfig.VERSION_NAME} debug log, ${java.time.LocalDateTime.now().withNano(0)}")
        appendLine("Headline: ${headline.first} (${headline.second})")
        appendLine("\n== Radio link (app Bluetooth)")
        link.log().forEach { appendLine(it) }
        appendLine("\n== Facts")
        facts.forEach { (k, v) -> appendLine("$k: $v") }
        appendLine("\n== Station log (last 150 lines)")
        appendLine(runCatching { uniffi.wcr_mobile.stationLog(150u) }.getOrElse { "cannot read: $it" })
        appendLine("\n== Station status")
        appendLine(lastStatusJson)
    }

    /** True while audio from the radio is arriving now (the last 5 s). */
    var hearing by mutableStateOf(false)
        private set

    /** The station heard most recently by radio, and how long ago: "XX0BBB · 2 min ago". */
    var lastHeard by mutableStateOf("nobody yet")
        private set

    /** Everything the station reports, for Copy debug log: plain facts, nothing made up. */
    var facts by mutableStateOf(listOf("Station" to "starting…"))
        private set

    private var chats by mutableStateOf(emptyMap<String, List<Line>>())
    override val lines: List<Line> get() = chats[channel].orEmpty()

    private var lastFromRadio = 0L
    private var fromRadioAt = 0L
    private var lastConfirms = 0L
    private var confirmAt = 0L
    private var lastStatusJson = "(the station has not answered)"
    private val main = Handler(Looper.getMainLooper())
    private val link = RadioLink(context)

    init {
        link.start()
        val radioName = link.pairedRadios().firstOrNull()?.name ?: "VR-N76"
        status = status.copy(rig = radioName, btAddress = link.pairedRadios().firstOrNull()?.address ?: "")
        val setup = StationSetup(
            home = context.filesDir.resolve("wcr").path,
            callsign = start.call,
            grid = start.grid,
            mode = start.mode.id,
            frequencyKhz = khz(start.freq).toUInt(),
            // Only Bluetooth-safe presets run over the radio's audio channel.
            preset = if (start.preset in BT_PRESETS) start.preset else "vhf-fm",
            modem73 = context.applicationInfo.nativeLibraryDir + "/libmodem73.so",
            radioPort = link.port.toUShort(),
            radioName = radioName,
        )
        thread(name = "station", isDaemon = true) {
            val err = runCatching { startStation(setup) }.getOrElse { it.message ?: "$it" }
            if (err != null) {
                Log.w(TAG, "station did not start: $err")
                main.post { status = status.copy(radioUp = false, radioNote = err); facts = listOf("Station" to "did not start: $err"); headline = "STATION DID NOT START" to err; show(err) }
                return@thread
            }
            var n = 0
            while (true) {
                val st = runCatching { stationStatus() }.getOrDefault("")
                val chat = runCatching { chatSnapshot() }.getOrDefault("")
                main.post { apply(st, chat) }
                if (n % 4 == 0) fetchTraffic()
                if (n % 6 == 0) fetchMail()
                n++
                Thread.sleep(POLL_MS)
            }
        }
    }

    private fun apply(statusJson: String, chatJson: String) {
        tick++
        if (statusJson.isNotEmpty()) runCatching { applyStatus(JSONObject(statusJson)) }
        else if (!headline.first.startsWith("STATION")) headline = "NOT LINKED" to "the station is not answering yet"
        if (chatJson.isNotEmpty()) runCatching { applyChat(JSONObject(chatJson)) }
    }

    private fun applyStatus(j: JSONObject) {
        j.optString("stopped").takeIf { it.isNotEmpty() }?.let {
            status = status.copy(radioUp = false, radioNote = "station stopped: $it")
            headline = "STATION STOPPED" to it
            return
        }
        val inDb = j.optDouble("audio_in_db", -90.0)
        val outDb = j.optDouble("audio_out_db", -90.0)
        val khz = j.optInt("freq_khz", 0)
        status = status.copy(
            radioUp = j.optBoolean("tnc_ok", false),
            radioNote = j.optString("tnc"),
            preset = j.optString("preset").ifEmpty { status.preset },
            freq = if (khz > 0) "%.3f".format(khz / 1000.0) else status.freq,
            inDb = inDb.toInt(),
            inLevel = level(inDb),
            outDb = outDb.toInt(),
            outLevel = level(outDb),
            snrDb = j.optDouble("snr", 0.0).toInt(),
            queue = j.optInt("queue_out", 0),
            retry = j.optInt("retries", 0),
            occupancy = j.optInt("occupancy_pct", 0),
            hubUp = j.optBoolean("hub_ok", false),
        )
        onAir = if (j.optBoolean("ptt_on")) OnAir(lineId = -1, piece = 1, of = 1, seconds = 0, progress = 0f) else null
        // Only hard evidence: the Bluetooth socket this app holds, bytes it
        // actually passed, and the radio's own replies (0x02 ack, 0x09 TX echo).
        val now = android.os.SystemClock.elapsedRealtime()
        val fromRadio = link.bytesFromRadio
        if (fromRadio > lastFromRadio) { lastFromRadio = fromRadio; fromRadioAt = now }
        val frames = j.optJSONObject("bt_rx_frames")
        // 0x09 is the radio echoing what it transmits (HTCommander's notes); 0x02 acks go
        // the other way, so they prove nothing. Not yet seen from the VR-N76 on the G77.
        val confirms = frames?.optLong("0x09") ?: 0
        if (confirms > lastConfirms) { lastConfirms = confirms; confirmAt = now }
        val keyAsked = j.optBoolean("ptt_on")
        val open = link.open && j.optBoolean("tnc_ok")
        val quietS = if (fromRadioAt > 0) (now - fromRadioAt) / 1000 else -1
        hearing = open && quietS in 0..5
        val reason = j.optString("tnc").ifEmpty { link.log().lastOrNull().orEmpty() }
        headline = when {
            !open -> "NOT LINKED" to reason.ifEmpty { "the station has not reached the radio" }
            keyAsked && now - confirmAt < 3000 -> "TRANSMITTING" to "the radio is confirming the audio it keys"
            keyAsked -> "SENDING TO RADIO" to "audio is going to the radio; check its TX light, it sends no TX echo"
            quietS < 0 -> "CHANNEL OPEN" to "nothing has come from the radio yet (no audio, no replies)"
            quietS <= 5 -> "LINKED" to "the radio is sending audio now"
            else -> "LINKED, RADIO QUIET" to "last audio from the radio ${quietS} s ago"
        }
        status = status.copy(radioUp = open, radioNote = headline.first.lowercase())
        val mine = chats.values.flatten().count { it.mine }
        val theirs = chats.values.flatten().count { !it.mine && it.kind == LineKind.Message }
        facts = listOf(
            "Bluetooth channel" to if (link.open) "open (${link.radio?.name})" else "closed",
            "Bytes from radio" to "%,d".format(fromRadio) + if (quietS in 0..5) " · arriving now" else "",
            "Bytes to radio" to "%,d".format(link.bytesToRadio),
            "TX echoes from radio" to "$confirms",
            "Key asked by modem73" to if (keyAsked) "yes, now" else "no",
            "SBC errors" to "${j.optLong("sbc_decode_errors", 0)}",
            "IN / OUT" to "%.1f / %.1f dB".format(j.optDouble("audio_in_db", -90.0), j.optDouble("audio_out_db", -90.0)),
            "modem73" to j.optString("modem_down").ifEmpty {
                j.optString("preset_running").takeIf { it.isNotEmpty() }?.let { "running ($it)" } ?: "not running yet"
            },
            "Preset · frequency" to j.optString("preset") + " · " + if (khz > 0) "%.3f MHz".format(khz / 1000.0) else "not set",
            "Last decode SNR" to if (j.optLong("snr_ts", 0) > 0) "%.1f dB".format(j.optDouble("snr", 0.0)) else "nothing decoded yet",
            "Waiting to send" to "${j.optInt("queue_out", 0)}",
            "Your lines / lines heard" to "$mine / $theirs",
        )
        lastStatusJson = j.toString(1)
        val heardRows = j.optJSONArray("heard").objects()
        heardAll = heardRows.map(LiveJson::heard).sortedByDescending { it.lastHeard }
        heard = heardRows.mapNotNull { h ->
            h.optDouble("snr").takeIf { !it.isNaN() }?.let { Heard(h.optString("callsign"), it.toInt()) }
        }
        lastHeard = heardRows.maxByOrNull { it.optLong("last_heard") }?.let { h ->
            val ago = System.currentTimeMillis() / 1000 - h.optLong("last_heard")
            h.optString("callsign") + " · " + when {
                h.optLong("last_heard") <= 0 -> "heard"
                ago < 60 -> "just now"
                ago < 3600 -> "${ago / 60} min ago"
                else -> "${ago / 3600} h ago"
            }
        } ?: "nobody yet"
    }

    private fun applyChat(j: JSONObject) {
        val convs = j.optJSONArray("convs").objects()
        chats = convs.associate { c -> c.optString("id") to c.optJSONArray("lines").objects().map(::line) }
        val names = (listOf("#bulletin") + convs.map { it.optString("id") }).distinct()
        channels = names.map { n ->
            val last = chats[n]?.lastOrNull()
            Channel(
                n,
                if (n.startsWith("#")) ChannelKind.Room else ChannelKind.Direct,
                last?.let { if (it.mine) "You: ${it.text}" else if (n.startsWith("#")) "${it.from}: ${it.text}" else it.text }.orEmpty(),
                last?.time.orEmpty(),
            )
        }
    }

    private fun line(l: JSONObject): Line {
        val snr = l.optDouble("snr").takeIf { !it.isNaN() }?.toInt()
        val via = l.optString("via")
        val ticks = l.optString("ticks")
        val raw = l.optString("text")
        // A file that came in: "📎 name (size) saved to PATH". Show the name, keep the path for Save.
        val saved = if (raw.startsWith("📎") && " saved to " in raw) raw.substringAfterLast(" saved to ").trim() else ""
        return Line(
            id = l.optLong("seq"),
            time = l.optString("when").takeLast(5),
            from = l.optString("nick"),
            text = if (saved.isNotEmpty()) raw.substringBeforeLast(" saved to ") else raw,
            mine = l.optBoolean("mine"),
            kind = if (l.optBoolean("sys")) LineKind.Note else LineKind.Message,
            via = when {
                via.isEmpty() -> null
                via == "rf" -> Via.Radio
                else -> Via.Internet
            },
            snrDb = snr,
            keyed = l.optInt("tries", 0),
            filePath = saved,
            progress = l.optString("progress"),
            mark = when {
                "[all]" in ticks -> Mark.AllMembers
                "[ok]" in ticks -> Mark.Delivered
                "[no]" in ticks -> Mark.Refused
                "[gw]" in ticks -> Mark.Gateway
                "[rl]" in ticks -> Mark.Relayed
                "[tx]" in ticks -> Mark.Sent
                "[..]" in ticks -> Mark.Queued
                else -> null
            },
        )
    }

    // ---- Mail, On air and files: the station's own loopback HTTP ----

    private fun call(method: String, path: String, body: String = ""): Pair<Int, String> =
        runCatching { stationCall(method, path, body).let { it.code.toInt() to it.body } }
            .getOrElse { 503 to (it.message ?: "$it") }

    /** Runs off the main thread; [done] gets the station's answer on the main thread. */
    private fun ask(method: String, path: String, body: String = "", done: (Int, String) -> Unit) {
        thread(name = "station-call", isDaemon = true) {
            val (code, text) = call(method, path, body)
            main.post { done(code, text) }
        }
    }

    /** The reason in an error reply, in plain words. */
    private fun why(code: Int, text: String): String =
        runCatching { JSONObject(text).optString("error") }.getOrNull()?.takeIf { it.isNotEmpty() }
            ?: text.ifEmpty { "the station answered $code" }

    private fun fetchMail() {
        val (c1, inboxJson) = call("GET", "/mail/list?folder=inbox")
        if (c1 != 200) return
        val out = call("GET", "/mail/list?folder=outbox").second
        val sent = call("GET", "/mail/list?folder=sent").second
        val wait = call("GET", "/mail/waiting").second
        val tab = call("GET", "/mail/tab").second
        fun rows(t: String) = runCatching { JSONArray(t).objects().map(LiveJson::mail) }.getOrDefault(emptyList())
        val i = rows(inboxJson).sortedByDescending { it.ts }
        val s = (rows(out) + rows(sent)).distinctBy { it.id }.sortedByDescending { it.ts }
        val w = runCatching { JSONArray(wait).objects().map(LiveJson::waiting) }.getOrDefault(emptyList())
        val t = runCatching { LiveJson.tab(JSONObject(tab)) }.getOrNull()
        main.post {
            inbox = i
            sentMail = s
            waitingMail = w
            if (t != null) mailTab = t
        }
    }

    private var trafficLast = 0L

    private fun fetchTraffic() {
        val (code, text) = call("GET", "/traffic?since=$trafficLast")
        if (code != 200) return
        val j = runCatching { JSONObject(text) }.getOrNull() ?: return
        val rows = j.optJSONArray("rows").objects().map(LiveJson::traffic)
        trafficLast = j.optLong("last", trafficLast)
        if (rows.isEmpty()) return
        main.post { traffic = (rows.sortedByDescending { it.seq } + traffic).distinctBy { it.seq }.take(300) }
    }

    /** Open one mail (marks it read). */
    fun openMail(id: String, done: (LiveMail?) -> Unit) =
        ask("GET", "/mail/open?id=" + java.net.URLEncoder.encode(id, "UTF-8")) { code, text ->
            done(if (code == 200) runCatching { LiveJson.mail(JSONObject(text)) }.getOrNull() else null)
            thread(isDaemon = true) { fetchMail() }
        }

    /** Queue a mail; [files] are (name, bytes). The station shrinks pictures and says if it cannot go. */
    fun composeMail(to: String, subject: String, body: String, files: List<Pair<String, ByteArray>>, done: (Boolean) -> Unit) {
        val j = JSONObject().put("to", to.trim()).put("subject", subject).put("body", body)
            .put("files", JSONArray(files.map { (name, data) -> JSONObject().put("name", name).put("data", android.util.Base64.encodeToString(data, android.util.Base64.NO_WRAP)) }))
        ask("POST", "/mail/compose", j.toString()) { code, text ->
            if (code == 200) { show("Queued. It goes out when the channel is clear."); thread(isDaemon = true) { fetchMail() } } else show(why(code, text))
            done(code == 200)
        }
    }

    /** Ask the gateway, over the radio, which mail is waiting (headers only). */
    fun checkMail() = ask("POST", "/mail/check", "{}") { code, text ->
        show(if (code == 200) "Asking the gateway over the radio what is waiting…" else why(code, text))
    }

    /** Pull waiting mail through the gateway. */
    fun getWaiting(ids: List<String>) = ask("POST", "/mail/get", JSONObject().put("ids", JSONArray(ids)).toString()) { code, text ->
        show(if (code == 200) "Asking the gateway for ${if (ids.size == 1) "the mail" else "${ids.size} mails"}…" else why(code, text))
    }

    fun deleteLiveMail(id: String) = ask("POST", "/mail/delete", JSONObject().put("id", id).toString()) { code, text ->
        if (code == 200) { inbox = inbox.filterNot { it.id == id }; sentMail = sentMail.filterNot { it.id == id }; waitingMail = waitingMail.filterNot { it.id == id }; show("Deleted") } else show(why(code, text))
    }

    fun retryMail(id: String) = ask("POST", "/mail/retry", JSONObject().put("id", id).toString()) { code, text ->
        show(if (code == 200) "Trying again" else why(code, text))
    }

    fun cancelMail(id: String) = ask("POST", "/mail/cancel", JSONObject().put("id", id).toString()) { code, text ->
        show(if (code == 200) "Stopped" else why(code, text))
    }

    /** Send a small file in a direct chat. */
    fun sendFile(to: String, name: String, data: ByteArray) {
        thread(isDaemon = true) {
            val err = runCatching { chatSendFile(to, name, data) }.getOrElse { it.message ?: "$it" }
            main.post { show(err ?: "Sending $name to $to") }
        }
    }

    /** Show a short note (the snackbar). */
    fun say(text: String) = show(text)

    /** The callsign and locator the station is running with (read once at start). */
    val runningCall = start.call
    val runningGrid = start.grid

    /** The user changed callsign or locator; the station uses it after a restart. */
    val restartNeeded: Boolean get() = status.call != runningCall || status.grid != runningGrid

    override fun update(change: (Status) -> Status) {
        val before = status
        status = change(status)
        if (status.call != before.call || status.grid != before.grid) {
            context.getSharedPreferences(Station.PREFS, Context.MODE_PRIVATE).edit()
                .putString(Station.CALL, status.call.trim().uppercase())
                .putString(Station.GRID, status.grid.trim())
                .apply()
        }
    }

    override fun openChannel(name: String) {
        channel = name
    }

    override fun send(text: String) {
        val to = channel
        thread { chatSend(to, text)?.let { main.post { show(it) } } }
    }

    override fun stopTx() = show("Stop is not wired to the real radio yet")

    override fun undoQueued() = show("Undo is not wired to the real radio yet")

    /**
     * Settings → Frequency or Preset. The station uses it now and the app keeps
     * it for the next start. Bluetooth audio cannot tune the radio, so the
     * frequency is dialled on the radio by hand.
     */
    override fun tune(freq: String, emission: String, preset: String) {
        val p = if (preset in BT_PRESETS) preset else status.preset
        context.getSharedPreferences(Station.PREFS, Context.MODE_PRIVATE).edit()
            .putString(Station.FREQ, freq).putString(Station.PRESET, p).apply()
        val freqChanged = freq != status.freq
        status = status.copy(freq = freq, preset = p)
        thread {
            val err = radioSetting("freq", freq) ?: radioSetting("preset", p)
            main.post {
                show(
                    when {
                        err != null -> "Saved for the next start. The station said: $err"
                        freqChanged -> "Now $freq MHz. Dial $freq on the ${status.rig} by hand."
                        else -> "Now running $p"
                    },
                )
            }
        }
    }

    override fun getMail(id: Long) = show(NO_MAIL)
    override fun readMail(id: Long) {}
    override fun sendMail(to: String, subject: String, body: String, attachment: Attachment?) = show(NO_MAIL)
    override fun deleteMail(id: Long) {}

    override fun dismissNote() {
        note = null
    }

    private fun show(text: String) {
        note = text
        main.removeCallbacksAndMessages(NOTE)
        main.postAtTime({ note = null }, NOTE, android.os.SystemClock.uptimeMillis() + 4000)
    }

    companion object {
        private const val TAG = "LiveRadio"
        private const val POLL_MS = 500L
        private val NOTE = Any()
        private const val NO_MAIL = "Mail is not on the real radio yet"
        /** Presets that survive the radio's Bluetooth audio (wcr btaudio::preset_allowed). */
        val BT_PRESETS = setOf("vhf-fm", "hf-poor", "hf-weak", "hf-deep")

        private fun khz(mhz: String) = ((mhz.toDoubleOrNull() ?: 0.0) * 1000).toInt()

        /** dBFS to the 0..1 meter: -60 dB is empty, 0 dB full. */
        private fun level(db: Double) = ((db + 60) / 60).coerceIn(0.0, 1.0).toFloat()

    }
}
