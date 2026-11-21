package com.weechatradio.app.radio

import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.time.LocalTime
import java.time.format.DateTimeFormatter
import kotlin.random.Random

/**
 * A pretend station for building the screens before the phone arrives.
 * Lines you send go "on air" after a short wait and get a receipt; people
 * sometimes answer; the levels wander about like a real channel.
 */
class SimulatedRadio(initial: RadioStatus = RadioStatus()) : Radio {
    private val main by lazy { Handler(Looper.getMainLooper()) }
    private var nextId = 100L

    override var status by mutableStateOf(initial)
    override var chats by mutableStateOf(sampleChats())
    override var mail by mutableStateOf(sampleMail())
    override var onAir by mutableStateOf(sampleOnAir())
    override val mailAddress: String get() = "${status.callsign}@mail.weechatradio.com"

    override fun connect(name: String, link: RadioLink) {
        // VOX starts on a robust preset (hf-poor is what worked for Tobias over
        // speaker and mic); the Bluetooth-PTT radios keep vhf-fm.
        val preset = if (link.isVox) "hf-poor" else status.preset
        status = status.copy(
            connected = true, radioName = name, link = link, preset = preset,
            sentToday = 14, heardToday = 31,
        )
        main.removeCallbacksAndMessages(null)
        tickLevels()
    }

    override fun disconnect() {
        main.removeCallbacksAndMessages(null)
        status = status.copy(connected = false)
    }

    private fun tickLevels() {
        status = status.copy(
            hearing = (0.55f + Random.nextFloat() * 0.15f),
            channelBusy = Random.nextFloat() * 0.08f,
            snrDb = Random.nextInt(5, 10),
        )
        main.postDelayed(::tickLevels, 2000)
    }

    override fun openChat(name: String) {
        chats = chats.map { if (it.name == name) it.copy(unread = 0) else it }
    }

    override fun sendChat(name: String, text: String) {
        val id = nextId++
        val line = ChatMessage(id, status.callsign, text, now(), mine = true, delivery = Delivery.Waiting)
        if (chats.none { it.name == name }) chats = listOf(Chat(name, emptyList())) + chats
        chats = chats.map { if (it.name == name) it.copy(messages = it.messages + line) else it }
        main.postDelayed({
            setDelivery(name, id, if (name.startsWith("#")) Delivery.Sent else Delivery.Received)
            heard("$name · you", sent = true)
        }, 1800)
        if (!name.startsWith("#") && Random.nextInt(3) == 0) {
            main.postDelayed({
                val reply = ChatMessage(nextId++, name, "Copy that, 73.", now(), mine = false)
                chats = chats.map { if (it.name == name) it.copy(messages = it.messages + reply) else it }
                heard(name, sent = false)
            }, 5000)
        }
    }

    private fun setDelivery(name: String, id: Long, d: Delivery) {
        chats = chats.map { c ->
            if (c.name != name) c
            else c.copy(messages = c.messages.map { if (it.id == id) it.copy(delivery = d) else it })
        }
    }

    private fun heard(what: String, sent: Boolean) {
        val detail = if (sent) "sent" else "heard · ${status.snrDb} dB"
        onAir = listOf(OnAirEvent(now(), what, detail, sent)) + onAir.take(19)
        status = if (sent) status.copy(sentToday = status.sentToday + 1)
        else status.copy(heardToday = status.heardToday + 1)
    }

    override fun sendMail(to: String, subject: String, body: String) {
        val m = Mail(nextId++, MailFolder.Outbox, to, subject, body, now())
        mail = listOf(m) + mail
        main.postDelayed({
            mail = mail.map { if (it.id == m.id) it.copy(folder = MailFolder.Sent) else it }
            heard("Mail to $to", sent = true)
        }, 4000)
    }

    override fun getMail(id: Long) {
        main.postDelayed({
            mail = mail.map {
                if (it.id == id) it.copy(downloaded = true, body = "Sounds great, see you there.\n\nSam") else it
            }
        }, 2500)
    }

    override fun deleteMail(id: Long) {
        mail = mail.filterNot { it.id == id }
    }

    override fun setFrequency(mhz: String) { status = status.copy(frequencyMhz = mhz) }
    override fun setPreset(name: String) { status = status.copy(preset = name) }
    override fun setCallsign(callsign: String) { status = status.copy(callsign = callsign.uppercase()) }
    override fun setTxLevel(db: Int) { status = status.copy(txLevelDb = db) }
    override fun setUseInternetHub(on: Boolean) { status = status.copy(useInternetHub = on) }

    private fun now(): String = LocalTime.now().format(DateTimeFormatter.ofPattern("HH:mm"))
}

private fun sampleChats() = listOf(
    Chat("#bulletin", listOf(ChatMessage(1, "2E0XYZ", "73 both, 2E0XYZ out", "21:26", false)), unread = 2),
    Chat(
        "M0ABC",
        listOf(
            ChatMessage(2, "M0ABC", "Evening, are you on the hill tomorrow?", "21:08", false),
            ChatMessage(3, "M7TJF", "Yes, portable with the N76 from 10. How am I coming in?", "21:10", true, Delivery.Received),
            ChatMessage(4, "M0ABC", "5 and 9 here. Mail is on its way.", "21:12", false),
            ChatMessage(5, "M7TJF", "Thanks, will listen on 144.950", "21:14", true, Delivery.NoReceipt),
            ChatMessage(6, "M0ABC", "Just going to QSY to the calling freq", "21:22", false),
        ),
        unread = 1,
        lastHeard = "Heard 2 min ago · by radio · 1 dB",
    ),
    Chat("#local", listOf(ChatMessage(7, "G4XYZ", "anyone hearing the Pi gateway?", "20:51", false))),
    Chat(
        "2E0FOX",
        listOf(ChatMessage(8, "M7TJF", "QSY to 145.500?", "Tue", true, Delivery.Received)),
        viaInternet = true,
        lastHeard = "Heard Tue · by internet",
    ),
    Chat("#emcomm", listOf(ChatMessage(9, "M0QRP", "Net check-in at 19:00 local", "Mon", false))),
)

private fun sampleMail() = listOf(
    Mail(11, MailFolder.Inbox, "sam@example.com", "Weekend plans", "", "21:20", downloaded = false),
    Mail(
        12, MailFolder.Inbox, "G4XYZ", "Repeater list",
        "Hi Tobias,\n\nHere are the ones I can hit from the hill with the handheld:\n\n" +
            "GB3XX 145.725 −0.6 tone 77.0\nGB3YY 433.200 +1.6 tone 82.5\nGB3ZZ 145.6625 −0.6 tone 71.9\n\n" +
            "Map attached. See you on Sunday.\n\n73, Dave",
        "20:31", attachment = "map.jpg · 3.8 KB",
    ),
    Mail(13, MailFolder.Inbox, "mum@example.com", "Re: back Sunday", "Safe travels, text when you're down.", "Wed"),
    Mail(14, MailFolder.Inbox, "M0ABC", "Net times", "Thursday 20:00 on 145.500, simplex.", "Mon"),
    Mail(15, MailFolder.Sent, "sam@example.com", "Weekend plans", "Are we still on for Saturday?", "Fri"),
)

private fun sampleOnAir() = listOf(
    OnAirEvent("21:33", "#bulletin · 2E0XYZ", "heard · 1 dB", false),
    OnAirEvent("21:32", "Receipt from M0ABC", "heard · 2 dB", false),
    OnAirEvent("21:31", "#bulletin · you", "sent", true),
    OnAirEvent("21:27", "Mail header to sam@…", "sent", true),
)
