import BaeKit
import SwiftUI

/// The arrow a row draws after its title when its facts were read from a
/// catalog record — the same glyph the records row links out with.
///
/// Hidden rather than removed when the facts came from anywhere else, so the
/// title line keeps its width whichever reading a row has. It states a fact
/// and answers nothing, so it takes no hits and opens nothing.
struct RecordArrow: View {
    let reading: BridgeTriageReading

    private var readFromRecord: Bool {
        switch reading {
        case .identified: true
        case .prefilled, .unidentified: false
        }
    }

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

#if DEBUG

    // MARK: - Previews

    #Preview("Read from a record, and not") {
        VStack(alignment: .leading, spacing: 8) {
            RecordArrow(reading: .identified(records: []))
            RecordArrow(reading: .prefilled)
        }
        .padding()
        .windowBackground()
    }
#endif
