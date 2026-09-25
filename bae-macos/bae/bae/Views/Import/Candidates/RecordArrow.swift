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
            .font(.system(size: 11))
            .foregroundStyle(.secondary)
            .accessibilityIdentifier("identified-glyph")
            .accessibilityLabel(coreString("core.identity.identified"))
            .opacity(readFromRecord ? 1 : 0)
            .allowsHitTesting(false)
            .accessibilityHidden(!readFromRecord)
            .fixedSize()
    }
}

extension BridgeTriageReading {
    /// Whether the candidate's draft was read from a catalog's release.
    var readFromRecord: Bool {
        switch self {
        case .identified: true
        case .prefilled, .unidentified: false
        }
    }
}

extension BridgeImportedReleaseSummary {
    /// Whether a catalog describes the library release.
    var readFromRecord: Bool { !records.isEmpty }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Read from a record, and not") {
        VStack(alignment: .leading, spacing: 8) {
            RecordArrow(readFromRecord: true)
            RecordArrow(readFromRecord: false)
        }
        .padding()
        .windowBackground()
    }
#endif
