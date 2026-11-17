package fm.bae.app.data

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgePendingAction
import uniffi.bae_bridge.BridgePendingReason
import uniffi.bae_bridge.BridgePendingRecord

class PhonePendingRecordsTest {
    private fun record(
        action: BridgePendingAction = BridgePendingAction.Wait,
        sendsFiles: Boolean = false,
    ) = BridgePendingRecord(
        subject = BridgeMessage(key = "core.pending.subject.cover", args = emptyList()),
        reason = BridgePendingReason(key = "core.pending.reason.storage", detail = null, causes = emptyList()),
        action = action,
        actionKey = null,
        sendsFiles = sendsFiles,
    )

    /** A record of files a phone sends, or of a source file to restore, is a bug: left out, and reported once. */
    @Test
    fun aRecordOnlyAMacCanHaveIsLeftOutAndReportedOnce() {
        val phones = PhonePendingRecords()
        val fine = record()
        val sends = record(sendsFiles = true)
        val restores = record(action = BridgePendingAction.RestoreFile)
        val reports = mutableListOf<Throwable>()
        val report = { _: String, error: Throwable -> reports += error }

        assertEquals(listOf(fine), phones.take(listOf(fine, sends, restores), "list", report).listed)
        phones.take(listOf(fine, sends, restores), "list", report)
        assertEquals(1, reports.size)

        phones.take(listOf(fine), "list", report)
        phones.take(listOf(sends), "list", report)
        assertEquals(2, reports.size)
    }
}
