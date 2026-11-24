import BaeKit
import SwiftUI

/// The arrow a row draws after its title when its facts were read from a
/// catalog record — the same glyph the records row links out with.
///
/// An empty box of the same size when the facts came from anywhere else, so
/// the title line keeps its width whichever reading a row has, and nothing
/// reaches VoiceOver. It states a fact and answers nothing, so it takes no
/// hits and opens nothing.
struct RecordArrow: View {
    let readFromRecord: Bool

    /// The board draws the arrow 13 points across in the title's 16-point
    /// line.
    private static let width: CGFloat = 13
    private static let height: CGFloat = 16

    var body: some View {
        Group {
            if readFromRecord {
                Image(systemName: "arrow.up.right")
                    .font(.system(size: Self.width))
                    .foregroundStyle(Theme.textSecondary)
                    .accessibilityIdentifier("identified-glyph")
                    .accessibilityLabel(
                        localizedCoreString("core.identity.identified")
                    )
            }
            else {
                Color.clear
            }
        }
        .frame(width: Self.width, height: Self.height)
        .allowsHitTesting(false)
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
