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
        guard let text = progress.primaryActivityText else {
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
        case .cancelling: "xmark.circle"
        case .publishing: "arrow.triangle.2.circlepath"
        case .uploading: "arrow.up.circle.fill"
        case .preparing: "seal"
        case .retrying: "exclamationmark.triangle.fill"
        case .prepared: "checkmark.circle"
        case .queued: "clock"
        case .uploaded: "icloud"
        }
    }

    fileprivate var tone: StatusTone {
        switch self {
        case .uploading, .preparing: .activity
        case .retrying: .danger
        case .publishing, .uploaded: .info
        case .cancelling, .prepared, .queued: .neutral
        }
    }
}
