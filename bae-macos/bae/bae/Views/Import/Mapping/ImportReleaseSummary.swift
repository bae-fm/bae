import BaeKit
import SwiftUI

/// The release identity shown throughout import: title, artist, release facts,
/// and the source-audio facts observed by the scan.
struct ImportReleaseSummary {
    let title: String
    let titleIsPlaceholder: Bool
    let artist: String?
    let factsLine: String
    let contextLine: String?
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
        contextLine = trackText
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
        contextLine = String(localized: "\(values.tracks.count) tracks")
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
            contextLine = nil
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
            contextLine = trackText
        }
        else {
            factsLine = ""
            contextLine = nil
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
                    ImportMetadataProvenanceChip(provenance: provenance)
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

/// The non-editable context that remains beside the album fields: how many
/// tracks the draft maps and which metadata source supplied it. Pressing values
/// live under Details; source audio sits below the cover.
struct ImportReleaseContextView: View {
    let summary: ImportReleaseSummary

    @ViewBuilder
    var body: some View {
        if summary.contextLine != nil || summary.provenance != nil {
            HStack(spacing: 6) {
                if let contextLine = summary.contextLine {
                    Text(contextLine)
                        .font(.system(size: 11.5))
                        .foregroundStyle(.tertiary)
                }
                if let provenance = summary.provenance {
                    ImportMetadataProvenanceChip(provenance: provenance)
                }
            }
        }
    }
}

private struct ImportMetadataProvenanceChip: View {
    let provenance: BridgeMetadataProvenance

    var body: some View {
        Group {
            if let url = provenance.externalReleaseURL {
                Link(destination: url) {
                    label
                }
                .buttonStyle(.plain)
            }
            else {
                label
            }
        }
    }

    private var label: some View {
        Text(verbatim: provenance.label)
            .font(.system(size: 10.5, weight: .medium))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(Color.secondary.opacity(0.15), in: Capsule())
            .foregroundStyle(.secondary)
            .lineLimit(1)
    }
}

extension BridgeMetadataProvenance {
    fileprivate var label: String {
        switch self {
        case .externalRelease(let source, _):
            bridgeMetadataSourceName(source: source)
        case .fileTags:
            coreString("ui.import.metadata.file_tags")
        }
    }

    fileprivate var externalReleaseURL: URL? {
        guard case .externalRelease(let source, let releaseId) = self else {
            return nil
        }
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
        case .sidebar: 0
        case .card: 2
        }
    }

    fileprivate var titleFont: Font {
        switch self {
        case .sidebar: .system(size: 14, weight: .semibold)
        case .card: .system(size: 17, weight: .semibold)
        }
    }

    fileprivate var artistFont: Font {
        switch self {
        case .sidebar: .system(size: 12.5)
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

/// The candidate's aggregate source-audio facts as one non-interactive line.
struct ImportSourceAudioSummaryView: View {
    let sourceAudio: BridgeCandidateSourceAudio

    var body: some View {
        Text(sourceAudio.summary.text)
            .font(.system(size: 11.5))
            .foregroundStyle(.tertiary)
            .lineLimit(1)
            .truncationMode(.tail)
            .accessibilityLabel(coreString("core.audio.label"))
            .accessibilityValue(sourceAudio.summary.text)
    }
}
