import BaeKit
import SwiftUI

/// A release's storage at rest: the glyph of where it is kept and the word
/// Local, Pinned or In cloud.
struct MacStorageState: View {
    let state: BridgeReleaseStorageState

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            switch state {
            case .onOneDevice:
                Image(systemName: "folder").themeIcon(.medium)
                    .accessibilityHidden(true)
                Text("Local")
            case .inStorage(pinned: true):
                Image(systemName: "pin").themeIcon(.medium)
                    .accessibilityHidden(true)
                Text("Pinned")
            case .inStorage(pinned: false):
                Image(systemName: "cloud").themeIcon(.medium)
                    .accessibilityHidden(true)
                Text("In cloud")
            }
        }
        .themeText(.body)
        .foregroundStyle(.secondary)
    }
}
