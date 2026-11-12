import BaeKit
import SwiftUI

/// The arrow a row draws after its title when its facts were read from a
/// catalog record — the same glyph the records row links out with.
///
/// Hidden rather than removed when the facts came from anywhere else, so the
/// title line keeps its width whichever reading a row has. It states a fact
/// and answers nothing, so it takes no hits and opens nothing.
struct RecordArrow: View {
    let readFromRecord: Bool

    var body: some View {
        Image(systemName: "arrow.up.right")
            .themeIcon(.small)
            .foregroundStyle(.secondary)
            .accessibilityIdentifier("identified-glyph")
            .accessibilityLabel(localizedCoreString("core.identity.identified"))
            .opacity(readFromRecord ? 1 : 0)
            .allowsHitTesting(false)
            .accessibilityHidden(!readFromRecord)
            .fixedSize()
    }
}

extension BridgeTriageReading {
    /// Whether the candidate is linked to a catalog's pressing or album.
    var readFromRecord: Bool {
        switch self {
        case .identified, .identifiedAlbum: true
        case .prefilled, .unidentified: false
        }
    }
}

#if DEBUG

    // MARK: - Previews

#endif
