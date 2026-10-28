import Foundation

extension BridgeUploadActivity {
    /// The phase alone ("Uploading"), for a line that is one release's own.
    public var phaseText: String {
        localizedCoreString(bridgeUploadActivityKey(activity: self))
    }
}

extension BridgeUploadProgress {
    /// The phase core chose for files it holds: uploading while any file
    /// moves bytes, queued otherwise. Only an idle total has none.
    public var heldActivity: BridgeUploadActivity {
        guard let activity else {
            preconditionFailure(
                "an active cloud upload has no projected activity"
            )
        }
        return activity
    }

}

extension BridgeUploadBar {
    /// How far the encrypted bytes have come, as a 0...1 fraction.
    public var fraction: Double {
        guard bytesTotal > 0 else { return 0 }
        precondition(
            bytesDone <= bytesTotal,
            "cloud upload progress cannot exceed its exact total"
        )
        return Double(bytesDone) / Double(bytesTotal)
    }

    /// "3 MB of 221.2 MB", read off the bar itself so the text and the fill
    /// measure the same thing.
    public var text: String {
        formattedDownloadBytesProgress(done: bytesDone, total: bytesTotal)
    }
}
