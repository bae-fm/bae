import BaeKit
import SwiftUI

/// The dominant cloud-upload phase as core projected it. Every storage and
/// import surface uses this renderer so the same phase has one label, symbol,
/// and color throughout the app.
struct UploadActivityLabel: View {
    let progress: BridgeUploadProgress

    var body: some View {
        Label(text, systemImage: activity.systemImage)
            .foregroundStyle(activity.tint)
            .lineLimit(1)
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

    fileprivate var tint: Color {
        switch self {
        case .uploading, .preparing: .orange
        case .retrying: .red
        case .publishing, .uploaded: .blue
        case .cancelling, .prepared, .queued: .secondary
        }
    }
}
