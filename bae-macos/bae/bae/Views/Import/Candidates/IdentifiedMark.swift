import BaeKit
import SwiftUI

/// The mark a row's title carries when its draft was read from a source's
/// release. What it says is that the row is identified at all — as opposed to
/// filled in from the files' tags, or blank. Which sources, and what each of
/// them states, is the hover's to say.
///
/// The same glyph the main pane's source links carry, so the two read as one
/// gesture: this release came from somewhere you can go and look at.
struct IdentifiedMark: View {
    let sources: [BridgeIdentifiedSource]

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
                IdentifiedFromPopover(sources: sources)
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

/// Which sources a row's draft was read from, and what each of their releases
/// says about itself.
///
/// Both lines when the pick paired both sources: a MusicBrainz release and a
/// Discogs release describing one pressing can disagree about its label and
/// its year, so each line states its own source's document rather than the
/// draft they were merged into.
struct IdentifiedFromPopover: View {
    let sources: [BridgeIdentifiedSource]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(coreString("core.import.triage.identified_from").uppercased())
                .font(.system(size: 10, weight: .bold))
                .kerning(0.6)
                .foregroundStyle(.secondary)
                .padding(.bottom, 1)
            ForEach(sources, id: \.source) { source in
                IdentifiedSourceLine(source: source)
            }
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 12)
        .frame(width: 232)
    }
}

/// One source's line: its name, what its own release says, and the way to that
/// release's page there.
struct IdentifiedSourceLine: View {
    let source: BridgeIdentifiedSource

    var body: some View {
        HStack(spacing: 6) {
            Text(verbatim: bridgeMetadataSourceName(source: source.source))
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.primary)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let facts {
                // What the source says is why the line is here, so it keeps
                // its width and the source's name yields — the name is a
                // brand the reader already knows.
                Text(verbatim: facts)
                    .font(.system(size: 10.5, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .fixedSize()
            }
            if let url = URL(string: source.url) {
                Link(destination: url) {
                    Image(systemName: "arrow.up.right")
                        .font(.system(size: 10))
                        .foregroundStyle(Theme.accent)
                }
                .buttonStyle(.plain)
            }
        }
    }

    /// The label and the year this source's own release states, whichever of
    /// them it states. `nil` when it states neither — the line is then the
    /// source's name and the way to it.
    private var facts: String? {
        let stated = [source.label, source.year.map(String.init)]
            .compactMap { $0 }
        return stated.isEmpty ? nil : stated.joined(separator: " \u{00b7} ")
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Identified from") {
        IdentifiedFromPopover(sources: PreviewData.identifiedFromBothSources)
            .importPreviewEnvironment()
    }
#endif
