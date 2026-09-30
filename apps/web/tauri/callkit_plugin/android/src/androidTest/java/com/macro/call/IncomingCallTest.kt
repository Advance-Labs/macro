package com.macro.call

import android.content.Context
import android.app.NotificationManager
import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.util.UUID
import kotlinx.coroutines.CompletableDeferred

/** Exercises real Telecom/foreground service wiring without backend credentials. */
@RunWith(AndroidJUnit4::class)
class IncomingCallTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext

    @Before fun registerRecipient() {
        context.getSharedPreferences("macro_push", Context.MODE_PRIVATE).edit().putString("recipient", "call-test-user").commit()
        if (Build.VERSION.SDK_INT >= 33) {
            instrumentation.uiAutomation.executeShellCommand("pm grant ${context.packageName} android.permission.POST_NOTIFICATIONS").close()
        }
        instrumentation.runOnMainSync { Calls.end(context) }
    }

    @After fun cleanup() {
        instrumentation.runOnMainSync { Calls.end(context) }
        context.getSharedPreferences("macro_push", Context.MODE_PRIVATE).edit().clear().commit()
    }

    private fun data(id: String, recipient: String = "call-test-user") = mapOf(
        "recipientId" to recipient,
        "payload" to JSONObject().apply {
            put("callId", id)
            put("channelId", "call-test-channel")
            put("channelName", "Emulator call")
            put("livekitServerUrl", "wss://unused.invalid")
            put("livekitToken", "test-token")
            // An unavailable status endpoint must never authorize an answer.
            put("ringStatusUrl", "https://unavailable-call-test.macro.com/status")
        }.toString(),
    )

    @Test fun incomingTelecomConnectionDeclinesAndRejectsRedelivery() {
        val id = UUID.randomUUID().toString()
        instrumentation.runOnMainSync { Calls.receive(context, data(id), System.currentTimeMillis()) }
        val deadline = System.currentTimeMillis() + 10_000
        while (System.currentTimeMillis() < deadline) {
            var connected = false
            instrumentation.runOnMainSync {
                connected = Calls.connection != null && context.getSystemService(NotificationManager::class.java)
                    .activeNotifications.any { it.id == CallService.NOTIFICATION_ID }
            }
            if (connected) break
            Thread.sleep(100)
        }
        instrumentation.runOnMainSync {
            assertEquals(id, Calls.offer?.callId)
            assertNotNull("Telecom must create the native connection", Calls.connection)
            val notification = context.getSystemService(NotificationManager::class.java)
                .activeNotifications.single { it.id == CallService.NOTIFICATION_ID }.notification
            assertTrue("Incoming notification must expose answer/decline", notification.actions.size >= 2)
            assertNull("Ringing must not start a media room", Calls.room)
            assertNull("Ringing must not restore an active web session", Calls.snapshot())
            Calls.connection!!.onReject()
            assertNull(Calls.offer)
            assertNull(Calls.connection)
            Calls.receive(context, data(id), System.currentTimeMillis())
            assertNull("Declined calls must not ring again", Calls.offer)
        }
    }

    @Test fun ignoresStaleAndOtherAccountOffers() {
        instrumentation.runOnMainSync {
            Calls.receive(context, data(UUID.randomUUID().toString(), "another-account"), System.currentTimeMillis())
            assertNull(Calls.offer)
            Calls.receive(context, data(UUID.randomUUID().toString()), System.currentTimeMillis() - 61_000)
            assertNull(Calls.offer)
            assertNull(Calls.room)
        }
    }

    @Test fun incomingWhileScreenOffKeepsNotificationActions() {
        instrumentation.uiAutomation.executeShellCommand("input keyevent 223").close()
        try {
            incomingTelecomConnectionDeclinesAndRejectsRedelivery()
        } finally {
            instrumentation.uiAutomation.executeShellCommand("input keyevent 224").close()
        }
    }

    @Test fun obsoleteAnswerCompletionCannotUnlockAnotherCallsAnswer() {
        val oldId = UUID.randomUUID().toString()
        val newId = UUID.randomUUID().toString()
        val oldStatus = CompletableDeferred<String?>()
        val newStatus = CompletableDeferred<String?>()
        var checks = 0
        try {
            instrumentation.runOnMainSync {
                Calls.receive(context, data(oldId), System.currentTimeMillis())
                Calls.answer(context, oldId) { oldStatus.await() }
                Calls.end(context, oldId)
            }
            instrumentation.waitForIdleSync()
            instrumentation.runOnMainSync {
                Calls.receive(context, data(newId), System.currentTimeMillis())
                assertEquals(newId, Calls.offer?.callId)
                Calls.answer(context, newId) { checks++; newStatus.await() }
                oldStatus.complete("ringing")
                Calls.answer(context, newId) { checks++; "answered" }
                assertEquals("Old verification must not allow a duplicate answer check", 1, checks)
                assertEquals(newId, Calls.offer?.callId)
                assertNull(Calls.room)
                newStatus.complete("ended")
                assertNull(Calls.offer)
            }
        } finally {
            oldStatus.complete("ended"); newStatus.complete("ended")
        }
    }
}
