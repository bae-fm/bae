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
    let hasMatchedRelease: Bool

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
        hasMatchedRelease = candidate.pickedRelease != nil
    }

    init(candidate: Candidate, fileTags values: BridgeReleaseUserEdit) {
        title =
            values.albumTitle.isEmpty
            ? candidate.displayName : values.albumTitle
        titleIsPlaceholder = false
        let artistNames = values.albumArtistAssignments.map(\.displayName)
        artist =
            artistNames.isEmpty
            ? nil : ListFormatter.localizedString(byJoining: artistNames)
        factsLine = Self.factsLine([
            coreString("ui.import.metadata.from_file_tags"),
            String(localized: "\(values.tracks.count) tracks"),
        ])
        provenance = .fileTags
        sourceAudio = candidate.files.sourceAudio
        hasMatchedRelease = false
    }

    init?(row: BridgeTriageRow) {
        if let summary = row.metadataSummary {
            titleIsPlaceholder = summary.albumTitle.isEmpty
            title =
                summary.albumTitle.isEmpty ? "Album title" : summary.albumTitle
            let artistNames = summary.albumArtistAssignments.map(\.displayName)
            artist =
                artistNames.isEmpty
                ? nil : ListFormatter.localizedString(byJoining: artistNames)
            factsLine = ""
            provenance = nil
            sourceAudio = nil
            hasMatchedRelease = false
            return
        }
        guard let matched = row.matched else { return nil }
        title = matched.title
        titleIsPlaceholder = false
        artist = matched.artist
        if let pressing = matched.pressing {
            let trackText = pressing.trackCount.map {
                String(localized: "\(Int($0)) tracks")
            }
            factsLine = Self.factsLine([
                pressing.format,
                pressing.year.map {
                    Int($0).formatted(.number.grouping(.never))
                },
                trackText,
            ])
        }
        else {
            factsLine = ""
        }
        provenance = nil
        sourceAudio = nil
        hasMatchedRelease = false
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
            if let artist = summary.artist {
                Text(artist)
                    .font(style.artistFont)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            HStack(spacing: 6) {
                Text(summary.factsLine)
                    .font(.system(size: 11.5))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                if let provenance = summary.provenance {
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
            Link(destination: url) { capsule(label) }
                .buttonStyle(.plain)
        }
        else {
            capsule(label)
        }
    }

    private func capsule(_ label: String) -> some View {
        Text(verbatim: label)
            .font(.system(size: 10.5, weight: .medium))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(Color.secondary.opacity(0.15), in: Capsule())
            .foregroundStyle(.secondary)
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
