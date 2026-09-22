import BaeKit
import Foundation

extension BridgeCandidateAction {
    /// What the action is called. The name carries no count: a surface that
    /// states how many folders the action applies to draws that number of its
    /// own, beside the name.
    var label: String {
        switch self {
        case .importReady: String(localized: "Import ready")
        case .identify: String(localized: "Identify selected")
        case .retryIdentification:
            String(localized: "Retry failed identification")
        case .resetToFileMetadata: String(localized: "Reset to file metadata")
        case .clearMetadata: String(localized: "Clear metadata")
        case .skip: String(localized: "Skip selected")
        case .restore: String(localized: "Restore to Pending")
        }
    }

    /// The name with the number of folders it applies to, for a surface with
    /// nowhere to put that number separately: a menu item, a progress title,
    /// a confirmation's commit button.
    func label(count: Int) -> String {
        String(localized: "\(label) (\(count))")
    }

    var symbol: String {
        switch self {
        case .importReady: "square.and.arrow.down"
        case .identify: "magnifyingglass"
        case .retryIdentification: "arrow.clockwise"
        case .resetToFileMetadata: "doc.text"
        case .clearMetadata: "eraser"
        case .skip: "minus.circle"
        case .restore: "arrow.uturn.backward"
        }
    }
}
