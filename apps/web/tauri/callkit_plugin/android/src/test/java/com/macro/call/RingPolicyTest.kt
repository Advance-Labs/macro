package com.macro.call

import org.junit.Assert.*
import org.junit.Test

class RingPolicyTest {
    @Test fun expiresDelayedPushesAndRejectsFutureTimestamps() {
        assertEquals(RingPolicy.Decision.RESOLVE, RingPolicy.decide(1_000, 61_001, "ringing"))
        assertEquals(RingPolicy.Decision.RESOLVE, RingPolicy.decide(2_000, 1_000, "ringing"))
        assertEquals(RingPolicy.Decision.RESOLVE, RingPolicy.decide(0, 1_000, "ringing"))
    }
    @Test fun resolvesRemoteCancellationAndAnswersElsewhere() {
        for (status in listOf("ended", "answered", "invalid")) {
            assertEquals(RingPolicy.Decision.RESOLVE, RingPolicy.decide(1_000, 2_000, status))
        }
    }
    @Test fun transientFailuresCanRetryButCannotAuthorizeAnAnswer() {
        for (status in listOf(null, "unknown")) {
            assertEquals(RingPolicy.Decision.RETRY, RingPolicy.decide(1_000, 2_000, status))
        }
        assertEquals(RingPolicy.Decision.RING, RingPolicy.decide(1_000, 2_000, "ringing"))
        assertEquals(RingPolicy.Decision.RESOLVE, RingPolicy.decide(1_000, 61_001, null))
    }
}
