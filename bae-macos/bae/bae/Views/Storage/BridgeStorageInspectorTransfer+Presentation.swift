import BaeKit
import SwiftUI

extension BridgeStorageInspectorTransfer {
    var queueId: Int {
        switch self {
        case .download: 0
        case .output: 1
        case .upload: 2
        }
    }

    var title: LocalizedStringKey {
        switch self {
        case .download: "Downloads"
        case .output: "Export & Save"
        case .upload: "Sync queue"
        }
    }

    var icon: String {
        switch self {
        case .download: "arrow.down.circle"
        case .output: "square.and.arrow.up"
        case .upload: "arrow.up.arrow.down.circle"
        }
    }

    /// What the section's Cancel All abandons: the whole queue, not only the
    /// selected release.
    var cancelAllHelp: LocalizedStringKey {
        switch self {
        case .download: "Cancel every download, not only this release's"
        case .output: "Cancel every export, not only this release's"
        case .upload:
            "Stop every upload that can still be stopped, not only this release's"
        }
    }

    var pauseRequested: Bool {
        switch self {
        case .download(_, let paused), .output(_, let paused),
            .upload(_, let paused):
            paused
        }
    }

}
