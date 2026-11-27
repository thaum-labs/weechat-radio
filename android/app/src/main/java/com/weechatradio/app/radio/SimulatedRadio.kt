package com.weechatradio.app.radio

import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.time.LocalTime
import java.time.format.DateTimeFormatter

/**
 * A pretend station with the sample data from the designs (docs/design/android).
 * XX0… callsigns are made-up stations. When [live], a timer keys queued lines,
 * moves the waterfall and turns sent lines into receipts, so the app can be
 * tried without a radio. Tests pass live = false so nothing moves.
 */
class SimulatedRadio(
    start: Status = Status(),
    startOnAir: Boolean = false,
    private val live: Boolean = true,
) : Radio {
    override var status by mutableStateOf(start)
        private set
    override var onAir by mutableStateOf<OnAir?>(null)
        private set
    override var channel by mutableStateOf("#bulletin")
        private set
    override var channels by mutableStateOf(sampleChannels)
        private set
    override val heard = sampleHeard
    override val stations: List<MapStation> get() = listOf(MapStation(status.call, 51.2308, -2.3207, status.mode, "Frome", "", null, true)) + sampleStations
    override val lines: List<Line> get() = chats[channel] ?: emptyList()
    override var mails by mutableStateOf(sampleMails)
        private set
    override var frames by mutableStateOf(sampleFrames)
        private set
    override var note by mutableStateOf<String?>(null)
        private set
    override var tick by mutableIntStateOf(0)
        private set

    private var chats by mutableStateOf(mapOf("#bulletin" to sampleLines))
    private var nextId = 100L
    private val handler = Handler(Looper.getMainLooper())
    private val clearNote = Runnable { note = null }
    private val step = object : Runnable {
        override fun run() {
            advance()
            handler.postDelayed(this, STEP_MS)
        }
    }

    init {
        if (startOnAir) {
            chats = mapOf("#bulletin" to sampleLinesOnAir)
            onAir = OnAir(lineId = 31, piece = 2, of = 3, seconds = 7, progress = 0.66f)
            note = "Queued behind the message on air"
        }
        if (live) handler.postDelayed(step, STEP_MS)
    }

    override fun update(change: (Status) -> Status) {
        status = change(status)
    }

    override fun openChannel(name: String) {
        channel = name
        if (name !in chats) chats = chats + (name to emptyList())
        channels = channels.map { if (it.name == name) it.copy(unread = 0) else it }
    }

    override fun send(text: String) {
        val t = text.trim()
        if (t.isEmpty()) return
        val pieces = (t.length + PIECE_CHARS - 1) / PIECE_CHARS
        val busy = onAir != null
        val line = Line(
            id = nextId++, time = now(), from = status.call, text = t, mine = true,
            mark = Mark.Queued, pieces = pieces,
        )
        chats = chats + (channel to lines + line)
        if (busy) say("Queued behind the message on air")
        frames = listOf(Frame(now() + ":00", FrameKind.Bulletin, "${status.call} → $channel", null, "sent", t, sent = true)) + frames
    }

    override fun stopTx() {
        val air = onAir ?: return
        onAir = null
        edit(air.lineId) { it.copy(mark = Mark.Refused, sendingPiece = 0) }
    }

    override fun undoQueued() {
        val last = lines.lastOrNull { it.mine && it.mark == Mark.Queued && it.sendingPiece == 0 } ?: return
        if (onAir?.lineId == last.id) return
        chats = chats + (channel to lines.filter { it.id != last.id })
        note = null
    }

    override fun tune(freq: String, emission: String, preset: String) {
        status = status.copy(freq = freq, emission = emission, preset = preset)
        say(if (status.link == Link.Bluetooth) {
            "Set the ${status.rig} to $freq $emission by hand: Bluetooth can't tune it yet"
        } else {
            "Tuned to $freq $emission + $preset"
        })
    }

    override fun getMail(id: Long) {
        mails = mails.map {
            if (it.id == id) it.copy(
                waiting = false, time = now(), day = "Today ${now()}",
                body = "Here are the photos from Saturday's walk. The view from the top was worth it.\n\nAnn",
                attachment = Attachment("bath-walk.jpg", "17.9 KB"),
            ) else it
        }
    }

    override fun readMail(id: Long) {
        mails = mails.map { if (it.id == id) it.copy(unread = false) else it }
    }

    override fun sendMail(to: String, subject: String, body: String, attachment: Attachment?) {
        mails = listOf(
            Mail(nextId++, Folder.Outbox, to.trim(), subject.trim().ifEmpty { "(no subject)" }, body, now(), attachment = attachment),
        ) + mails
        say("Mail queued for the gateway")
    }

    override fun deleteMail(id: Long) {
        mails = mails.filter { it.id != id }
    }

    override fun dismissNote() {
        note = null
    }

    /** One timer step: move the waterfall, key the next piece, hand out receipts. */
    private fun advance() {
        tick++
        if (!status.listening) return
        val air = onAir
        if (air == null) {
            val next = chats.values.flatten().firstOrNull { it.mine && it.mark == Mark.Queued } ?: return
            onAir = OnAir(next.id, 1, next.pieces, 0, 0f)
            edit(next.id) { it.copy(mark = Mark.Sent, sendingPiece = 1, keyed = 1) }
            return
        }
        val progress = air.progress + 1f / (air.of * STEPS_PER_PIECE)
        val piece = (progress * air.of).toInt() + 1
        if (progress >= 1f) {
            onAir = null
            edit(air.lineId) { it.copy(sendingPiece = 0) }
            handler.postDelayed({ edit(air.lineId) { if (it.mark == Mark.Sent) it.copy(mark = Mark.Delivered) else it } }, 2_000)
        } else {
            onAir = air.copy(piece = piece.coerceAtMost(air.of), seconds = (tick / 4), progress = progress)
            edit(air.lineId) { it.copy(sendingPiece = piece.coerceAtMost(air.of)) }
        }
    }

    private fun say(text: String) {
        note = text
        if (live) {
            handler.removeCallbacks(clearNote)
            handler.postDelayed(clearNote, 4_000)
        }
    }

    private fun edit(id: Long, change: (Line) -> Line) {
        chats = chats.mapValues { (_, l) -> l.map { if (it.id == id) change(it) else it } }
    }

    private fun now(): String = LocalTime.now().format(DateTimeFormatter.ofPattern("HH:mm"))

    private companion object {
        const val STEP_MS = 250L
        const val STEPS_PER_PIECE = 12
        const val PIECE_CHARS = 120
    }
}

// ---- Sample data from docs/design/android/material (XX0… are made-up stations) ----

private val sampleLines = listOf(
    Line(1, text = "Saturday 21 Nov", kind = LineKind.Day),
    Line(2, "22:41", "XX0AAA", "Pi gateway in Frome is up if anyone wants to test mail ✉️", via = Via.Radio, snrDb = 10),
    Line(3, "22:48", "XX0BBB", "cq cq from Bath, anyone about on 433.625? ☀️", via = Via.Radio, snrDb = 11),
    Line(4, "22:49", "XX0AAA", "Evening XX0BBB! Strong in Frome, 5/9 👍", via = Via.Radio, snrDb = 9),
    Line(5, "22:51", "M7TJF", "Hi both, M7TJF here. Phone station now live 📱", mine = true, keyed = 1, mark = Mark.Delivered),
    Line(
        6, "22:52", "XX0BBB",
        "Net notes for Sunday: we meet on 433.625 NFM at 10:00, low power only, handhelds rest between blocks. " +
            "M7TJF, can you run net control from Frome? I will relay from Bath if the ridge gets in the way.",
        via = Via.Radio, snrDb = -4, pieces = 3,
    ),
    Line(7, "22:54", "XX0AAA", "👍👍", via = Via.Radio, snrDb = -7),
    Line(8, "22:55", "M7TJF", "Net control is fine, see you at 10:00. 73", mine = true, keyed = 1, mark = Mark.Delivered),
    Line(9, text = "XX0CCC joined #bulletin", kind = LineKind.Note),
)

private val sampleLinesOnAir = listOf(
    Line(1, text = "Saturday 21 Nov", kind = LineKind.Day),
    Line(4, "22:49", "XX0AAA", "Evening XX0BBB! Strong in Frome, 5/9 👍", via = Via.Radio, snrDb = 9),
    Line(5, "22:51", "M7TJF", "Hi both, M7TJF here. Phone station now live 📱", mine = true, keyed = 1, mark = Mark.Delivered),
    Line(6, "22:52", "XX0BBB", "Net notes for Sunday: we meet on 433.625 NFM at 10:00, low power only.", via = Via.Radio, snrDb = -4),
    Line(
        31, "23:30", "M7TJF",
        "Net control is fine. I will set up in Frome by 09:30, bring a flask and the Pi gateway so mail works too. " +
            "If the ridge blocks Bath, XX0BBB relays. Please check IN levels before TX and keep to NARROW, low power indoors. 73 and see you then ☕",
        mine = true, keyed = 1, mark = Mark.Sent, pieces = 3, sendingPiece = 2,
    ),
    Line(32, text = "Rest 6 s between blocks (handheld duty)", kind = LineKind.Rest),
    Line(33, "23:31", "M7TJF", "Also testing the phone mail screen tonight 📧", mine = true, mark = Mark.Queued),
)

private val sampleChannels = listOf(
    Channel("#bulletin", ChannelKind.Room, "everyone · radio + hub", "22:55"),
    Channel("#frome-bath-net", ChannelKind.Closed, "closed room · 3 members · [all] receipts", "22:31"),
    Channel("XX0BBB", ChannelKind.Direct, "Net notes for Sunday: we meet on…", "22:52", unread = 1),
    Channel("XX0AAA", ChannelKind.Direct, "👍👍", "22:54"),
)

private val sampleHeard = listOf(Heard("XX0BBB", 11), Heard("XX0AAA", 9), Heard("XX0CCC", 2), Heard("XX0DDD", 6))

private val sampleStations = listOf(
    MapStation("XX0BBB", 51.3811, -2.3590, Mode.RadioPlus, "Bath", "22:52", 11, live = true),
    MapStation("XX0AAA", 51.2290, -2.3050, Mode.InternetRadio, "Frome (gateway)", "22:54", 9, live = true),
    MapStation("XX0DDD", 51.3190, -2.2080, Mode.RadioPlus, "Trowbridge", "22:47", 6, live = true),
    MapStation("XX0CCC", 51.2050, -2.1800, Mode.Radio, "Warminster", "22:31", 2, live = false),
    MapStation("XX0EEE", 51.4545, -2.5879, Mode.Internet, "Bristol", "22:58", null, live = false),
    MapStation("XX0FFF", 51.0688, -1.7945, Mode.Internet, "Salisbury", "21:40", null, live = false),
)

private val sampleMails = listOf(
    Mail(1, Folder.Inbox, "ann@example.com", "Photos from the Bath walk", time = "", unread = true, waiting = true, waitingSize = "18.2 KB"),
    Mail(2, Folder.Inbox, "wcr-qa@mail.grokbot.com", "Re: phone attachment test", "Arrived intact, checksum matches. 73", "23:12", "Today 23:12", unread = true, attachment = Attachment("qa-32k.bin", "32.0 KB")),
    Mail(
        3, Folder.Inbox, "XX0BBB@mail.weechatradio.com", "Net notes for Sunday",
        "Hi Tobias,\n\nNotes for the Sunday net:\n\n1. 433.625 NFM, vhf-fast, 10:00\n2. Low power, handhelds rest 6 s between blocks\n" +
            "3. You run net control from Frome, I relay from Bath if the ridge gets in the way ⛰️\n\nPlan attached. Good luck with the phone app! 73\nXX0BBB",
        "22:40", "Today\n22:40", unread = true, attachment = Attachment("net-plan.txt", "1.2 KB"),
    ),
    Mail(4, Folder.Inbox, "XX0AAA@mail.weechatradio.com", "Re: aerial height at Frome", "Six metres on the mast should clear the ridge.", "Fri"),
    Mail(5, Folder.Inbox, "ann@example.com", "Hello from the hill", "Lovely day for it.", "Thu", attachment = Attachment("hill.jpg", "12.4 KB")),
    Mail(6, Folder.Inbox, "newsletter@rsgb.example", "Your callsign renewal reminder", "Your licence is due for revalidation.", "14 Nov"),
    Mail(
        7, Folder.Sent, "wcr-qa@mail.grokbot.com", "Phone attachment test",
        "Sending qa-32k.bin from the phone over vhf-fast. Please confirm it arrives intact. 73 de M7TJF",
        "23:04", "Today 23:04", attachment = Attachment("qa-32k.bin", "32.0 KB"), delivered = true,
        journey = listOf("23:04" to "Written", "23:05" to "On air", "23:07" to "At gateway M7TJF", "23:07" to "Sent on", "23:08" to "Delivered"),
    ),
    Mail(8, Folder.Sent, "XX0BBB@mail.weechatradio.com", "Re: Net notes for Sunday", "Count me in. 73", "22:45", "Today 22:45", delivered = true),
    Mail(9, Folder.Sent, "ann@example.com", "Walk photos?", "Did you get any good ones?", "Fri", "Fri 18:02", delivered = true),
)

private val sampleFrames = listOf(
    Frame("23:31:08", FrameKind.Bulletin, "M7TJF → #bulletin", null, "sent", "Also testing the phone mail screen tonight 📧", sent = true),
    Frame("23:30:41", FrameKind.Receipt, "XX0AAA → M7TJF", 10, "rf"),
    Frame("23:24:55", FrameKind.Mail, "M7TJF → M7TJF", 8, "rf", "chunk 3/3 · wcr-qa@mail.grokbot.com"),
    Frame("23:22:03", FrameKind.Direct, "XX0BBB → M7TJF", 11, "rf", "Heard the 3 pieces fine 👍"),
    Frame("23:19:47", FrameKind.Bulletin, "XX0CCC → #bulletin", 2, "internet", "See you all Sunday"),
)
