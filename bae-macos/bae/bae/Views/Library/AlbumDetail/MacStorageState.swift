import BaeKit
import SwiftUI

/// A release's storage at rest: the word Local, Pinned or In cloud,
/// with no glyph.
struct MacStorageState: View {
    let state: BridgeReleaseStorageState

    var body: some View {
        Group {
            switch state {
            case .onOneDevice:
                Text("Local")
            case .inStorage(pinned: true):
                Text("Pinned")
            case .inStorage(pinned: false):
                Text("In cloud")
            }
        }
        .themeText(.body)
        .foregroundStyle(Theme.macTextSecondary)
    }
}
