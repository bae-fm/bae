import BaeKit
import SwiftUI

/// The release identity shown throughout import: title, artist, release facts,
/// and the source-audio facts observed by the scan.
struct ImportReleaseSummary {
    let title: String
    let titleIsPlaceholder: Bool
    let artist: String?
    let factsLine: String
    let sourceAudio: BridgeCandidateSourceAudio?
    let provenance: BridgeMetadataProvenance?

    init(candidate: Candidate, editValues values: BridgeRawReleaseEdit) {
        let provenance = candidate.metadataProvenance
        titleIsPlaceholder = values.albumTitle.isEmpty
        title =
            if values.albumTitle.isEmpty {
                "Album title"
            }
            else {
                values.albumTitle
            }
        let artistNames = values.albumArtistAssignments.map(\.displayName)
        artist =
            artistNames.isEmpty
            ? nil : ListFormatter.localizedString(byJoining: artistNames)
        let count = candidate.mapping.willWriteCount
        let trackText = String(localized: "\(count) tracks")
        switch provenance {
        case .externalRelease:
            factsLine = Self.factsLine([
                values.pressing.format,
                values.pressing.year,
                values.pressing.country,
                values.pressing.catalogNumber,
                trackText,
            ])
        case .fileTags:
            factsLine = Self.factsLine([
                coreString("ui.import.metadata.from_file_tags"), trackText,
            ])
        case nil:
            factsLine = trackText
        }
        self.provenance = provenance
        sourceAudio = candidate.files.sourceAudio
    }

    /// The row's applied draft. `nil` for a row that has none, which is the
    /// `unidentified` reading — that row draws its folder instead of a
    /// release.
    init?(row: BridgeTriageRow) {
        guard let summary = row.metadataSummary else { return nil }
        titleIsPlaceholder = summary.albumTitle.isEmpty
        title = summary.albumTitle.isEmpty ? "Album title" : summary.albumTitle
        let artistNames = summary.albumArtistAssignments.map(\.displayName)
        artist =
            artistNames.isEmpty
            ? nil : ListFormatter.localizedString(byJoining: artistNames)
        factsLine = ""
        provenance = row.metadataProvenance
        sourceAudio = nil
    }

    private static func factsLine(_ facts: [String?]) -> String {
        facts.compactMap { $0?.isEmpty == false ? $0 : nil }
            .joined(separator: " \u{00b7} ")
    }

}

/// One rendering of an import release summary, scaled for its two homes.
struct ImportReleaseSummaryView: View {
    enum Style {
        case sidebar
        case card
    }

    let summary: ImportReleaseSummary
    let style: Style

    var body: some View {
        VStack(alignment: .leading, spacing: style.stackSpacing) {
            Text(summary.title)
                .font(style.titleFont)
                .foregroundStyle(
                    summary.titleIsPlaceholder ? .secondary : .primary
                )
                .lineLimit(1)
                .truncationMode(style.titleTruncation)
            artistLine
            HStack(spacing: 6) {
                Text(summary.factsLine)
                    .font(.system(size: 11.5))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                // Only the style that shows this line builds its chips: the
                // other draws it at zero height and full transparency, where
                // a link would still take clicks off the row under it.
                if style.showsFacts, let provenance = summary.provenance {
                    ImportMetadataProvenanceChips(provenance: provenance)
                }
            }
            .padding(.top, style.factsTopPadding)
            .frame(height: style.factsHeight)
            .opacity(style.showsFacts ? 1 : 0)
            if style.showsFacts, let sourceAudio = summary.sourceAudio {
                ImportSourceAudioSummaryView(sourceAudio: sourceAudio)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private var artistLine: some View {
        if let artist = summary.artist {
            Text(artist)
                .font(style.artistFont)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }

}

/// The metadata source attached to the editable album identity. Pressing values
/// live under Release; source audio sits below the cover.
struct ImportReleaseContextView: View {
    let summary: ImportReleaseSummary

    var body: some View {
        if let provenance = summary.provenance {
            ImportMetadataProvenanceChips(provenance: provenance)
        }
    }
}

/// Every source this metadata claims, one chip each. A pick pairs a
/// MusicBrainz release and a Discogs release into one pressing, and both are
/// the release's, so both are named — the one the draft was read from first,
/// each linking to its own release page.
private struct ImportMetadataProvenanceChips: View {
    let provenance: BridgeMetadataProvenance

    var body: some View {
        HStack(spacing: 4) {
            switch provenance {
            case .externalRelease:
                ForEach(provenance.releaseRefs, id: \.source) { release in
                    chip(
                        label: bridgeMetadataSourceName(source: release.source),
                        url: release.releaseURL
                    )
                }
            case .fileTags:
                chip(
                    label: coreString("ui.import.metadata.file_tags"),
                    url: nil
                )
            }
        }
    }

    @ViewBuilder
    private func chip(label: String, url: URL?) -> some View {
        if let url {
            Link(destination: url) {
                HStack(spacing: 3) {
                    MetadataSourceCapsule(label: label)
                    Image(systemName: "arrow.up.right")
                        .font(.system(size: 9))
                }
                .foregroundStyle(Theme.accent)
            }
            .buttonStyle(.plain)
        }
        else {
            MetadataSourceCapsule(label: label).foregroundStyle(.secondary)
        }
    }
}

/// One metadata source's name, in a capsule. The draft header wraps it in a
/// link to the release it names; a candidate row draws the same capsule with
/// no link — so the text colour is the caller's, which is what tints a link's
/// whole chip.
struct MetadataSourceCapsule: View {
    let label: String

    var body: some View {
        Text(verbatim: label)
            .font(.system(size: 10.5, weight: .medium))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(Color.secondary.opacity(0.15), in: Capsule())
            .lineLimit(1)
    }
}

extension BridgeMetadataProvenance {
    /// The releases this provenance names — the one the draft was read from,
    /// then each partner the pick carried. Empty for File Tags, which names
    /// no external release.
    var releaseRefs: [BridgeMetadataRef] {
        switch self {
        case .externalRelease(let source, let releaseId, let partners):
            [BridgeMetadataRef(source: source, releaseId: releaseId)]
                + partners
        case .fileTags:
            []
        }
    }
}

extension BridgeMetadataRef {
    fileprivate var releaseURL: URL? {
        let root =
            switch source {
            case .musicBrainz: URL(string: "https://musicbrainz.org/release")
            case .discogs: URL(string: "https://www.discogs.com/release")
            }
        return root?.appending(path: releaseId)
    }
}

extension ImportReleaseSummaryView.Style {
    fileprivate var stackSpacing: CGFloat {
        switch self {
        case .sidebar: 3
        case .card: 2
        }
    }

    fileprivate var titleFont: Font {
        switch self {
        case .sidebar: .system(size: 13, weight: .semibold)
        case .card: .system(size: 17, weight: .semibold)
        }
    }

    fileprivate var artistFont: Font {
        switch self {
        case .sidebar: .system(size: 11.5)
        case .card: .system(size: 13)
        }
    }

    fileprivate var factsTopPadding: CGFloat {
        switch self {
        case .sidebar: 1
        case .card: 4
        }
    }

    fileprivate var showsFacts: Bool {
        switch self {
        case .sidebar: false
        case .card: true
        }
    }

    fileprivate var factsHeight: CGFloat? {
        switch self {
        case .sidebar: 0
        case .card: nil
        }
    }

    fileprivate var titleTruncation: Text.TruncationMode {
        switch self {
        case .sidebar: .middle
        case .card: .tail
        }
    }
}

/// The candidate's aggregate source-audio facts as non-interactive text.
struct ImportSourceAudioSummaryView: View {
    let sourceAudio: BridgeCandidateSourceAudio

    var body: some View {
        Text(sourceAudio.summary.text)
            .font(.system(size: 11.5))
            .foregroundStyle(.tertiary)
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityLabel(coreString("core.audio.label"))
            .accessibilityValue(sourceAudio.summary.text)
    }
}
