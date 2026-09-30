package com.macro.call

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.util.Base64
import io.livekit.android.LiveKit
import io.livekit.android.room.track.VideoTrack
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.launch
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeNotNull
import org.junit.Test
import org.junit.runner.RunWith
import java.util.UUID
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

/** Optional integration test against LiveKit's isolated --dev server. */
@RunWith(AndroidJUnit4::class)
class MediaLifecycleTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext

    private fun token(identity: String = "android-emulator-test"): String {
        fun encode(bytes: ByteArray) = Base64.encodeToString(bytes, Base64.URL_SAFE or Base64.NO_PADDING or Base64.NO_WRAP)
        val header = encode("{\"alg\":\"HS256\",\"typ\":\"JWT\"}".toByteArray())
        val payload = encode(JSONObject().apply {
            put("iss", "devkey"); put("sub", identity)
            put("exp", System.currentTimeMillis() / 1000 + 300)
            put("video", JSONObject().apply { put("roomJoin", true); put("room", "android-call-test"); put("canPublish", true); put("canSubscribe", true) })
        }.toString().toByteArray())
        val body = "$header.$payload"
        val mac = Mac.getInstance("HmacSHA256").apply { init(SecretKeySpec("secret".toByteArray(), "HmacSHA256")) }
        return "$body.${encode(mac.doFinal(body.toByteArray()))}"
    }

    private fun awaitCondition(message: String, condition: () -> Boolean) {
        val deadline = System.currentTimeMillis() + 30_000
        while (System.currentTimeMillis() < deadline) {
            var ready = false
            instrumentation.runOnMainSync { ready = condition() }
            if (ready) return
            Thread.sleep(100)
        }
        fail(message)
    }

    @Test fun nativeRoomSurvivesDuplicateStartAndReleasesCapture() {
        val url = InstrumentationRegistry.getArguments().getString("livekitUrl")
        assumeNotNull(url)
        for (permission in listOf(Manifest.permission.RECORD_AUDIO, Manifest.permission.CAMERA)) {
            instrumentation.uiAutomation.executeShellCommand("pm grant ${context.packageName} $permission").close()
            awaitCondition("Capture permission was not granted") { context.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED }
        }
        val offer = CallOffer(UUID.randomUUID().toString(), "media-test", "Emulator media", url!!, token())
        val observer = LiveKit.create(context)
        val monitor = instrumentation.addMonitor(CallActivity::class.java.name, null, false)
        try {
            instrumentation.runOnMainSync {
                Calls.outgoing(context, offer)
                context.startActivity(Intent(context, CallActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            }
            val activity = instrumentation.waitForMonitorWithTimeout(monitor, 5000)
            assertNotNull("Native call controls did not open", activity)
            awaitCondition("Native room did not connect") { Calls.state == "connected" }
            instrumentation.runOnMainSync { Calls.scope.launch { observer.connect(url, token("media-observer")) } }
            awaitCondition("Remote participant did not join") { Calls.room?.remoteParticipants?.isNotEmpty() == true }
            instrumentation.runOnMainSync {
                val room = Calls.room
                assertNotNull(room)
                Calls.outgoing(context, offer)
                assertSame("Duplicate start must retain the media room", room, Calls.room)
                Calls.scope.launch { Calls.microphone(false) }
            }
            awaitCondition("Native microphone did not mute") { Calls.muted }
            instrumentation.runOnMainSync { Calls.scope.launch { Calls.camera(context, true) } }
            awaitCondition("Native camera did not start") { Calls.video }
            awaitCondition("Remote participant did not receive video") {
                observer.remoteParticipants.values.any { participant -> participant.trackPublications.values.any { it.track is VideoTrack } }
            }
            instrumentation.runOnMainSync { Calls.scope.launch { Calls.camera(context, false) } }
            awaitCondition("Native camera did not stop") { !Calls.video }
            instrumentation.uiAutomation.executeShellCommand("svc wifi disable").close()
            instrumentation.uiAutomation.executeShellCommand("svc data disable").close()
            try {
                awaitCondition("Native room did not report network interruption") { Calls.state == "reconnecting" }
            } finally {
                instrumentation.uiAutomation.executeShellCommand("svc wifi enable").close()
                instrumentation.uiAutomation.executeShellCommand("svc data enable").close()
            }
            awaitCondition("Native room did not reconnect") { Calls.state == "connected" }
            instrumentation.uiAutomation.executeShellCommand("input keyevent 3").close()
            awaitCondition("Call controls did not enter picture in picture") { activity!!.isInPictureInPictureMode }
            instrumentation.runOnMainSync {
                assertEquals("connected", Calls.state)
                assertNotNull("Navigation must retain the media room", Calls.room)
                Calls.end(context, offer.callId)
                assertNull(Calls.room)
                assertNull(Calls.connection)
                assertNull(Calls.snapshot())
            }
        } finally {
            instrumentation.uiAutomation.executeShellCommand("svc wifi enable").close()
            instrumentation.uiAutomation.executeShellCommand("svc data enable").close()
            instrumentation.removeMonitor(monitor)
            instrumentation.runOnMainSync { observer.disconnect(); observer.release(); Calls.end(context, offer.callId) }
        }
    }
}
