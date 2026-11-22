package com.weechatradio.app.radio

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

/** One station for the app's whole life, shared by the screens and the listening service. */
object Station {
    val radio: Radio by lazy { SimulatedRadio() }
}
