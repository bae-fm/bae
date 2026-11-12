package fm.bae.app.data

import uniffi.bae_bridge.BridgePendingAction
import uniffi.bae_bridge.BridgePendingRecord

/**
 * A phone sends no release files or covers and can't restore a source file, so core never gives it a record of
 * either. [take] keeps the records a phone can have and reports the rest as a bug, once for as long as the same ones
 * keep arriving: a bug is not a row, so no view draws them.
 */
class PhonePendingRecords {
    private var reported = emptyList<BridgePendingRecord>()

    /** What [take] found in one list: the records to draw, and the ones only a Mac can have. */
    class Taken(
        val listed: List<BridgePendingRecord>,
        val onlyAMacHas: List<BridgePendingRecord>,
    )

    fun take(
        records: List<BridgePendingRecord>,
        operation: String,
        report: (operation: String, error: Throwable) -> Unit,
    ): Taken {
        val (bad, listed) = records.partition { it.sendsFiles || it.action == BridgePendingAction.RestoreFile }
        if (bad != reported) {
            if (bad.isNotEmpty()) report(operation, MacRecordsOnAPhone(bad))
            reported = bad
        }
        return Taken(listed, bad)
    }
}

/** Records only a Mac can have, which a phone was sent. */
class MacRecordsOnAPhone(
    records: List<BridgePendingRecord>,
) : IllegalStateException("a phone has pending records only a Mac can have: $records")
