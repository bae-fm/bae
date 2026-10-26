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
            values.albumTitle.isEmpty
            ? Self.placeholderTitle : values.albumTitle
        artist =
            values.albumArtistAssignments.isEmpty
            ? nil : bridgeCreditLine(credits: values.albumArtistAssignments)
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
        title =
            summary.albumTitle.isEmpty
            ? Self.placeholderTitle : summary.albumTitle
        artist = summary.albumArtists.isEmpty ? nil : summary.albumArtists
        factsLine = ""
        sourceAudio = nil
    }

    /// A folder that failed validation, as its tags call the release: the
    /// album title, and the artist under it.
    init(tags: BridgeSkippedTags) {
        titleIsPlaceholder = false
        title = tags.albumTitle
        artist = tags.artist
        factsLine = ""
        sourceAudio = nil
    }

    /// The title shown in place of a release's empty album title.
    private static var placeholderTitle: String {
        String(localized: "Album title")
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
