import BaeKit
import Foundation

extension BridgeCandidateAction {
    func label(count: Int) -> String {
        switch self {
        case .importReady: String(localized: "Import ready (\(count))")
        case .identify: String(localized: "Identify selected (\(count))")
        case .retryIdentification:
            String(localized: "Retry failed identification (\(count))")
        case .useFileMetadata: String(localized: "Use file metadata (\(count))")
        case .clearMetadata: String(localized: "Clear metadata (\(count))")
        case .skip: String(localized: "Skip selected (\(count))")
        case .restore: String(localized: "Restore to Pending (\(count))")
        }
    }

    var symbol: String {
        switch self {
        case .importReady: "square.and.arrow.down"
        case .identify: "magnifyingglass"
        case .retryIdentification: "arrow.clockwise"
        case .useFileMetadata: "doc.text"
        case .clearMetadata: "eraser"
        case .skip: "minus.circle"
        case .restore: "arrow.uturn.backward"
        }
    }
}
