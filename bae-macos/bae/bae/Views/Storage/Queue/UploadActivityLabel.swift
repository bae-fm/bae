import BaeKit
import SwiftUI

/// Core's current cloud-upload phase as a chip, the same on every screen.
struct UploadActivityLabel: View {
    private let activity: BridgeUploadActivity
    private let text: String

    /// The phase alone ("Uploading"), on a line that is one release's own.
    init(phaseOf progress: BridgeUploadProgress) {
        activity = progress.heldActivity
        text = activity.phaseText
    }

    /// The phase with how many files are in it ("2 uploading"), above the
    /// list of those files.
    init(fileCountOf progress: BridgeUploadProgress) {
        activity = progress.heldActivity
        text = progress.fileCountText
    }

    var body: some View {
        StatusChip(
            verbatim: text,
            tone: activity.tone,
            symbol: activity.systemImage
        )
    }
}

extension BridgeUploadActivity {
    fileprivate var systemImage: String {
        switch self {
        case .uploading: "arrow.up.circle.fill"
        case .queued: "clock"
        }
    }

    fileprivate var tone: StatusTone {
        switch self {
        case .uploading: .activity
        case .queued: .neutral
        }
    }
}
