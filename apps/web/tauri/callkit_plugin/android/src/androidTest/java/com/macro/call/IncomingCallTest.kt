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

    @Test fun telecomTeardownFailureStillRemovesNotificationAndAllowsAnotherCall() {
        val id = UUID.randomUUID().toString()
        val manager = context.getSystemService(NotificationManager::class.java)
        instrumentation.runOnMainSync { Calls.receive(context, data(id), System.currentTimeMillis()) }
        fun awaitNotification(expected: Boolean) {
            val deadline = System.currentTimeMillis() + 10_000
            while (System.currentTimeMillis() < deadline) {
                if (manager.activeNotifications.any { it.id == CallService.NOTIFICATION_ID } == expected) return
                Thread.sleep(100)
            }
            fail("Call notification visibility did not become $expected")
        }
        awaitNotification(true)
        var injected = false
        instrumentation.runOnMainSync {
            assertNotNull(Calls.connection)
            Calls.end(context, id) { injected = true; throw IllegalStateException("Simulated Telecom callback failure") }
            assertTrue("Fault must occur during actual Telecom teardown", injected)
            assertNull(Calls.offer)
            assertNull(Calls.connection)
            assertNull(Calls.snapshot())
        }
        awaitNotification(false)
        instrumentation.waitForIdleSync()
        val next = UUID.randomUUID().toString()
        instrumentation.runOnMainSync { Calls.receive(context, data(next), System.currentTimeMillis()) }
        awaitNotification(true)
        instrumentation.runOnMainSync {
            assertEquals("Failed cleanup must not prevent the next incoming call", next, Calls.offer?.callId)
            assertNotNull(Calls.connection)
            Calls.end(context, next)
        }
        awaitNotification(false)
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

    @Test fun appJoinAdoptsRingingCallWithoutTreatingItsMembershipAsRemoteAnswer() {
        instrumentation.uiAutomation.executeShellCommand("pm grant ${context.packageName} android.permission.RECORD_AUDIO").close()
        val id = UUID.randomUUID().toString()
        val status = CompletableDeferred<String?>()
        lateinit var lease: String
        instrumentation.runOnMainSync {
            Calls.receive(context, data(id), System.currentTimeMillis())
            Calls.answer(context, id) { status.await() }
            lease = Calls.prepareJoin(context, "call-test-channel")
        }
        val deadline = System.currentTimeMillis() + 10_000
        while (System.currentTimeMillis() < deadline) {
            var ready = false
            instrumentation.runOnMainSync { ready = Calls.connection != null }
            if (ready) break
            Thread.sleep(100)
        }
        instrumentation.runOnMainSync {
            val connection = Calls.connection
            assertNotNull("Telecom must create the ringing connection before app adoption", connection)
            // The join API and its new token make the old verifier report answered.
            status.complete("answered")
            assertEquals(id, Calls.offer?.callId)
            Calls.outgoing(context, CallOffer(id, "call-test-channel", "Joined call", "wss://unused.invalid", "new-join-token"), lease)
            assertEquals(id, Calls.offer?.callId)
            assertEquals("new-join-token", Calls.offer?.token)
            assertSame("App Join must reuse the existing Telecom connection", connection, Calls.connection)
            assertNotNull("App Join must start native media instead of declining its own answer", Calls.room)
            assertNotNull(Calls.snapshot())
            Calls.abortJoin(context, lease)
            assertEquals("Committed cleanup must preserve the media session", id, Calls.offer?.callId)
            Calls.end(context, id)
        }
    }

    @Test fun incomingDuringPendingAppJoinCannotBeResolvedByOldRingPolling() {
        val id = UUID.randomUUID().toString()
        instrumentation.runOnMainSync {
            val lease = Calls.prepareJoin(context, "call-test-channel")
            Calls.receive(context, data(id), System.currentTimeMillis())
            Calls.answer(context, id) { "answered" }
            assertEquals(id, Calls.offer?.callId)
            Calls.abortJoin(context, lease)
            assertNull("Cancelling a pending join must release its arriving ring", Calls.offer)
            assertThrows(IllegalStateException::class.java) {
                Calls.outgoing(context, CallOffer(id, "call-test-channel", "Cancelled", "wss://unused.invalid", "token"), lease)
            }
            assertNull(Calls.room)
        }
    }

    @Test fun accountResetAbortsOnlyThatAccountsPendingJoin() {
        instrumentation.runOnMainSync {
            val oldLease = Calls.prepareJoin(context, "call-test-channel")
            Calls.resetRecipient(context, "call-test-user")
            assertThrows(IllegalStateException::class.java) {
                Calls.outgoing(context, CallOffer(UUID.randomUUID().toString(), "call-test-channel", "Old account", "wss://unused.invalid", "token"), oldLease)
            }
            context.getSharedPreferences("macro_push", Context.MODE_PRIVATE).edit().putString("recipient", "new-account").commit()
            val newLease = Calls.prepareJoin(context, "call-test-channel")
            Calls.resetRecipient(context, "call-test-user")
            Calls.receive(context, data(UUID.randomUUID().toString(), "new-account"), System.currentTimeMillis())
            assertNotNull("Old account's late reset must not abort a new lease", Calls.offer)
            Calls.abortJoin(context, newLease)
            assertNull(Calls.offer)
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
