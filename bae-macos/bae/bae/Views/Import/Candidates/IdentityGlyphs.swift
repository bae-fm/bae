import BaeKit
import SwiftUI

/// A candidate's seal states that an identifier lookup named the chosen
/// record. Rip verification belongs in the details behind it and in the pane.
struct IdentityGlyphs: View {
    let identifiedBy: BridgeMarkKind?
    let marks: [BridgeReleaseMark]
    let verification: BridgeVerification?
    let records: [BridgeReleaseRecord]

    @Environment(\.backgroundProminence)
    private var backgroundProminence

    var body: some View {
        Image(systemName: "seal")
            .font(.system(size: 11))
            .foregroundStyle(
                backgroundProminence == .increased
                    ? AnyShapeStyle(.primary) : AnyShapeStyle(.secondary)
            )
            .accessibilityIdentifier("identified-glyph")
            .accessibilityLabel(coreString("core.identity.identified"))
            .opacity(identifiedBy == nil ? 0 : 1)
            .allowsHitTesting(identifiedBy != nil)
            .accessibilityHidden(identifiedBy == nil)
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
