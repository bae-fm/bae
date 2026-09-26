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
        case .cancelIdentification:
            String(localized: "Stop identifying selected")
        case .cancelImport: String(localized: "Cancel import of selected")
        case .retryIdentification:
            String(localized: "Retry failed identification")
        case .resetToFileMetadata: String(localized: "Reset to file metadata")
        case .clearMetadata: String(localized: "Clear metadata")
        case .combine: String(localized: "Combine as One Release")
        case .separate: String(localized: "Keep as Separate Releases")
        case .skip: String(localized: "Skip selected")
        case .restore: String(localized: "Restore to Pending")
        case .revealFolder: String(localized: "Reveal in Finder")
        }
    }

    /// The name with the number of folders it applies to, for a surface with
    /// nowhere to put that number separately: a menu item, a progress title,
    /// a confirmation's commit button.
    func label(count: Int) -> String {
        String(localized: "\(label) (\(count))")
    }

    /// What the action is called in one row's menu, where it names that row's
    /// candidate rather than a selection.
    var rowLabel: String {
        switch self {
        case .importReady: String(localized: "Import")
        case .identify: String(localized: "Identify")
        case .cancelIdentification: String(localized: "Stop Identifying")
        case .cancelImport: String(localized: "Cancel Import")
        case .skip: String(localized: "Skip")
        case .restore: String(localized: "Unskip")
        case .retryIdentification, .resetToFileMetadata, .clearMetadata,
            .combine, .separate, .revealFolder:
            label
        }
    }

    /// Whether the action replaces what a person may have chosen, so a
    /// surface asks before it runs.
    var needsConfirmation: Bool {
        switch self {
        case .resetToFileMetadata, .clearMetadata: true
        case .importReady, .identify, .cancelIdentification, .cancelImport,
            .retryIdentification, .combine, .separate, .skip, .restore,
            .revealFolder:
            false
        }
    }

    var symbol: String {
        switch self {
        case .importReady: "square.and.arrow.down"
        case .identify: "magnifyingglass"
        case .cancelIdentification, .cancelImport: "xmark.circle"
        case .retryIdentification: "arrow.clockwise"
        case .resetToFileMetadata: "doc.text"
        case .clearMetadata: "eraser"
        case .combine: "square.stack.3d.up"
        case .separate: "square.split.1x2"
        case .skip: "minus.circle"
        case .restore: "arrow.uturn.backward"
        case .revealFolder: "folder"
        }
    }
}
