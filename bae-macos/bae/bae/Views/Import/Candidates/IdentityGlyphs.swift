import BaeKit
import SwiftUI

/// The two facts a candidate row states about its release, each as its own
/// glyph after the title: a seal when a name read off the folder tied its
/// files to the record its draft came from, a check when the rip databases
/// found other copies of the disc carrying the same audio.
///
/// Independent — either holds without the other — because they answer
/// different questions: one is about the record, the other about the bits.
/// Core decides both; nothing here derives either from the lines behind them.
/// Neither word is ever drawn: hovering opens the card that says how.
struct IdentityGlyphs: View {
    /// Which name tied the files to the record, or `nil` when nothing did.
    /// The kind is not drawn — the mark lines in the card say it.
    let identifiedBy: BridgeMarkKind?
    let verified: Bool
    let marks: [BridgeReleaseMark]
    let verification: BridgeVerification?
    let records: [BridgeReleaseRecord]

    @Environment(\.backgroundProminence)
    private var backgroundProminence

    @ViewBuilder
    var body: some View {
        if identifiedBy != nil || verified {
            HStack(spacing: 4) {
                if identifiedBy != nil {
                    glyph(
                        "seal",
                        weight: .regular,
                        tint: AnyShapeStyle(.secondary),
                        identifier: "identified-glyph",
                        label: coreString("core.identity.identified")
                    )
                }
                if verified {
                    glyph(
                        "checkmark",
                        weight: .semibold,
                        tint: AnyShapeStyle(Color.green),
                        identifier: "verified-glyph",
                        label: coreString("core.identity.verified")
                    )
                }
            }
            // The title truncates first: a glyph is the row's whole answer and
            // clipping it would lose that, where a clipped title still reads.
            .fixedSize()
            .hoverPopover(arrowEdge: .bottom) {
                ReleaseFactsPopover(
                    marks: marks,
                    verification: verification,
                    records: records
                )
                .popoverEntrance(anchor: .top)
                .background { PopoverBehavior() }
            }
        }
    }

    /// Each glyph keeps its own colour, except on the selected row it sits in:
    /// there the whole text column goes white, and a glyph that kept its
    /// colour would be the one thing that did not follow.
    private func glyph(
        _ systemName: String,
        weight: Font.Weight,
        tint: AnyShapeStyle,
        identifier: String,
        label: String
    ) -> some View {
        Image(systemName: systemName)
            .font(.system(size: 11, weight: weight))
            .foregroundStyle(
                backgroundProminence == .increased
                    ? AnyShapeStyle(.primary) : tint
            )
            .accessibilityIdentifier(identifier)
            .accessibilityLabel(label)
    }
}
