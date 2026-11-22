package com.weechatradio.app.radio

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import com.weechatradio.app.MainActivity
import com.weechatradio.app.R

/**
 * Keeps the station listening with the screen off (design 22). Android needs a
 * foreground service and a notification for that: "weechat-radio listening on
 * 433.625" with Open, Mute and Stop. On by default; Settings → Listen in
 * background turns it off. Phase 0 moves the radio audio in here.
 */
class ListeningService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val radio = Station.radio
        when (intent?.action) {
            ACTION_STOP -> {
                radio.update { it.copy(listening = false) }
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_MUTE -> radio.update { it.copy(muted = !it.muted) }
        }
        val note = notification(this, radio.status)
        try {
            if (Build.VERSION.SDK_INT >= 29) startForeground(ID, note, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
            else startForeground(ID, note)
        } catch (e: Exception) {
            // Android refused (e.g. started from the background): stay quiet rather than crash.
            stopSelf()
        }
        return START_STICKY
    }

    companion object {
        private const val ID = 1
        private const val CHANNEL = "listening"
        private const val ACTION_MUTE = "com.weechatradio.app.MUTE"
        private const val ACTION_STOP = "com.weechatradio.app.STOP"

        fun start(context: Context) {
            runCatching { context.startForegroundService(Intent(context, ListeningService::class.java)) }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, ListeningService::class.java))
        }

        private fun notification(context: Context, s: Status): Notification {
            val manager = context.getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(
                NotificationChannel(CHANNEL, "Background listening", NotificationManager.IMPORTANCE_LOW).apply {
                    description = "Shown while the station listens with the screen off"
                },
            )
            val flags = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
            val open = PendingIntent.getActivity(context, 0, Intent(context, MainActivity::class.java), flags)
            fun action(name: String, req: Int) =
                PendingIntent.getService(context, req, Intent(context, ListeningService::class.java).setAction(name), flags)
            val where = if (s.linked) "${s.rig.substringBefore(' ')} · ${s.preset} · RX" else "internet only"
            return Notification.Builder(context, CHANNEL)
                .setSmallIcon(R.drawable.ic_stat_listening)
                .setContentTitle("weechat-radio listening on ${s.freq}")
                .setContentText("${s.call} · $where. Keeps listening with the screen off and uses battery.")
                .setStyle(Notification.BigTextStyle().bigText("${s.call} · $where. Keeps listening with the screen off and uses battery."))
                .setContentIntent(open)
                .setOngoing(true)
                .addAction(Notification.Action.Builder(null, "Open", open).build())
                .addAction(Notification.Action.Builder(null, if (s.muted) "Unmute" else "Mute", action(ACTION_MUTE, 1)).build())
                .addAction(Notification.Action.Builder(null, "Stop", action(ACTION_STOP, 2)).build())
                .build()
        }
    }
}
