import Foundation

/// `Core`-table lookup for the outbox progress strings.
private func coreMessage(_ key: String) -> String {
    NSLocalizedString(key, tableName: "Core", bundle: .main, comment: "")
}

extension BridgeOutboxSnapshot {
    /// Bytes transferred of the files uploading right now, e.g. "1.2 GB of 3.4
    /// GB". The denominator is the active files' size (`activeBytesTotal`), not
    /// the whole backlog — a large queue shouldn't make the live transfer read
    /// as barely started. Empty when nothing is uploading.
    var transferProgressText: String {
        guard activeBytesTotal > 0 else { return "" }
        return String(
            format: coreMessage("core.outbox.bytes_progress"),
            Int64(total.bytesDone).formatted(.byteCount(style: .file)),
            Int64(activeBytesTotal).formatted(.byteCount(style: .file))
        )
    }

    /// The live transfer as a 0...1 fraction for the master progress bar. Zero
    /// (an empty bar) when nothing is uploading.
    var transferFraction: Double {
        guard activeBytesTotal > 0 else { return 0 }
        return Double(total.bytesDone) / Double(activeBytesTotal)
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
