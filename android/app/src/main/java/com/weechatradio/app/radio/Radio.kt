package com.weechatradio.app.radio

/** Where a line we sent has got to, in the words the app shows. */
enum class Delivery(val words: String) {
    Waiting("waiting for a clear channel…"),
    Sent("sent"),
    Received("received"),
    NoReceipt("no receipt"),
}

/** How the phone reaches the radio. */
enum class RadioLink(val label: String, val isVox: Boolean) {
    /** VR-N76, UV-PRO, GA-5WB: audio and PTT over the radio's own Bluetooth link. */
    BluetoothRadio("Bluetooth", false),
    /** Any radio paired to the phone as a Bluetooth headset; the radio's VOX keys it. */
    VoxHeadset("VOX · Bluetooth headset", true),
    /** Any radio on a USB-C to K1 audio lead; the radio's VOX keys it. */
    VoxCable("VOX · USB-C cable", true),
}

data class ChatMessage(
    val id: Long,
    val from: String,
    val text: String,
    val time: String,
    val mine: Boolean,
    val delivery: Delivery? = null,
)

data class Chat(
    /** "#bulletin" for a room, a callsign for a person. */
    val name: String,
    val messages: List<ChatMessage>,
    val unread: Int = 0,
    /** Last heard through the internet hub rather than on air. */
    val viaInternet: Boolean = false,
    val lastHeard: String = "",
) {
    val isRoom: Boolean get() = name.startsWith("#")
}

enum class MailFolder { Inbox, Outbox, Sent }

data class Mail(
    val id: Long,
    val folder: MailFolder,
    /** Who it is from (Inbox) or to (Outbox, Sent). */
    val peer: String,
    val subject: String,
    val body: String,
    val time: String,
    /** Only the header has come over the air; Get pulls the body. */
    val downloaded: Boolean = true,
    val attachment: String? = null,
)

data class OnAirEvent(val time: String, val what: String, val detail: String, val sent: Boolean)

data class RadioStatus(
    val radioName: String = "VR-N76",
    val link: RadioLink = RadioLink.BluetoothRadio,
    val connected: Boolean = false,
    val frequencyMhz: String = "144.950",
    val preset: String = "vhf-fm",
    val mode: String = "radio-plus",
    val callsign: String = "M7TJF",
    val grid: String = "IO81UF",
    /** 0..1, how well we hear the channel. */
    val hearing: Float = 0f,
    /** 0..1, how much of the time the channel is busy. */
    val channelBusy: Float = 0f,
    val snrDb: Int = 0,
    val txLevelDb: Int = -15,
    val useInternetHub: Boolean = true,
    val sentToday: Int = 0,
    val heardToday: Int = 0,
)

/**
 * Everything the screens need from a station. [SimulatedRadio] fakes it for
 * now; the real one (wcr core + modem73 + Bluetooth) comes with Phase 0.
 *
 * Properties are Compose state, so screens redraw when they change.
 */
interface Radio {
    val status: RadioStatus
    val chats: List<Chat>
    val mail: List<Mail>
    val onAir: List<OnAirEvent>
    val mailAddress: String

    fun connect(name: String, link: RadioLink)
    /** Back to the Connect radio screen, to pick another radio. */
    fun disconnect()
    fun sendChat(name: String, text: String)
    fun openChat(name: String)
    fun sendMail(to: String, subject: String, body: String)
    fun getMail(id: Long)
    fun deleteMail(id: Long)
    fun setFrequency(mhz: String)
    fun setPreset(name: String)
    fun setCallsign(callsign: String)
    fun setTxLevel(db: Int)
    fun setUseInternetHub(on: Boolean)
}
