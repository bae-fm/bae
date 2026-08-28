import BaeKit
import SwiftUI

/// The three lines that identify a release wherever import presents one.
/// Sources differ, but the presentation does not: title, artist, then the
/// pressing facts joined for the current locale.
struct ImportReleaseSummary {
    let title: String
    let artist: String?
    let factsLine: String
    let source: BridgeMetadataSource?

    init(candidate: Candidate, editValues values: BridgeRawReleaseEdit) {
        title =
            values.albumTitle.isEmpty
            ? candidate.displayName : values.albumTitle
        let albumArtists = values.albumArtistAssignments.editorText
        artist = albumArtists.isEmpty ? nil : albumArtists
        let count = candidate.mapping.willWriteCount
        let trackText = String(localized: "\(count) tracks")
        let lead =
            candidate.identity == .unknown
            ? [coreString("ui.import.metadata.from_file_tags")]
            : [
                values.pressing.format,
                values.pressing.year,
                values.pressing.country,
                values.pressing.catalogNumber,
            ]
        factsLine = Self.factsLine(lead + [trackText])
        source = candidate.pickedRelease?.source
    }

    init?(row: BridgeTriageRow) {
        guard let matched = row.matched else { return nil }
        title = matched.title
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
        source = nil
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
                if let source = summary.source {
                    sourceChip(source)
                }
            }
            .padding(.top, style.factsTopPadding)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func sourceChip(_ source: BridgeMetadataSource) -> some View {
        Text(verbatim: bridgeMetadataSourceName(source: source))
            .font(.system(size: 10.5, weight: .medium))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(Color.secondary.opacity(0.15), in: Capsule())
            .foregroundStyle(.secondary)
            .lineLimit(1)
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

    fileprivate var titleTruncation: Text.TruncationMode {
        switch self {
        case .sidebar: .middle
        case .card: .tail
        }
    }
}
