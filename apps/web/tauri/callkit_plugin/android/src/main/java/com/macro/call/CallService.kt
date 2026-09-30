package com.macro.call

import android.app.*
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.Person
import androidx.core.content.ContextCompat
import kotlinx.coroutines.launch

/** Keeps the native room alive without requiring a running WebView. */
class CallService : Service() {
    companion object {
        const val NOTIFICATION_ID = 7404
        private const val CHANNEL = "macro_calls"
        fun start(ctx: Context, media: Boolean = false, camera: Boolean = false) {
            ContextCompat.startForegroundService(ctx, Intent(ctx, CallService::class.java)
                .putExtra("media", media).putExtra("camera", camera))
        }
        fun action(ctx: Context, name: String, id: String): PendingIntent = PendingIntent.getBroadcast(
            ctx, name.hashCode(), Intent(ctx, CallReceiver::class.java).setAction(name).putExtra("callId", id),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        fun screen(ctx: Context): PendingIntent = PendingIntent.getActivity(ctx, 0,
            Intent(ctx, CallActivity::class.java), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    }
    private var callId: String? = null
    private var mediaSession: android.media.session.MediaSession? = null
    override fun onCreate() {
        super.onCreate()
        mediaSession = android.media.session.MediaSession(this, "Macro call").apply {
            setCallback(object : android.media.session.MediaSession.Callback() {
                override fun onStop() { callId?.let { Calls.end(this@CallService, it) } }
                override fun onPlay() { Calls.scope.launch { runCatching { Calls.microphone(true) } } }
                override fun onPause() { Calls.scope.launch { runCatching { Calls.microphone(false) } } }
                override fun onMediaButtonEvent(intent: Intent): Boolean {
                    val event = intent.getParcelableExtra<android.view.KeyEvent>(Intent.EXTRA_KEY_EVENT) ?: return false
                    val supported = listOf(android.view.KeyEvent.KEYCODE_HEADSETHOOK, android.view.KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE,
                        android.view.KeyEvent.KEYCODE_MEDIA_PLAY, android.view.KeyEvent.KEYCODE_MEDIA_PAUSE, android.view.KeyEvent.KEYCODE_MEDIA_STOP)
                    if (event.keyCode !in supported) return false
                    if (event.action != android.view.KeyEvent.ACTION_UP) return true
                    val id = callId ?: return true
                    when (event.keyCode) {
                        android.view.KeyEvent.KEYCODE_MEDIA_STOP -> Calls.end(this@CallService, id)
                        android.view.KeyEvent.KEYCODE_MEDIA_PAUSE -> Calls.scope.launch { runCatching { Calls.microphone(false) } }
                        else -> if (Calls.room == null) Calls.answer(this@CallService, id)
                            else Calls.scope.launch { runCatching { Calls.microphone(event.keyCode == android.view.KeyEvent.KEYCODE_MEDIA_PLAY || Calls.muted) } }
                    }
                    return true
                }
            })
            setPlaybackState(android.media.session.PlaybackState.Builder()
                .setActions(android.media.session.PlaybackState.ACTION_PLAY or android.media.session.PlaybackState.ACTION_PAUSE or android.media.session.PlaybackState.ACTION_STOP)
                .setState(android.media.session.PlaybackState.STATE_PLAYING, 0, 1f).build())
            isActive = true
        }
    }
    override fun onBind(intent: Intent?): IBinder? = null
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val call = Calls.offer ?: run { stopSelf(); return START_NOT_STICKY }
        callId = call.callId
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL, "Calls", NotificationManager.IMPORTANCE_HIGH).apply {
            setSound(android.media.RingtoneManager.getDefaultUri(android.media.RingtoneManager.TYPE_RINGTONE),
                android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_NOTIFICATION_RINGTONE).build())
            enableVibration(true)
        })
        val person = Person.Builder().setName((Calls.title ?: call.title).ifBlank { "Macro call" }).setImportant(true).build()
        val ringing = Calls.room == null
        val notification = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.sym_call_incoming).setContentTitle(Calls.title ?: call.title)
            .setCategory(NotificationCompat.CATEGORY_CALL).setOngoing(true).setOnlyAlertOnce(true)
            .setSilent(!ringing)
            .setContentIntent(screen(this)).setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setStyle(if (ringing) NotificationCompat.CallStyle.forIncomingCall(person,
                action(this, "end", call.callId), action(this, "answer", call.callId))
                else NotificationCompat.CallStyle.forOngoingCall(person, action(this, "end", call.callId)))
            .setFullScreenIntent(if (ringing) screen(this) else null, ringing)
            .build().apply { if (ringing) this.flags = this.flags or Notification.FLAG_INSISTENT }
        var types = ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL
        if (intent?.getBooleanExtra("media", false) == true) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
        if (intent?.getBooleanExtra("camera", false) == true) types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA
        try {
            if (Build.VERSION.SDK_INT >= 29) startForeground(NOTIFICATION_ID, notification, types)
            else startForeground(NOTIFICATION_ID, notification)
        } catch (_: Exception) { Calls.end(this, call.callId); stopSelf() }
        return START_NOT_STICKY
    }
    override fun onDestroy() {
        mediaSession?.release(); mediaSession = null
        callId?.let { Calls.end(this, it) }
        super.onDestroy()
    }
}
