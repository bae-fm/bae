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

    var pauseRequested: Bool {
        switch self {
        case .download(_, let paused), .output(_, let paused),
            .upload(_, let paused):
            paused
        }
    }

}
