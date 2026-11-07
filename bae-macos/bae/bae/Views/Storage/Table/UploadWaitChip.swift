import BaeKit
import SwiftUI

/// What keeps a release's waiting files from being sent.
struct UploadWaitChip: View {
    let wait: BridgeUploadWait

    var body: some View {
        switch wait {
        case .original(let fileName):
            StatusChip("Waiting for \(fileName)", tone: .warning)
        case .paused:
            QueueWaitChip(wait: .paused)
        case .offline:
            QueueWaitChip(wait: .offline)
        case .metered:
            StatusChip("Waiting for Wi-Fi", tone: .warning)
        case .retrying(let atMs):
            StatusChip(
                "Retrying at \(Self.time(atMs: atMs))",
                tone: .warning
            )
        }
    }

    /// The retry's clock time, in the person's locale.
    private static func time(atMs: Int64) -> String {
        Date(timeIntervalSince1970: Double(atMs) / 1000)
            .formatted(date: .omitted, time: .shortened)
    }
}

#if DEBUG
    #Preview("Upload waits") {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            UploadWaitChip(wait: .original(fileName: "01 Track.flac"))
            UploadWaitChip(wait: .paused)
            UploadWaitChip(wait: .offline)
            UploadWaitChip(wait: .metered)
            UploadWaitChip(wait: .retrying(atMs: 1_800_000_000_000))
        }
        .padding()
    }
#endif
