import BaeKit
import SwiftUI

/// Core's current cloud-upload phase as a chip, the same on every screen.
struct UploadActivityLabel: View {
    let progress: BridgeUploadProgress

    var body: some View {
        StatusChip(
            verbatim: text,
            tone: activity.tone,
            symbol: activity.systemImage
        )
    }

    private var activity: BridgeUploadActivity {
        guard let activity = progress.activity else {
            preconditionFailure(
                "an active cloud upload has no projected activity"
            )
        }
        return activity
    }

    private var text: String {
        guard let text = progress.activityText else {
            preconditionFailure(
                "an active cloud upload has no projected activity label"
            )
        }
        return text
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
