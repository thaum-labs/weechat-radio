package com.weechatradio.app.radio

import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothSocket
import android.content.Context
import android.util.Log
import java.io.Closeable
import java.io.InputStream
import java.io.OutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.UUID
import kotlin.concurrent.thread

/**
 * The radio's own Bluetooth audio channel ("BS AOC"), served on a loopback
 * port for the wcr core. Android has no raw Bluetooth sockets, so the app
 * opens the channel and the core's audio bridge dials this port; bytes pass
 * through unchanged.
 *
 * Each time the core dials in, the radio is opened afresh. The first byte
 * back says how it went: 1 = linked, 0 = the radio did not answer (the core
 * then shows "not answering" and tries again in 5 s).
 */
class RadioLink(context: Context) {
    private val adapter = context.getSystemService(BluetoothManager::class.java)?.adapter
    private val server = ServerSocket(0, 1, InetAddress.getLoopbackAddress())

    /** The loopback port the core dials. */
    val port: Int get() = server.localPort

    /** The radio this link opens, from the paired devices; null if none is paired. */
    @Volatile var radio: BluetoothDevice? = null
        private set

    fun start() {
        thread(name = "radio-link", isDaemon = true) {
            while (true) {
                val core = runCatching { server.accept() }.getOrNull() ?: break
                runCatching { serve(core) }.onFailure { Log.w(TAG, "link ended: $it") }
            }
        }
    }

    /** Paired radios, best match first. */
    @SuppressLint("MissingPermission")
    fun pairedRadios(): List<BluetoothDevice> = runCatching {
        adapter?.bondedDevices.orEmpty()
            .filter { d -> RADIO_NAMES.any { d.name.orEmpty().contains(it, ignoreCase = true) } }
            .sortedBy { d -> RADIO_NAMES.indexOfFirst { d.name.orEmpty().contains(it, ignoreCase = true) } }
    }.getOrDefault(emptyList())

    @SuppressLint("MissingPermission")
    private fun serve(core: Socket) {
        core.tcpNoDelay = true
        val dev = pairedRadios().firstOrNull()
        radio = dev
        val bt: BluetoothSocket? = dev?.let { d ->
            adapter?.cancelDiscovery()
            // By the channel's UUID; if the radio's service list does not answer,
            // straight to channel 2, where the desktop app finds it on these radios.
            open(d, "uuid") { d.createInsecureRfcommSocketToServiceRecord(AUDIO_CHANNEL) }
                ?: open(d, "channel $FALLBACK_CHANNEL") {
                    d.javaClass.getMethod("createInsecureRfcommSocket", Int::class.javaPrimitiveType)
                        .invoke(d, FALLBACK_CHANNEL) as BluetoothSocket
                }
        }
        if (bt == null) {
            runCatching { core.getOutputStream().write(0) }
            core.close()
            return
        }
        core.getOutputStream().apply { write(1); flush() }
        Log.i(TAG, "linked to ${dev.name}")
        val both = listOf<Closeable>(core, bt)
        pump(bt.inputStream, core.getOutputStream(), both)
        pump(core.getInputStream(), bt.outputStream, both)
    }

    @SuppressLint("MissingPermission")
    private fun open(d: BluetoothDevice, how: String, make: () -> BluetoothSocket): BluetoothSocket? =
        runCatching { make().also { it.connect() } }
            .onFailure { Log.w(TAG, "${d.name} audio channel by $how: $it") }
            .getOrNull()

    /** Copy one way until either side closes, then close both. */
    private fun pump(from: InputStream, to: OutputStream, both: List<Closeable>) {
        thread(name = "radio-pump", isDaemon = true) {
            val buf = ByteArray(4096)
            try {
                while (true) {
                    val n = from.read(buf)
                    if (n < 0) break
                    to.write(buf, 0, n)
                    to.flush()
                }
            } catch (_: Exception) {
            } finally {
                both.forEach { runCatching { it.close() } }
            }
        }
    }

    companion object {
        private const val TAG = "RadioLink"
        /** The radio's audio channel (wcr btaudio, the same UUID on every Benshi radio). */
        val AUDIO_CHANNEL: UUID = UUID.fromString("39144315-32fa-40db-85ed-fbfeba2d86e6")
        private const val FALLBACK_CHANNEL = 2
        /** Radios that carry audio over this channel, as the desktop app lists them. */
        val RADIO_NAMES = listOf("VR-N76", "VR-N7600", "VR-N7500", "UV-PRO", "UV-50PRO", "GA-5WB", "RT-660")
    }
}
