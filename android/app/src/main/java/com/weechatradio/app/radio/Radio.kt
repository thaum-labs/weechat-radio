package com.weechatradio.app.radio

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/**
 * Everything the screens need from a station. [SimulatedRadio] fakes it until
 * the real link to the radio (Phase 0) is built; the screens never know which.
 * Reads are Compose state, so screens redraw when they change.
 */
interface Radio {
    val status: Status
    /** The message being keyed now, or null. */
    val onAir: OnAir?
    /** The open chat: "#bulletin", a room or a callsign. */
    val channel: String
    val channels: List<Channel>
    val heard: List<Heard>
    /** Stations for the live map, ours first. */
    val stations: List<MapStation>
    /** Lines of the open chat, oldest first. */
    val lines: List<Line>
    val mails: List<Mail>
    /** Frames heard or sent, newest first. */
    val frames: List<Frame>
    /** A short note for the snackbar, or null. */
    val note: String?
    /** Moves on while the station runs, to scroll the waterfall. */
    val tick: Int

    fun update(change: (Status) -> Status)
    fun openChannel(name: String)
    fun send(text: String)
    /** Stop keying now. */
    fun stopTx()
    /** Take back the last queued line (snackbar UNDO). */
    fun undoQueued()
    /** Tune to a frequency and preset. On Bluetooth the radio is set by hand, so this says what to dial. */
    fun tune(freq: String, emission: String, preset: String)
    /** Pull a waiting mail's body through the gateway. */
    fun getMail(id: Long)
    fun readMail(id: Long)
    fun sendMail(to: String, subject: String, body: String, attachment: Attachment?)
    fun deleteMail(id: Long)
    fun dismissNote()
}

/**
 * One station for the app's whole life, shared by the screens and the
 * listening service. It starts on the simulated radio; Settings → Radio →
 * "Use the real radio" switches to [LiveRadio] (Phase 0). The real station
 * starts once per app run, so switching back only hides it.
 */
object Station {
    private val simulated by lazy { SimulatedRadio() }
    private var live: LiveRadio? = null

    var radio: Radio by mutableStateOf<Radio>(simulated)
        private set

    fun useReal(context: Context, on: Boolean) {
        radio = if (on) live ?: LiveRadio(context.applicationContext, simulated.status).also { live = it } else simulated
    }
}
