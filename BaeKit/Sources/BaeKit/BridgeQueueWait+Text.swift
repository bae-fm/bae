import Foundation

extension BridgeQueueWait {
    /// What holds the queue back, in words ("Paused", "Waiting for network"),
    /// from core's catalog.
    public var text: String {
        localizedCoreString(bridgeQueueWaitKey(wait: self))
    }
}

extension BridgeUploadQueueWait {
    /// What holds the upload queue back, in words ("Paused", "Waiting for
    /// Wi-Fi", "Retrying at 6:40 PM"), from core's catalog. The retry's clock
    /// time is in the person's locale.
    public var text: String {
        let line = localizedCoreString(bridgeUploadQueueWaitKey(wait: self))
        guard case .retrying(let atMs) = self else {
            return line
        }
        return String(
            format: line,
            locale: Locale.current,
            Date(timeIntervalSince1970: Double(atMs) / 1000)
                .formatted(date: .omitted, time: .shortened)
        )
    }
}
