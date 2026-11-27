package com.weechatradio.app.radio

/** Station mode, as on the desktop. Colours live in ui/theme ModeColors. */
enum class Mode(val id: String, val words: String) {
    Internet("internet", "chat over the hub, no radio"),
    InternetRadio("internet-radio", "radio + hub gateway"),
    Radio("radio", "radio only, nothing forwarded"),
    RadioPlus("radio-plus", "radio, a gateway may carry mail"),
}

/** How the phone reaches the radio (setup step 1). */
enum class Link(val label: String) {
    /** VR-N76, UV-PRO, GA-5WB, VR-N7600: the radio does audio and PTT. */
    Bluetooth("Bluetooth"),
    /** Digirig or a USB sound card, PTT by CAT, VOX or RTS. */
    Usb("USB cable"),
    InternetOnly("No radio, internet only"),
}

/** What runs over the Bluetooth link (setup step 1b). */
enum class BtAudio(val label: String) {
    /** Radio audio into modem73 on the phone. */
    Modem73("modem73"),
    /** The radio's own KISS TNC, afsk-1200 only. */
    Tnc1200("TNC 1200"),
    /** Any radio paired as a Bluetooth headset, keyed by its own VOX. */
    HeadsetVox("Headset VOX"),
}

/** How a USB-cabled radio is keyed. */
enum class Ptt { CAT, VOX, RTS }

/** How a line reached us. */
enum class Via { Radio, Internet }

/** The receipt marks of a line (see the "What the marks mean" sheet). */
enum class Mark { Queued, Sent, Relayed, Gateway, Delivered, AllMembers, Refused }

enum class LineKind { Message, Day, Note, Rest }

data class Line(
    val id: Long,
    val time: String = "",
    val from: String = "",
    val text: String,
    val mine: Boolean = false,
    val kind: LineKind = LineKind.Message,
    val via: Via? = null,
    val snrDb: Int? = null,
    /** Times it was keyed (×1, ×2); 0 = not yet. */
    val keyed: Int = 0,
    val mark: Mark? = null,
    /** Pieces of one long message on the air, and how many of them arrived. */
    val pieces: Int = 1,
    val piecesHeard: Int = pieces,
    /** While keying: the piece on the air now (1-based), 0 = not on air. */
    val sendingPiece: Int = 0,
    /** A file that came in on this line: where the station saved it. */
    val filePath: String = "",
    /** A file on its way, as the station draws it: "[████░░░░] 3/15". */
    val progress: String = "",
)

enum class ChannelKind { Room, Closed, Direct }

data class Channel(
    val name: String,
    val kind: ChannelKind,
    val summary: String,
    val time: String,
    val unread: Int = 0,
)

data class Heard(val call: String, val snrDb: Int)

/** A station on the live map: where it is, its mode, and when and how we last heard it. */
data class MapStation(
    val call: String,
    val lat: Double,
    val lon: Double,
    val mode: Mode,
    val place: String,
    /** "22:52"; empty if never heard. */
    val lastHeard: String,
    /** SNR when heard by radio; null when only seen through the hub. */
    val snrDb: Int?,
    /** Heard by radio in the last 30 min: drawn with a live line to us. */
    val live: Boolean,
)

enum class Folder { Inbox, Outbox, Sent }

data class Attachment(val name: String, val size: String)

data class Mail(
    val id: Long,
    val folder: Folder,
    /** From (Inbox) or to (Outbox, Sent). */
    val peer: String,
    val subject: String,
    val body: String = "",
    val time: String,
    /** Long form for the open mail, e.g. "Today 22:40". */
    val day: String = time,
    val unread: Boolean = false,
    val attachment: Attachment? = null,
    /** Only the header has come over the air; Get pulls the body through the gateway. */
    val waiting: Boolean = false,
    val waitingSize: String = "",
    /** Sent mail: delivered, and the steps it took (local time, what happened). */
    val delivered: Boolean = false,
    val journey: List<Pair<String, String>> = emptyList(),
)

enum class FrameKind(val words: String) { Bulletin("bulletin"), Receipt("receipt"), Mail("mail"), Direct("dm") }

/** One frame heard or sent, for the read-only "On the air" list. */
data class Frame(
    val time: String,
    val kind: FrameKind,
    val route: String,
    val snrDb: Int?,
    val via: String,
    val text: String = "",
    val sent: Boolean = false,
)

/** A message being keyed right now. */
data class OnAir(val lineId: Long, val piece: Int, val of: Int, val seconds: Int, val progress: Float)

data class Status(
    val call: String = "M7TJF",
    val grid: String = "IO81UF",
    val mode: Mode = Mode.RadioPlus,
    val link: Link = Link.Usb,
    val btAudio: BtAudio = BtAudio.Modem73,
    val ptt: Ptt = Ptt.CAT,
    /** "Digirig Mini" or "VR-N76". */
    val rig: String = "Digirig Mini",
    val btAddress: String = "38:D2:00:01:03:49",
    val preset: String = "vhf-fast",
    val freq: String = "433.625",
    val emission: String = "NFM",
    val power: String = "5 W",
    /** 0..1 and dB, as the meters show them. */
    val inLevel: Float = 0.52f,
    val inDb: Int = -18,
    val outLevel: Float = 0.42f,
    val outDb: Int = -14,
    /** The OUT level set in setup, in dB. */
    val outSetDb: Float = -6.0f,
    val snrDb: Int = 11,
    val occupancy: Int = 18,
    val queue: Int = 1,
    val air: Int = 0,
    val retry: Int = 0,
    val beacon: String = "in 4 min",
    val hold: String = "retry in 9 s",
    val hubUp: Boolean = true,
    /** Mail gateway strength, 0..3 bars. */
    val mailGwBars: Int = 3,
    val gateway: String = "M7TJF",
    val gatewayPlace: String = "Pi, Frome",
    val gps: String = "51.2308, -2.3207 · fix · 9 sats",
    val phoneBattery: Int = 78,
    /** The radio's battery, when the radio tells us. null shows a dash. */
    val radioBattery: Int? = null,
    val listenInBackground: Boolean = true,
    /** False after Stop in the notification. */
    val listening: Boolean = true,
    /** Alerts muted from the notification. */
    val muted: Boolean = false,
    val showSnr: Boolean = true,
    val copyTo: String = "me@example.com",
    val copyToConfirmed: Boolean = true,
    val txDuty: String = "handheld",
    val setUp: Boolean = true,
    /** The real station: is the radio's audio channel open, and if not, what the core says. */
    val radioUp: Boolean = true,
    val radioNote: String = "",
    /** True when the real station runs (Phase 0); false on the simulated radio. */
    val real: Boolean = false,
) {
    val mailAddress: String get() = "$call@mail.weechatradio.com"
    val linked: Boolean get() = link != Link.InternetOnly
}
