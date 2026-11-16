import Foundation

extension BridgeUploadActivity {
    /// The phase alone ("Uploading"), for a line that is one release's own.
    public var phaseText: String {
        localizedCoreString(bridgeUploadActivityKey(activity: self))
    }
}

extension BridgeUploadBar {
    /// "3 MB of 221.2 MB", read off the bar itself so the text and the fill
    /// measure the same thing.
    public var text: String {
        formattedDownloadBytesProgress(done: bytesDone, total: bytesTotal)
    }
}
