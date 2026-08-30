import BaeKit
import SwiftUI

/// The release identity shown throughout import: title, artist, release facts,
/// and the source-audio facts observed by the scan.
struct ImportReleaseSummary {
    let title: String
    let titleIsPlaceholder: Bool
    let artist: String?
    let factsLine: String
    let sourceAudioLine: String?
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
        sourceAudioLine = candidate.files.sourceAudio?.text
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
        sourceAudioLine = candidate.files.sourceAudio?.text
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
            sourceAudioLine = nil
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
        sourceAudioLine = nil
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
                    sourceChip(provenance)
                }
            }
            .padding(.top, style.factsTopPadding)
            .frame(height: style.factsHeight)
            .opacity(style.showsFacts ? 1 : 0)
            Text(summary.sourceAudioLine ?? "")
                .font(.system(size: 11.5))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(height: style.sourceAudioHeight)
                .opacity(
                    style.showsFacts && summary.sourceAudioLine != nil ? 1 : 0
                )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func sourceChip(
        _ provenance: BridgeMetadataProvenance
    ) -> some View {
        Group {
            if let url = provenance.externalReleaseURL {
                Link(destination: url) {
                    chipLabel(provenance.label)
                }
                .buttonStyle(.plain)
            }
            else {
                chipLabel(provenance.label)
            }
        }
    }

    private func chipLabel(_ label: String) -> some View {
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

    fileprivate var sourceAudioHeight: CGFloat? {
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
