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
        let count = candidate.mapping.trackMappings.count
        let trackText = String(localized: "\(count) tracks")
        switch provenance {
        case .externalRelease:
            factsLine = Self.factsLine([
                PressingText.media(values.pressing.facts.media),
                values.pressing.year,
                values.pressing.facts.area?.text,
                // One line has room for one number: the first label's.
                values.pressing.labels.first { !$0.catalogNumber.isEmpty }?
                    .catalogNumber,
                trackText,
            ])
        case .fileMetadata:
            factsLine = Self.factsLine([
                coreString("core.import.metadata.file_metadata"), trackText,
            ])
        case nil:
            factsLine = trackText
        }
        sourceAudio = candidate.files.sourceAudio
    }

    /// The row's applied draft; `nil` for an `unidentified` row, which has
    /// none.
    init?(row: BridgeTriageRow) {
        guard let summary = row.metadataSummary else { return nil }
        titleIsPlaceholder = summary.albumTitle.isEmpty
        title = summary.albumTitle.isEmpty ? "Album title" : summary.albumTitle
        let artistNames = summary.albumArtistAssignments.map(\.displayName)
        artist =
            artistNames.isEmpty
            ? nil : ListFormatter.localizedString(byJoining: artistNames)
        factsLine = ""
        sourceAudio = nil
    }

    /// A Done row's library release: its title, and its artist beside its
    /// year on the line under it.
    init(release: BridgeImportedReleaseSummary) {
        titleIsPlaceholder = release.title.isEmpty
        title = release.title.isEmpty ? "Album title" : release.title
        let byline = Self.factsLine([
            release.artist, release.year.map { String($0) },
        ])
        artist = byline.isEmpty ? nil : byline
        factsLine = ""
        sourceAudio = nil
    }

    private static func factsLine(_ facts: [String?]) -> String {
        facts.compactMap { $0?.isEmpty == false ? $0 : nil }
            .joined(separator: " \u{00b7} ")
    }

}

/// An import release summary, scaled for the sidebar or the card, with the
/// caller's accessory after the title.
struct ImportReleaseSummaryView<TitleAccessory: View>: View {
    enum Style {
        case sidebar
        case card
    }

    let summary: ImportReleaseSummary
    let style: Style
    @ViewBuilder
    let titleAccessory: () -> TitleAccessory

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            titleLine
            artistLine
            Text(summary.factsLine)
                .themeText(.detail)
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .padding(.top, style.factsTopPadding)
                .frame(height: style.factsHeight)
                .opacity(style.showsFacts ? 1 : 0)
            if style.showsFacts, let sourceAudio = summary.sourceAudio {
                ImportSourceAudioSummaryView(sourceAudio: sourceAudio)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// The title and the caller's accessory; the title truncates first.
    private var titleLine: some View {
        HStack(spacing: ThemeSpace.compact) {
            Text(summary.title)
                .themeText(style.titleText)
                .foregroundStyle(
                    summary.titleIsPlaceholder ? .secondary : .primary
                )
                .lineLimit(1)
                .truncationMode(style.titleTruncation)
            titleAccessory()
                .layoutPriority(1)
        }
    }

    @ViewBuilder
    private var artistLine: some View {
        if let artist = summary.artist {
            Text(artist)
                .themeText(style.artistText)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }

}

extension ImportReleaseSummaryView where TitleAccessory == EmptyView {
    init(summary: ImportReleaseSummary, style: Style) {
        self.init(summary: summary, style: style) { EmptyView() }
    }
}

extension ImportReleaseSummaryView.Style {
    fileprivate var titleText: ThemeText {
        switch self {
        case .sidebar: .rowTitle
        case .card: .heading
        }
    }

    fileprivate var artistText: ThemeText {
        switch self {
        case .sidebar: .detail
        case .card: .body
        }
    }

    fileprivate var factsTopPadding: CGFloat {
        switch self {
        case .sidebar: ThemeSpace.hairline
        case .card: ThemeSpace.inline
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
            .themeText(.detail)
            .foregroundStyle(.tertiary)
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityLabel(coreString("core.audio.label"))
            .accessibilityValue(sourceAudio.summary.text)
    }
}
