package com.macro.call

import android.Manifest
import android.app.Activity
import android.app.PictureInPictureParams
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.telecom.CallAudioState
import android.util.Rational
import android.view.View
import android.view.WindowManager
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import io.livekit.android.renderer.SurfaceViewRenderer
import io.livekit.android.room.track.Track
import io.livekit.android.room.track.VideoTrack
import kotlinx.coroutines.launch

/** Native media UI also owns PiP: navigating/reloading the WebView never owns the room. */
class CallActivity : Activity() {
    private lateinit var layout: LinearLayout
    private val renderers = mutableListOf<Pair<VideoTrack, SurfaceViewRenderer>>()
    private var permissionAction: (() -> Unit)? = null
    private var rendered: List<Any?>? = null
    private val callback: () -> Unit = { render() }
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= 27) { setShowWhenLocked(true); setTurnScreenOn(true) }
        else window.addFlags(WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED or WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON)
        layout = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        ViewCompat.setOnApplyWindowInsetsListener(layout) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            insets
        }
        setContentView(layout)
        Calls.changed = callback
        intent.getStringExtra("answer")?.let { id ->
            permission(Manifest.permission.RECORD_AUDIO) { Calls.answer(this, id) }
        }
        render()
    }
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        intent.getStringExtra("answer")?.let { id -> permission(Manifest.permission.RECORD_AUDIO) { Calls.answer(this, id) } }
        render()
    }
    private fun permission(name: String, action: () -> Unit) {
        if (checkSelfPermission(name) == PackageManager.PERMISSION_GRANTED) action()
        else { permissionAction = action; requestPermissions(arrayOf(name), 4) }
    }
    override fun onRequestPermissionsResult(code: Int, permissions: Array<out String>, grants: IntArray) {
        super.onRequestPermissionsResult(code, permissions, grants)
        val action = permissionAction; permissionAction = null
        if (code == 4 && grants.firstOrNull() == PackageManager.PERMISSION_GRANTED) action?.invoke()
        else { Calls.error = "Permission denied. Enable it in Android settings to use this control."; render() }
    }
    private fun button(text: String, action: () -> Unit) { layout.addView(Button(this).apply { this.text = text; setOnClickListener { action() } }) }
    private fun detach() { renderers.forEach { (track, renderer) -> track.removeRenderer(renderer); renderer.release() }; renderers.clear() }
    private fun render() {
        val call = Calls.offer ?: run { finish(); return }
        val media = Calls.room
        val tracks = media?.remoteParticipants?.values?.flatMap { it.trackPublications.values.mapNotNull { pub -> pub.track as? VideoTrack } }.orEmpty() +
            listOfNotNull(media?.localParticipant?.getTrackPublication(Track.Source.CAMERA)?.track as? VideoTrack)
        val nextRender = listOf(call.callId, Calls.title, Calls.state, Calls.error, Calls.muted, Calls.video, isInPictureInPictureMode, tracks.take(4))
        if (rendered == nextRender) return
        rendered = nextRender
        detach(); layout.removeAllViews()
        layout.addView(TextView(this).apply { text = "${Calls.title ?: call.title} · ${Calls.state}" })
        Calls.error?.let { message -> layout.addView(TextView(this).apply { text = message }) }
        tracks.take(4).forEach { track ->
            val renderer = SurfaceViewRenderer(this)
            media?.initVideoRenderer(renderer); track.addRenderer(renderer)
            renderers.add(track to renderer)
            layout.addView(renderer, LinearLayout.LayoutParams(-1, 0, 1f))
        }
        if (!isInPictureInPictureMode) {
            if (media == null) button("Answer") { permission(Manifest.permission.RECORD_AUDIO) { Calls.answer(this, call.callId) } }
            else {
                button(if (Calls.muted) "Unmute" else "Mute") { Calls.scope.launch { runCatching { Calls.microphone(Calls.muted) }.onFailure { Calls.error = "Could not change microphone"; render() } } }
                button(if (Calls.video) "Camera off" else "Camera on") { permission(Manifest.permission.CAMERA) { Calls.scope.launch { runCatching { Calls.camera(this@CallActivity, !Calls.video) }.onFailure { Calls.error = "Could not change camera"; render() } } } }
                button("Switch camera") { Calls.switchCamera() }
                button("Earpiece") { Calls.route(CallAudioState.ROUTE_EARPIECE) }
                button("Speaker") { Calls.route(CallAudioState.ROUTE_SPEAKER) }
                button("Headset") { Calls.route(CallAudioState.ROUTE_WIRED_HEADSET) }
                button("Bluetooth") { if (Build.VERSION.SDK_INT >= 31) permission(Manifest.permission.BLUETOOTH_CONNECT) { Calls.route(CallAudioState.ROUTE_BLUETOOTH) } else Calls.route(CallAudioState.ROUTE_BLUETOOTH) }
                if (packageManager.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE)) button("Picture in picture") { pip() }
                button("Open Macro") { packageManager.getLaunchIntentForPackage(packageName)?.let { startActivity(it) } }
            }
            button(if (media == null) "Decline" else "End call") { Calls.end(this, call.callId) }
        }
    }
    private fun pip() { if (Calls.room != null && packageManager.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE)) enterPictureInPictureMode(PictureInPictureParams.Builder().setAspectRatio(Rational(16, 9)).build()) }
    override fun onUserLeaveHint() { pip() }
    override fun onPictureInPictureModeChanged(inPip: Boolean, config: android.content.res.Configuration) { super.onPictureInPictureModeChanged(inPip, config); render() }
    override fun onStop() {
        super.onStop()
        // Camera may continue in supported PiP; ordinary background keeps audio only.
        if (!isInPictureInPictureMode && Calls.video) Calls.scope.launch { runCatching { Calls.camera(this@CallActivity, false) } }
    }
    override fun onDestroy() { if (Calls.changed === callback) Calls.changed = null; detach(); super.onDestroy() }
}
