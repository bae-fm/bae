import Foundation

/// `Core`-table lookup for the outbox progress strings.
private func coreMessage(_ key: String) -> String {
    NSLocalizedString(key, tableName: "Core", bundle: .main, comment: "")
}

extension BridgeOutboxSnapshot {
    /// Aggregate bytes uploaded of the total, e.g. "1.2 GB of 14.4 GB". Empty
    /// when there's nothing to upload. The byte counts format per locale.
    var bytesProgressText: String {
        guard total.bytesTotal > 0 else { return "" }
        return String(
            format: coreMessage("core.outbox.bytes_progress"),
            Int64(total.bytesDone).formatted(.byteCount(style: .file)),
            Int64(total.bytesTotal).formatted(.byteCount(style: .file))
        )
    }

    /// Upload throughput, e.g. "5.2 MB/s". Empty when idle.
    var throughputText: String {
        guard throughputBps > 0 else { return "" }
        return String(
            format: coreMessage("core.outbox.throughput"),
            Int64(throughputBps).formatted(.byteCount(style: .file))
        )
    }

    /// Estimated time remaining, e.g. "2 min 14 sec remaining". Empty when not
    /// computable.
    var etaText: String {
        guard let eta = etaSeconds else { return "" }
        let duration = Duration.seconds(Int64(eta))
            .formatted(
                .units(
                    allowed: [.hours, .minutes, .seconds],
                    width: .abbreviated
                )
            )
        return String(format: coreMessage("core.outbox.eta"), duration)
    }
}
