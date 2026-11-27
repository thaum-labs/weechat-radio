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
import uniffi.wcr_mobile.chatSend
import uniffi.wcr_mobile.chatSnapshot
import uniffi.wcr_mobile.radioSetting
import uniffi.wcr_mobile.startStation
import uniffi.wcr_mobile.stationStatus
import kotlin.concurrent.thread

/**
 * The real station (Phase 0): the wcr node and modem73 run on the phone and
 * reach the radio over [RadioLink]. Only real things show: radio status,
 * chat and heard stations. Mail, the map and the "On the air" list are empty
 * here (no sample data) until their turn comes.
 */
class LiveRadio(
    context: Context,
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
    override val stations: List<MapStation> get() = emptyList()

    /** The test panel on the Radio tab: plain facts from the station, nothing made up. */
    var facts by mutableStateOf(listOf("Station" to "starting…"))
        private set

    private var chats by mutableStateOf(emptyMap<String, List<Line>>())
    override val lines: List<Line> get() = chats[channel].orEmpty()

    private var lastRxBytes = 0L
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
                main.post { status = status.copy(radioUp = false, radioNote = err); facts = listOf("Station" to "did not start: $err"); show(err) }
                return@thread
            }
            while (true) {
                val st = runCatching { stationStatus() }.getOrDefault("")
                val chat = runCatching { chatSnapshot() }.getOrDefault("")
                main.post { apply(st, chat) }
                Thread.sleep(POLL_MS)
            }
        }
    }

    private fun apply(statusJson: String, chatJson: String) {
        tick++
        if (statusJson.isNotEmpty()) runCatching { applyStatus(JSONObject(statusJson)) }
        if (chatJson.isNotEmpty()) runCatching { applyChat(JSONObject(chatJson)) }
    }

    private fun applyStatus(j: JSONObject) {
        j.optString("stopped").takeIf { it.isNotEmpty() }?.let {
            status = status.copy(radioUp = false, radioNote = "station stopped: $it")
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
        val rxBytes = j.optLong("bt_rx_bytes", 0)
        val mine = chats.values.flatten().count { it.mine }
        val theirs = chats.values.flatten().count { !it.mine && it.kind == LineKind.Message }
        facts = listOf(
            "Radio link" to if (j.optBoolean("tnc_ok")) "linked · ${status.rig}" else j.optString("tnc").ifEmpty { "not linked" },
            "Transmitting" to if (j.optBoolean("ptt_on")) "YES, keyed now" else "no",
            "Audio from radio" to "%,d bytes".format(rxBytes) + if (rxBytes > lastRxBytes) " · arriving" else " · quiet",
            "SBC errors" to "${j.optLong("sbc_decode_errors", 0)}",
            "IN level" to "%.1f dB".format(j.optDouble("audio_in_db", -90.0)),
            "OUT level" to "%.1f dB".format(j.optDouble("audio_out_db", -90.0)),
            "modem73" to j.optString("modem73").ifEmpty { "not running yet" },
            "Preset" to j.optString("preset"),
            "Frequency" to if (khz > 0) "%.3f MHz".format(khz / 1000.0) else "not set",
            "Last SNR" to if (j.optLong("snr_ts", 0) > 0) "%.1f dB".format(j.optDouble("snr", 0.0)) else "nothing decoded yet",
            "Waiting to send" to "${j.optInt("queue_out", 0)}",
            "Lines sent / heard" to "$mine / $theirs",
            "Internet hub" to if (j.optBoolean("hub_ok")) "connected" else "not connected",
        )
        lastRxBytes = rxBytes
        heard = j.optJSONArray("heard").objects().mapNotNull { h ->
            h.optDouble("snr").takeIf { !it.isNaN() }?.let { Heard(h.optString("callsign"), it.toInt()) }
        }
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
                last?.let { "${it.from}: ${it.text}" }.orEmpty(),
                last?.time.orEmpty(),
            )
        }
    }

    private fun line(l: JSONObject): Line {
        val snr = l.optDouble("snr").takeIf { !it.isNaN() }?.toInt()
        val via = l.optString("via")
        val ticks = l.optString("ticks")
        return Line(
            id = l.optLong("seq"),
            time = l.optString("when").takeLast(5),
            from = l.optString("nick"),
            text = l.optString("text"),
            mine = l.optBoolean("mine"),
            kind = if (l.optBoolean("sys")) LineKind.Note else LineKind.Message,
            via = when {
                via.isEmpty() -> null
                via == "rf" -> Via.Radio
                else -> Via.Internet
            },
            snrDb = snr,
            keyed = l.optInt("tries", 0),
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

    override fun update(change: (Status) -> Status) {
        status = change(status)
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

    override fun tune(freq: String, emission: String, preset: String) {
        thread {
            val err = radioSetting("freq", freq) ?: radioSetting("preset", preset)
            main.post { show(err ?: "Set the radio to $freq MHz $emission by hand; the station now uses $preset") }
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

        private fun JSONArray?.objects(): List<JSONObject> =
            if (this == null) emptyList() else (0 until length()).mapNotNull { optJSONObject(it) }
    }
}
