import BaeKit
import SwiftUI

/// The mark a row's title carries when its draft was read from a catalog's
/// release. What it says is that the row is identified at all — as opposed to
/// filled in from the files' tags, or blank. Which catalogs describe the
/// release is the hover's to say.
///
/// The same glyph the records row's links carry, so the two read as one
/// gesture: this release came from somewhere you can go and look at.
struct IdentifiedMark: View {
    let records: [BridgeReleaseRecord]

    @Environment(\.backgroundProminence)
    private var backgroundProminence

    var body: some View {
        Image(systemName: "arrow.up.right")
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(tint)
            .fixedSize()
            .accessibilityIdentifier("identified-mark")
            .accessibilityLabel(coreString("core.import.triage.identified"))
            .hoverPopover(arrowEdge: .bottom) {
                IdentifiedFromPopover(records: records)
                    .popoverEntrance(anchor: .top)
                    .background { PopoverBehavior() }
            }
    }

    /// Green, except on the selected row it sits in: there the whole text
    /// column goes white, and a glyph that kept its colour would be the one
    /// thing that did not follow.
    private var tint: AnyShapeStyle {
        backgroundProminence == .increased
            ? AnyShapeStyle(.primary) : AnyShapeStyle(Color.green)
    }
}

/// Which catalogs describe the release this row's draft was read from, each
/// linking to its own page for it.
struct IdentifiedFromPopover: View {
    let records: [BridgeReleaseRecord]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(coreString("core.import.triage.identified_from").uppercased())
                .font(.system(size: 10, weight: .bold))
                .kerning(0.6)
                .foregroundStyle(.secondary)
                .padding(.bottom, 1)
            ReleaseRecordsRow(records: records)
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 12)
        .frame(width: 300)
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Identified from") {
        IdentifiedFromPopover(records: PreviewData.releaseRecordsPair)
            .importPreviewEnvironment()
    }
#endif
