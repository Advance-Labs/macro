package com.macro.call

/** Decisions shared by push receipt, polling and answer revalidation. */
internal object RingPolicy {
    enum class Decision { RING, RESOLVE, RETRY }
    fun fresh(sentTime: Long, now: Long): Boolean = sentTime > 0 && now - sentTime in 0..60_000
    fun decide(sentTime: Long, now: Long, status: String?): Decision = when {
        !fresh(sentTime, now) -> Decision.RESOLVE
        status in listOf("answered", "ended", "invalid") -> Decision.RESOLVE
        status == "ringing" -> Decision.RING
        else -> Decision.RETRY
    }
}
